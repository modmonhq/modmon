#[async_trait]
impl LocalDeploymentRuntime for DockerComposeRuntime {
    async fn preflight(
        &self,
        target: &NamedTarget,
        resources: &[RuntimeResource],
        projects: &[RuntimeTarget],
    ) -> Result<(), MmError> {
        for resource in resources {
            let ownership =
                Self::ownership(target, resource.kind, &resource.name, &resource.resource)?;
            self.inspect_owned(&ownership).await?;
        }
        for project in projects {
            self.preflight_project(project).await?;
        }
        Ok(())
    }
    async fn ensure_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        let ownership = Self::ownership(target, ResourceKind::Network, name, resource)?;
        if self.inspect_owned(&ownership).await?.is_some() {
            return Ok(ResourceEnsureResult::Unchanged);
        }
        let mut args = vec!["network".to_owned(), "create".to_owned()];
        args.extend(ownership_label_args(&ownership));
        args.push(name.to_owned());
        let output = self.command(args).await?;
        self.ensure_success(output)?;
        Ok(ResourceEnsureResult::Created)
    }

    async fn ensure_internal_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        self.ensured_internal_network(target, name, resource).await
    }

    async fn ensure_cluster_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
        network: &crate::domain::cluster_network::LocalNetwork,
    ) -> Result<ResourceEnsureResult, MmError> {
        self.ensured_cluster_network(target, name, resource, network)
            .await
    }

    async fn ensure_network_with_subnet(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
        subnet: &str,
        gateway: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        let ownership = Self::ownership(target, ResourceKind::Network, name, resource)?;
        if let Some(existing) = self.inspect_owned(&ownership).await? {
            if network_addressing(&existing).contains(&(subnet.to_owned(), gateway.to_owned())) {
                return Ok(ResourceEnsureResult::Unchanged);
            }
            return Err(MmError::Target {
                message: format!(
                    "mm network '{name}' exists without the fixed subnet {subnet}; run `mm ingress stop`, remove it with `docker network rm {name}`, then retry"
                ),
            });
        }
        if let Some(conflict) = self.overlapping_network(subnet).await? {
            return Err(MmError::Target {
                message: format!(
                    "Docker network '{conflict}' overlaps mm's ingress subnet {subnet}; remove or re-address that network, then retry"
                ),
            });
        }
        let mut args = vec![
            "network".to_owned(),
            "create".to_owned(),
            "--subnet".to_owned(),
            subnet.to_owned(),
            "--gateway".to_owned(),
            gateway.to_owned(),
        ];
        args.extend(ownership_label_args(&ownership));
        args.push(name.to_owned());
        let output = self.command(args).await?;
        self.ensure_success(output)?;
        Ok(ResourceEnsureResult::Created)
    }

    async fn inspect_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<bool, MmError> {
        let ownership = Self::ownership(target, ResourceKind::Network, name, resource)?;
        Ok(self.inspect_owned(&ownership).await?.is_some())
    }

    async fn remove_network(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<(), MmError> {
        if !self.inspect_network(target, name, resource).await? {
            return Ok(());
        }
        let output = self
            .command(vec!["network".to_owned(), "rm".to_owned(), name.to_owned()])
            .await?;
        self.ensure_success(output)?;
        Ok(())
    }

    async fn ensure_volume(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        let ownership = Self::ownership(target, ResourceKind::Volume, name, resource)?;
        if self.inspect_owned(&ownership).await?.is_some() {
            return Ok(ResourceEnsureResult::Unchanged);
        }
        let mut args = vec!["volume".to_owned(), "create".to_owned()];
        args.extend(ownership_label_args(&ownership));
        args.push(name.to_owned());
        let output = self.command(args).await?;
        self.ensure_success(output)?;
        Ok(ResourceEnsureResult::Created)
    }

    async fn remove_volume(
        &self,
        target: &NamedTarget,
        name: &str,
        resource: &str,
    ) -> Result<(), MmError> {
        let ownership = Self::ownership(target, ResourceKind::Volume, name, resource)?;
        if self.inspect_owned(&ownership).await?.is_none() {
            return Ok(());
        }
        let output = self
            .command(vec!["volume".to_owned(), "rm".to_owned(), name.to_owned()])
            .await?;
        self.ensure_success(output)?;
        Ok(())
    }

    async fn set_volume_owner(
        &self,
        target: &NamedTarget,
        volume_name: &str,
        volume_resource: &str,
        ownership: VolumeOwnership,
    ) -> Result<(), MmError> {
        let resource_ownership =
            Self::ownership(target, ResourceKind::Volume, volume_name, volume_resource)?;
        if self.inspect_owned(&resource_ownership).await?.is_none() {
            return Err(MmError::Security {
                message: format!("owned volume '{volume_name}' does not exist"),
            });
        }
        let mut args = vec!["run".to_owned(), "--rm".to_owned()];
        args.extend(Self::volume_helper_args(volume_name));
        args.extend([
            "--cap-add".to_owned(),
            "CHOWN".to_owned(),
            "--cap-add".to_owned(),
            "FOWNER".to_owned(),
            VOLUME_HELPER_IMAGE.to_owned(),
            "sh".to_owned(),
            "-c".to_owned(),
            "chown \"$1:$2\" /volume && chmod \"$3\" /volume".to_owned(),
            "mm-volume-helper".to_owned(),
            ownership.uid.to_string(),
            ownership.gid.to_string(),
            format!("{:o}", ownership.mode),
        ]);
        let output = self
            .process
            .run(self.command_spec(args).timeout(Duration::from_secs(90)))
            .await?;
        self.ensure_success(output)?;
        Ok(())
    }

    async fn create_volume_file_if_absent(
        &self,
        target: &NamedTarget,
        volume_name: &str,
        volume_resource: &str,
        file_name: &str,
        mode: u32,
    ) -> Result<(), MmError> {
        validate_volume_file(file_name)?;
        let ownership =
            Self::ownership(target, ResourceKind::Volume, volume_name, volume_resource)?;
        if self.inspect_owned(&ownership).await?.is_none() {
            return Err(MmError::Security {
                message: format!("owned volume '{volume_name}' does not exist"),
            });
        }
        self.run_volume_helper(
            volume_name,
            file_name,
            mode,
            "if [ ! -e \"/volume/$1\" ]; then umask 077; : > \"/volume/$1\"; chmod \"$2\" \"/volume/$1\"; fi",
            &format!("creating '{file_name}' in volume '{volume_name}'"),
        )
        .await
    }

    async fn replace_volume_file(
        &self,
        target: &NamedTarget,
        volume_name: &str,
        volume_resource: &str,
        file_name: &str,
        source: &Path,
        mode: u32,
    ) -> Result<(), MmError> {
        validate_volume_file(file_name)?;
        let ownership =
            Self::ownership(target, ResourceKind::Volume, volume_name, volume_resource)?;
        if self.inspect_owned(&ownership).await?.is_none() {
            return Err(MmError::Security {
                message: format!("owned volume '{volume_name}' does not exist"),
            });
        }
        self.copy_volume_file(
            volume_name,
            source,
            OwnedVolumeFile {
                name: file_name,
                ownership: VolumeOwnership {
                    uid: 0,
                    gid: 0,
                    mode,
                },
            },
        )
        .await
    }

    async fn replace_volume_file_owned(
        &self,
        target: &NamedTarget,
        volume_name: &str,
        volume_resource: &str,
        file: OwnedVolumeFile<'_>,
        source: &Path,
    ) -> Result<(), MmError> {
        validate_volume_file(file.name)?;
        let ownership =
            Self::ownership(target, ResourceKind::Volume, volume_name, volume_resource)?;
        if self.inspect_owned(&ownership).await?.is_none() {
            return Err(MmError::Security {
                message: format!("owned volume '{volume_name}' does not exist"),
            });
        }
        self.copy_volume_file(volume_name, source, file).await
    }

    async fn remove_volume_file(
        &self,
        target: &NamedTarget,
        volume_name: &str,
        volume_resource: &str,
        file_name: &str,
    ) -> Result<(), MmError> {
        validate_volume_file(file_name)?;
        let ownership =
            Self::ownership(target, ResourceKind::Volume, volume_name, volume_resource)?;
        if self.inspect_owned(&ownership).await?.is_none() {
            return Err(MmError::Security {
                message: format!("owned volume '{volume_name}' does not exist"),
            });
        }
        self.run_volume_helper(
            volume_name,
            file_name,
            0,
            "rm -f -- \"/volume/$1\"",
            &format!("removing '{file_name}' from volume '{volume_name}'"),
        )
        .await
    }

    async fn apply(&self, target: &RuntimeTarget) -> Result<(), MmError> {
        self.up(target, true, &[], &BTreeMap::new()).await
    }

    async fn apply_keeping_other_services(&self, target: &RuntimeTarget) -> Result<(), MmError> {
        self.up(target, false, &[], &BTreeMap::new()).await
    }

    async fn apply_services(
        &self,
        target: &RuntimeTarget,
        services: &[String],
        counts: &BTreeMap<String, u32>,
    ) -> Result<(), MmError> {
        self.up(target, false, services, counts).await
    }

    async fn apply_scaled(
        &self,
        target: &RuntimeTarget,
        counts: &BTreeMap<String, u32>,
        keep_others: bool,
    ) -> Result<(), MmError> {
        self.up(target, !keep_others, &[], counts).await
    }

    async fn scale_service(
        &self,
        target: &RuntimeTarget,
        service: &str,
        count: u32,
    ) -> Result<(), MmError> {
        self.start_more(target, service, count).await
    }

    async fn pull_images(
        &self,
        target: &RuntimeTarget,
        services: Option<&[String]>,
        limit: Duration,
    ) -> Result<(), MmError> {
        require_local_target(target)?;
        let base = |args: Vec<String>| self.command_spec(args);
        crate::adapters::runtime::image_pull::ImagePuller {
            process: self.process.as_ref(),
            kind: crate::domain::runtime::RuntimeKind::DockerCompose,
            program: "docker",
            base: &base,
        }
        .pull(target, self.compose_args(target)?, services, limit)
        .await
    }

    async fn service_container_ids(
        &self,
        target: &RuntimeTarget,
        services: &[String],
    ) -> Result<Vec<String>, MmError> {
        require_local_target(target)?;
        crate::adapters::runtime::project_services::service_container_ids(
            self,
            &target.project_name,
            services,
        )
        .await
    }

    async fn remove_services(
        &self,
        target: &RuntimeTarget,
        services: &[String],
        whole_project: bool,
    ) -> Result<(), MmError> {
        require_local_target(target)?;
        self.preflight_project(target).await?;
        crate::adapters::runtime::project_services::remove_services(
            self,
            &target.project_name,
            services,
            whole_project,
            project_limit(target, ComposeOrder::Dependencies, DOCKER_FLOOR),
        )
        .await
    }

    async fn start_beside(
        &self,
        target: &RuntimeTarget,
        services: &[String],
    ) -> Result<Vec<String>, MmError> {
        self.start_beside_old(target, services).await
    }

    async fn replaced_containers(
        &self,
        target: &RuntimeTarget,
        services: &[String],
    ) -> Result<Vec<String>, MmError> {
        self.replaced_by_newer(target, services).await
    }

    async fn outdated_containers(
        &self,
        target: &RuntimeTarget,
        services: &[String],
    ) -> Result<Vec<String>, MmError> {
        self.outdated(target, services).await
    }

    async fn remove_containers(
        &self,
        target: &RuntimeTarget,
        containers: &[String],
    ) -> Result<(), MmError> {
        require_local_target(target)?;
        crate::adapters::runtime::project_services::remove_containers(
            self,
            containers,
            project_limit(target, ComposeOrder::Dependencies, DOCKER_FLOOR),
        )
        .await
    }

    async fn remove(&self, target: &RuntimeTarget) -> Result<(), MmError> {
        require_local_target(target)?;
        self.preflight_project(target).await?;
        let mut args = self.compose_args(target)?;
        args.extend(["down".to_owned(), "--remove-orphans".to_owned()]);
        let output = self
            .process
            .run(
                self.command_spec(args)
                    .current_dir(&target.root)
                    .timeout(project_limit(target, ComposeOrder::Dependencies, DOCKER_FLOOR)),
            )
            .await?;
        self.ensure_success(output)?;
        Ok(())
    }
}

