//! Binary composition root.

pub(crate) mod mcp;
mod mcp_commands;

mod agents;
mod application;
mod builds;
mod cluster_networks;
mod cluster_setup;
mod command_history;
pub use command_history::{CommandRecording, command_history};
mod config;
mod daemon_checkout;
mod delivery;
mod job_keeper;
pub use job_keeper::JobKeeper;
mod engine_api;
mod engine_facts;
pub use engine_api::ENGINE_API_ENVIRONMENT;
mod dashboard_secrets;
mod environment;
mod environments;
pub(crate) use environments::environment_placements;
mod forwards;
mod host_platform;
mod host_secrets;
mod host_values;
pub use host_values::AgeValueSealer;
mod keys;
mod mesh;
mod metrics;
mod node_addresses;
pub(crate) use node_addresses::{project_node_addresses, set_node_address, unset_node_address};
mod operations;
pub(crate) use operations::{job_gateway_as, service_gateway, service_gateway_as};
mod providers;
mod readiness;
mod releases;
mod remote;
mod runtime;
mod runtimes;
mod secret_store;
pub(crate) use readiness::*;
pub(crate) use secret_store::{
    cluster_store_report, restore_store, store_restore_plan, take_store_snapshot,
};
pub use secret_store::{
    store_audit, store_init, store_join, store_member, store_remove_peer, store_status,
    store_unsealer, store_versions,
};
mod services;
mod storage;
mod target_trust;
mod targets;
mod telemetry;
pub use telemetry::install_timing_trace;
mod update_steps;
pub(crate) use update_steps::update_follow_up_service;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::adapters::build::{DockerBuildxBuilder, GitBuildSourceInspector};
use crate::adapters::ca::{FilesystemLocalCaStore, PlatformTrustStore};
use crate::adapters::clock::SystemClock;
use crate::adapters::config::ConfigLoader;
use crate::adapters::dashboard::DashboardEventHub;
use crate::adapters::dashboard_service::SystemdDashboardServiceInstaller;
use crate::adapters::environment::{
    EnvironmentIndexer, FilesystemEnvironmentArtifactStore, FilesystemMaterializedEnvironmentWriter,
};
use crate::adapters::gate::{
    Argon2idPasswordVerifier, ReqwestCapSiteProvisioner, ReqwestCapTokenVerifier,
    XChaChaSessionCipher,
};
use crate::adapters::managed_service_runtime::SelectedTargetManagedServiceRuntime;
use crate::adapters::process::TokioProcessRunner;
use crate::adapters::remote::{ClientAccess, ClientKey, ConnectionReuse, SshRemoteTransport};
use crate::adapters::runtime::{RuntimeFactory, TcpIngressVerifier};
use crate::adapters::secrets::{
    AgeSecretStore, DockerComposeFileSecretDelivery, EnvironmentSecretStore,
    OpenBaoBackendUseCases, OpenBaoConnectionStore, OpenBaoSecretStore, PodmanSecretDelivery,
    ReqwestOpenBaoTransport, ReqwestVaultTransport, SecretProviderRegistry,
    StorageProviderRegistration, VaultBackendUseCases, VaultConnectionStore, VaultSecretStore,
    age_storage_descriptor, environment_storage_descriptor, openbao_provider_descriptor,
    openbao_storage_descriptor, vault_provider_descriptor, vault_storage_descriptor,
};
use crate::adapters::service_stack_runtime::{
    LocalServiceStackExecutor, LocalServiceStackExecutorConfig,
};
use crate::adapters::services::FilesystemServiceStackStore;
use crate::adapters::stack::FilesystemStackArtifactStore;
use crate::adapters::state::{MmHome, SqliteStateStore};
use crate::adapters::target::{
    AnsibleTargetBootstrapExecutor, FilesystemTargetIngressArtifactStore,
    FilesystemTargetOperationLock, FilesystemTargetStore, FilesystemTraefikDynamicConfigStore,
    RuntimeGateIngressPublisher, SshHostReleaseReader, SudoPrivilegedIngressInstaller,
    SystemdRootlessIngressInstaller, install_privileged_ingress_artifact,
    uninstall_privileged_ingress,
};
use crate::app::ports::{
    BuildOperationLock, BuildSourceInspector, Clock, EnvironmentSourceDiscovery,
    EnvironmentValueStore, ExecAuditJournal, GateRepository, ImageBuilder, IngressRouteRegistry,
    MaterializedEnvironmentWriter, ProcessRunner, ProjectDocumentStore, RuntimeDriver,
    RuntimeOperationAuthorizer, RuntimeSecretDelivery, SchedulerRepository, SecretBatchReplacer,
    SecretCreator, SecretDeliveryCapabilityCatalog, SecretMaterialResolver,
    SecretMetadataInspector, SecretRemover, SecretReplacer, SecretStorageCapabilityCatalog,
    StateStore, StreamingProcessRunner, TargetStore,
};
use crate::app::{
    Application, ApplicationDependencies, BuildExecutionPorts, BuildManager, BuildService,
    ClusterSetupDependencies, ClusterSetupService, DashboardQueryService, DeploymentExecutionPorts,
    DeploymentManager, DeploymentManagerDependencies, DeploymentService, DevelopmentMountManager,
    EnvironmentDependencies, EnvironmentManager, GateDriverRegistry, GateManager,
    GateManagerDependencies, JobSchedulerDependencies, JobSchedulerService, LocalCaManager,
    LocalRuntimeOperationAuthorizer, OnboardingService, PasswordCapGateDriver,
    RegisteredDashboardHostSource, RuntimeOperationsDependencies, RuntimeOperationsService,
    SecretManager, SecretManagerDependencies, ServiceStackManager, ServiceStackManagerConfig,
    StackManager, TargetBootstrapExecutionPorts, TargetBootstrapService, TargetIngressManager,
    TargetManager, WorkspaceSelection,
};
use crate::domain::ca::TrustScope;
use crate::domain::config::{ConfigDocument, resolve_environment};
use crate::domain::error::MmError;
use crate::domain::runtime::RuntimeKind;
use crate::domain::secret::InitializedAgeStore;
use crate::domain::service_stack::{CAP_HOST, DashboardServiceConfig};
use crate::domain::target::TargetId;
use crate::domain::target_ingress::ingress_verify_port;

