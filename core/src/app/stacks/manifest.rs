use super::*;

pub(super) fn router_name(
    document: &ConfigDocument,
    environment: &str,
    stack: &str,
    route: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hash_field(&mut hasher, &document.config.project.id.0.to_string());
    hash_field(&mut hasher, environment);
    hash_field(&mut hasher, stack);
    hash_field(&mut hasher, route);
    format!("mm-{}", &hasher.finalize().to_hex()[..20])
}

pub(super) fn route_claims(stacks: &[PreparedStackArtifact]) -> Vec<IngressRouteClaim> {
    stacks
        .iter()
        .flat_map(|stack| {
            stack.manifest.routes.iter().map(|route| IngressRouteClaim {
                target: stack.manifest.target.clone(),
                target_id: stack.manifest.target_id,
                project_id: stack.manifest.project_id,
                project_name: stack.manifest.project_name.clone(),
                environment: stack.manifest.environment.clone(),
                stack: stack.manifest.stack.clone(),
                route: route.name.clone(),
                host: route.host.clone(),
                path_prefix: route.path_prefix.clone(),
                service: route.service.clone(),
                port: route.port,
                tls: route.tls,
                provenance: route.provenance,
                certificate_reference: route.certificate_reference.clone(),
                ownership: route.ownership.clone(),
                priority: route.priority,
                router_name: route.router_name.clone(),
                backend_alias: route.backend_alias.clone(),
                artifact_hash: stack.manifest.artifact_hash.clone(),
                middlewares: route.middlewares.clone(),
            })
        })
        .collect()
}

pub(super) fn compiled_route_claims(stacks: &[CompiledStackArtifact]) -> Vec<IngressRouteClaim> {
    stacks
        .iter()
        .flat_map(|stack| {
            stack.manifest.routes.iter().map(|route| IngressRouteClaim {
                target: stack.manifest.target.clone(),
                target_id: stack.manifest.target_id,
                project_id: stack.manifest.project_id,
                project_name: stack.manifest.project_name.clone(),
                environment: stack.manifest.environment.clone(),
                stack: stack.manifest.stack.clone(),
                route: route.name.clone(),
                host: route.host.clone(),
                path_prefix: route.path_prefix.clone(),
                service: route.service.clone(),
                port: route.port,
                tls: route.tls,
                provenance: route.provenance,
                certificate_reference: route.certificate_reference.clone(),
                ownership: route.ownership.clone(),
                priority: route.priority,
                router_name: route.router_name.clone(),
                backend_alias: route.backend_alias.clone(),
                artifact_hash: stack.manifest.artifact_hash.clone(),
                middlewares: route.middlewares.clone(),
            })
        })
        .collect()
}

pub(super) fn hash_field(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}