impl DockerComposeRuntime {
    /// `compose up` for the target's project (only `only` and none of what
    /// they depend on, when it names any), with `counts[service]`
    /// containers of each service `counts` names. Without
    /// `remove_orphans`, containers of services the files no longer list
    /// keep running.
    async fn up(
        &self,
        target: &RuntimeTarget,
        remove_orphans: bool,
        only: &[String],
        counts: &BTreeMap<String, u32>,
    ) -> Result<(), MmError> {
        require_local_target(target)?;
        self.preflight_project(target).await?;
        let auth = crate::adapters::runtime::registry_auth::RegistryAuth::prepare(
            &target.registry_credentials,
        )?;
        let mut args = self.compose_args(target)?;
        args.extend(["up".to_owned(), "-d".to_owned()]);
        if remove_orphans {
            args.push("--remove-orphans".to_owned());
        }
        // Compose refuses a count for a service it was not asked to start.
        for (service, count) in counts
            .iter()
            .filter(|(service, _)| only.is_empty() || only.contains(service))
        {
            args.extend(["--scale".to_owned(), format!("{service}={count}")]);
        }
        args.push("--no-build".to_owned());
        if !only.is_empty() {
            args.push("--no-deps".to_owned());
            args.extend(only.iter().cloned());
        }
        let output = self
            .process
            .run(
                auth.docker(self.command_spec(args))
                    .interpolation_environment(&target.interpolation_environment)
                    .current_dir(&target.root)
                    .timeout(project_limit(target, ComposeOrder::Dependencies, DOCKER_FLOOR)),
            )
            .await?;
        self.ensure_started(output, target, only)
    }