pub use crate::adapters::agent_clients::{AgentClient, AgentSetupPlan, FileAction};
pub(crate) use crate::adapters::target::ForwardShortcutStore;
pub use agents::{AgentEnvironmentStatus, AgentStatus};
pub(crate) use agents::{
    AgentSetupScope, agent_status, apply_agent_setup, plan_agent_setup, trust_agents,
};
pub(crate) use application::*;
pub(crate) use builds::local_build_engine;
pub(crate) use cluster_setup::{
    ClusterSetupServices, cluster_environment, cluster_setup_services, cluster_setup_services_for,
    export_store_recovery, import_store_recovery, pushed_commit,
};
pub(crate) use config::*;
pub(crate) use daemon_checkout::{released, resolve_commit};
pub(crate) use environment::*;
pub(crate) use forwards::{declared_service_ports, forward_shortcuts};
pub(crate) use host_platform::public_ingress_control;
use host_platform::{
    dashboard_service_installer, privileged_ingress_installer, rootless_ingress_installer,
    target_ingress_manager,
};
pub use host_secrets::{
    connect_host_provider, disconnect_host_provider, host_secret_recipient, import_host_secrets,
};
pub use keys::KEY_VAULT_ENVIRONMENT;
pub(crate) use keys::{
    agent_key, agent_key_path, close_all_kept_connections, close_kept_connections,
    create_device_key, device_key, device_key_path, kept_connections, key_service,
};
pub use mesh::{
    autoscaler, cluster_networks, healer, ingress_keeper, leave_keeper, mesh_host, name_resolver,
    node_network_status, request_leave,
};
pub use metrics::container_metrics;
pub use providers::serve_as_host;
pub(crate) use providers::*;
pub(crate) use releases::{host_binary_source, self_update_service, update_check_service};
pub use remote::{DaemonUseCaseAdapter, serve_stdio_relay};
pub(crate) use remote::{
    RemoteIdentity, remote_environment, remote_environment_as, remote_transport_context,
};
pub(crate) use runtime::RuntimeComposition;
pub(crate) use services::*;
pub(crate) use storage::{delete_volume, plan_volume_delete, target_storage};
pub use storage::{shared_storage, shared_storage_host, storage_keeper};
pub use target_trust::TargetTrustReview;
pub(crate) use target_trust::trust_repo_target;
pub(crate) use targets::{
    TargetServices, authorize_local_setup, local_ca_manager, local_ingress_loopback_ports,
    prepare_public_ingress, target_services,
};
pub use targets::{ensure_local_target, install_ingress, uninstall_ingress};

pub async fn run(
    cli: crate::presentation::cli::Cli,
    cancellation: crate::domain::observability::OperationCancellation,
) -> Result<u8, MmError> {
    crate::presentation::cli::run(cli, cancellation).await
}

#[cfg(test)]
mod tests;
