use super::*;

pub(super) fn reuse_policy_for(build: &ModuleBuildConfig) -> BuildReusePolicy {
    // Credential-only secrets authenticate fetching inputs and do not change
    // the image, so they never block reuse.
    if build
        .secrets
        .keys()
        .all(|id| build.credential_only_secrets.contains(id))
    {
        return BuildReusePolicy::Allowed;
    }
    // Build planning has no atomic read+resolve primitive. Reuse is therefore
    // disabled for every secret-backed build rather than inferred from a provider name.
    BuildReusePolicy::Forbidden
}

pub(super) fn unit_predecessors(unit: &ImageBuildUnitPlan) -> impl Iterator<Item = &str> {
    unit.dependencies
        .iter()
        .map(|dependency| dependency.producer_module.as_str())
}

pub(super) fn topological_build_units(
    units: Vec<ImageBuildUnitPlan>,
) -> Result<Vec<ImageBuildUnitPlan>, MmError> {
    let index = units
        .iter()
        .enumerate()
        .map(|(position, unit)| (unit.module.as_str(), position))
        .collect::<BTreeMap<_, _>>();
    let mut remaining = units
        .iter()
        .map(|unit| {
            unit_predecessors(unit)
                .filter(|module| index.contains_key(module))
                .count()
        })
        .collect::<Vec<_>>();
    let mut ready = remaining
        .iter()
        .enumerate()
        .filter(|(_, count)| **count == 0)
        .map(|(position, _)| position)
        .collect::<BTreeSet<_>>();
    let mut dependents = vec![Vec::new(); units.len()];
    for (position, unit) in units.iter().enumerate() {
        for predecessor in unit_predecessors(unit) {
            let Some(upstream) = index.get(predecessor) else {
                continue;
            };
            dependents[*upstream].push(position);
        }
    }
    let mut ordered = Vec::with_capacity(units.len());
    while let Some(position) = ready.pop_first() {
        ordered.push(position);
        for dependent in &dependents[position] {
            remaining[*dependent] -= 1;
            if remaining[*dependent] == 0 {
                ready.insert(*dependent);
            }
        }
    }
    if ordered.len() != units.len() {
        return Err(MmError::Build {
            message: format!(
                "module build dependencies form a cycle: {}",
                cycle_path(&units, &index)
            ),
        });
    }
    Ok(ordered
        .into_iter()
        .map(|position| units[position].clone())
        .collect())
}

pub(super) fn cycle_path(units: &[ImageBuildUnitPlan], index: &BTreeMap<&str, usize>) -> String {
    let mut visiting = vec![false; units.len()];
    let mut stack = Vec::new();
    fn walk(
        position: usize,
        units: &[ImageBuildUnitPlan],
        index: &BTreeMap<&str, usize>,
        visiting: &mut [bool],
        stack: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        if visiting[position] {
            let start = stack
                .iter()
                .position(|module| module == &units[position].module)
                .unwrap_or(0);
            let mut path = stack[start..].to_vec();
            path.push(units[position].module.clone());
            return Some(path);
        }
        visiting[position] = true;
        stack.push(units[position].module.clone());
        for predecessor in unit_predecessors(&units[position]) {
            if let Some(upstream) = index.get(predecessor)
                && let Some(path) = walk(*upstream, units, index, visiting, stack)
            {
                return Some(path);
            }
        }
        stack.pop();
        visiting[position] = false;
        None
    }
    for position in 0..units.len() {
        if let Some(path) = walk(position, units, index, &mut visiting, &mut stack) {
            return path.join(" -> ");
        }
    }
    "unknown cycle".to_owned()
}

pub(super) fn infer_first_party_bases(
    root: &Path,
    units: &mut [ImageBuildUnitPlan],
) -> Result<(), MmError> {
    let mut producers = BTreeMap::<String, Vec<String>>::new();
    for unit in units.iter() {
        producers
            .entry(unit.image.clone())
            .or_default()
            .push(unit.module.clone());
    }
    for unit in units.iter_mut() {
        let dockerfile = root.join(&unit.dockerfile);
        let source = read_dockerfile(&dockerfile)?;
        let parsed = parse_dockerfile(&source, &dockerfile.display().to_string())?;
        for base in parsed.bases {
            let DockerfileBaseKind::Literal { repository } = base.kind else {
                continue;
            };
            let Some(candidates) = producers.get(&repository) else {
                continue;
            };
            let candidates = candidates.clone();
            if candidates.len() == 1 {
                unit.dependencies.push(FirstPartyBuildEdge {
                    producer_module: candidates[0].clone(),
                    dependent_module: unit.module.clone(),
                    stage: base.stage,
                    reference: BuildDependencyReference::Dockerfile {
                        original_reference: base.original_reference,
                        instruction_index: base.instruction_index,
                        source_line: base.source_line,
                    },
                });
            } else {
                return Err(MmError::Build {
                    message: format!(
                        "{}:{}: image '{}' referenced by module '{}' is produced by more than one first-party module ({}); use a unique repository or an explicit build.depends-on mapping",
                        dockerfile.display(),
                        base.source_line,
                        base.original_reference,
                        unit.module,
                        candidates.join(", ")
                    ),
                });
            }
        }
        unit.dependencies.sort();
    }
    Ok(())
}

pub(super) fn assign_canonical_fingerprints(
    units: &mut [ImageBuildUnitPlan],
    platforms: &[String],
) -> Result<(), MmError> {
    let mut fingerprints = BTreeMap::<String, String>::new();
    for unit in units.iter_mut() {
        unit.upstream_node_ids = unit
            .dependencies
            .iter()
            .map(|dependency| {
                fingerprints
                    .get(&dependency.producer_module)
                    .cloned()
                    .ok_or_else(|| MmError::Build {
                        message: format!(
                            "module '{}' depends on unknown module '{}'",
                            unit.module, dependency.producer_module
                        ),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        unit.input_fingerprint = unit_input_fingerprint(unit, platforms)
            .map_err(|message| MmError::Build { message })?;
        fingerprints.insert(unit.module.clone(), unit.input_fingerprint.clone());
    }
    Ok(())
}
