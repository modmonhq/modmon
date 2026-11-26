// Whether a stack already runs what its files would start
// (`domain::rollout::unchanged`).

impl DockerComposeRuntime {
    async fn stack_runs_its_files(
        &self,
        target: &RuntimeTarget,
        counts: &BTreeMap<String, u32>,
        leaving: &[String],
    ) -> Result<bool, MmError> {
        use crate::domain::rollout::{WantedService, service_images, stack_unchanged};

        require_local_target(target)?;
        let hashes = self.config_hashes(target).await?;
        let mut args = self.compose_args(target)?;
        args.push("config".to_owned());
        let output = self
            .process
            .run(
                self.command_spec(args)
                    .interpolation_environment(&target.interpolation_environment)
                    .current_dir(&target.root)
                    .timeout(Duration::from_secs(45))
                    .internal_stdout(),
            )
            .await?;
        let output = self.ensure_success(output)?;
        let images =
            service_images(&output.stdout).map_err(|message| MmError::RuntimeResponse { message })?;
        let tags = hashes
            .keys()
            .filter_map(|service| images.get(service).and_then(|(image, _)| image.clone()))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let Some(ids) = self.image_ids(&tags).await? else {
            return Ok(false);
        };
        let wanted = hashes
            .iter()
            .map(|(service, hash)| {
                let (image, always) = images.get(service).cloned().unwrap_or_default();
                WantedService {
                    service: service.clone(),
                    config_hash: hash.clone(),
                    count: counts.get(service).copied().unwrap_or(1),
                    image_id: image.and_then(|image| ids.get(&image).cloned()),
                    always_pulled: always,
                }
            })
            .collect::<Vec<_>>();
        let running = crate::adapters::runtime::project_services::stack_containers(
            self,
            &target.project_name,
            leaving,
        )
        .await?;
        Ok(stack_unchanged(&wanted, &running))
    }

    /// The engine's id of each image of `tags`; `None` when it lacks one.
    async fn image_ids(&self, tags: &[String]) -> Result<Option<BTreeMap<String, String>>, MmError> {
        if tags.is_empty() {
            return Ok(Some(BTreeMap::new()));
        }
        let mut args = vec![
            "image".to_owned(),
            "inspect".to_owned(),
            "--format".to_owned(),
            "{{.Id}}".to_owned(),
        ];
        args.extend(tags.iter().cloned());
        let output = self.command(args).await?;
        if !output.success() {
            return Ok(None);
        }
        let ids = output
            .stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if ids.len() != tags.len() {
            return Ok(None);
        }
        Ok(Some(tags.iter().cloned().zip(ids).collect()))
    }
}