    async fn overlapping_network(&self, subnet: &str) -> Result<Option<String>, MmError> {
        let listed = self
            .command(vec![
                "network".to_owned(),
                "ls".to_owned(),
                "--quiet".to_owned(),
                "--no-trunc".to_owned(),
            ])
            .await?;
        let listed = self.ensure_success(listed)?;
        let ids = listed
            .stdout
            .lines()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(None);
        }
        let mut args = vec![
            "network".to_owned(),
            "inspect".to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ];
        args.extend(ids);
        let inspected = self.command(args).await?;
        let inspected = self.ensure_success(inspected)?;
        let networks: serde_json::Value =
            serde_json::from_str(inspected.stdout.trim()).map_err(|error| {
                MmError::RuntimeResponse {
                    message: format!("Docker network inspect returned invalid JSON: {error}"),
                }
            })?;
        Ok(first_overlapping_network(&networks, subnet))
    }
}

fn network_addressing(network: &serde_json::Value) -> Vec<(String, String)> {
    network["IPAM"]["Config"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|config| {
            Some((
                config["Subnet"].as_str()?.to_owned(),
                config["Gateway"].as_str()?.to_owned(),
            ))
        })
        .collect()
}

fn first_overlapping_network(networks: &serde_json::Value, subnet: &str) -> Option<String> {
    networks.as_array()?.iter().find_map(|network| {
        let overlaps = network["IPAM"]["Config"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|config| config["Subnet"].as_str())
            .any(|existing| {
                crate::domain::target_ingress::ipv4_subnets_overlap(existing, subnet)
                    .unwrap_or(false)
            });
        overlaps.then(|| network["Name"].as_str().unwrap_or("unnamed").to_owned())
    })
}

#[cfg(test)]
mod addressing_tests {
    use super::{first_overlapping_network, network_addressing};

    #[test]
    fn subnet_conflicts_ignore_ipv6_and_report_the_overlapping_network() {
        let networks = serde_json::json!([
            {"Name": "bridge", "IPAM": {"Config": [{"Subnet": "172.17.0.0/16", "Gateway": "172.17.0.1"}]}},
            {"Name": "dual", "IPAM": {"Config": [{"Subnet": "fd00::/64"}, {"Subnet": "10.252.0.0/16"}]}},
            {"Name": "none", "IPAM": {"Config": null}}
        ]);
        assert_eq!(
            first_overlapping_network(&networks, "10.252.7.0/24").as_deref(),
            Some("dual")
        );
        assert_eq!(first_overlapping_network(&networks, "10.253.7.0/24"), None);
        assert_eq!(
            network_addressing(&networks[0]),
            [("172.17.0.0/16".to_owned(), "172.17.0.1".to_owned())]
        );
        assert!(network_addressing(&networks[2]).is_empty());
    }
}
