---
title: Production checklist
description: What to check before an environment runs real traffic, and the test or release artifact behind each item.
---

Go through this list before an environment on remote hosts takes real
traffic, and again after each upgrade. Each item says what to do, and what
backs it: a test in the repository (`core/tests/…` and `scripts/live-…` from
the root of a checkout) or a file published with every release. The
`scripts/live-…` checks run the real binaries against Docker hosts on your
machine; run one with `scripts/live-cluster.sh` and the requirements at the
top of the script.

## Install and pin the release

| Check | How | Backed by |
| ----- | --- | --------- |
| The binaries are the release's | The installer checks the archive's SHA-256 against the release's `SHA256SUMS` before it unpacks anything. By hand: `gh attestation verify ARCHIVE --owner modmonhq`, and `sha256sum --check` against `SHA256SUMS`; see [what a release carries](../contribute/releases.md#what-a-release-carries). | `SHA256SUMS` and the build provenance attestations of each [release](../contribute/releases.md#what-a-release-carries); `scripts/release/test-install-sh.mjs`, `scripts/release/test-verify.sh` |
| Everyone deploys with one release | Set [`deploy.mm-version`](../reference/configuration.md#bindings-environments-and-secrets) on the environment. | `core/tests/production_readiness.rs`: `a_pinned_environment_takes_only_its_release_of_mm_and_mmd`, `the_pin_is_read_from_the_environments_deploy_settings` |
| Hosts and this machine run the same release | `mm doctor` reports `MM_VERSION_PIN`, `MM_HOST_RELEASE_OLDER`, `MM_HOST_RELEASE_NEWER`, `MM_NODE_VERSION_SKEW`. | `core/tests/production_readiness.rs`: `doctor_names_the_hosts_outside_the_pin_and_the_ones_on_another_release` |

## Set up the hosts

| Check | How | Backed by |
| ----- | --- | --------- |
| Setup on a clean host works and repeats without changes | `mm target setup plan`, apply, `mm target setup record`, then plan and apply again: nothing changes. See [set up a cluster](../remote-daemon.md#set-up-a-cluster). | `scripts/live-target-setup.sh`; `scripts/live-cluster-setup.sh` checks real hosts after setup |
| Developers reach the hosts only through the relay | One restricted `mm` account, no shell, no Docker socket. See [remote access](../remote-access.md). | `scripts/live-remote-relay.sh`; `core/tests/remote_protocol.rs` |
| SSH keys made by mm are encrypted | `mm doctor` names a plain key (`MM_KEY_UNPROTECTED`); `mm key protect` encrypts it. | `core/tests/key_protection.rs` |
| A host enrolled for push holds no deploy key | Setup removes what fetching left; remove that key from the git host too. See [deploy without git on the host](../remote-daemon.md#deploy-without-git-on-the-host). | `core/tests/push_delivery.rs`: `enrolling_a_project_for_push_turns_a_fetching_host_over` |
| Only allowed branches are deployed | Set `deploy-from` on the environment. A pushed commit from another branch is refused before it is uploaded. | `core/tests/push_delivery.rs`: `deploy_from_is_held_against_the_branch_the_client_states` |
| Containers are kept apart from the hosts | The mesh guard and `hardening:`. See [hardening](../hardening.md). | `core/tests/mesh_guard.rs`, `core/tests/hardening.rs`; `scripts/live-cluster.sh` |

## Keep the hosts running

| Check | How | Backed by |
| ----- | --- | --------- |
| `mmd` is replaced when it stops answering | systemd's watchdog restarts it within 30 s. See [when mmd stops answering](../remote-daemon.md#when-mmd-stops-answering). | `core/tests/daemon_alive.rs`: `the_keep_alive_is_sent_only_while_requests_are_answered`; `core/tests/daemon_wedge.rs` |
| A restart of `mmd` drops no name lookup and no request | `mmd` hands its sockets to systemd, which keeps them across the restart. | `core/tests/production_readiness.rs`: `every_socket_mmd_listens_on_is_handed_to_the_service_manager_and_taken_back`; `scripts/live-cluster.sh`, stage "an mmd upgrade, one node at a time, under traffic" |
| A mistake in `host.yaml` stops no container's names | `mmd` refuses requests until the file loads; check with `mmd access validate`. | `core/tests/production_readiness.rs`: `a_host_policy_with_a_mistake_leaves_the_daemon_running_and_refuses_requests` |
| A secret store that does not answer stops plans, not services | Plans and applies are refused and say so; running services keep their values. See [read secrets from OpenBao or Vault on the host](../remote-daemon.md#read-secrets-from-openbao-or-vault-on-the-host). | `core/tests/host_secrets.rs`: `a_host_refuses_to_plan_a_deployment_while_the_store_does_not_answer` |
| A node that does not answer is named, and the others are read | `mm service status`, `mm service logs`, and `mm doctor` name it. | `core/tests/service_across_nodes.rs`: `a_followed_log_that_loses_every_stream_fails_with_the_reason`, `a_nodes_stream_that_breaks_keeps_its_lines_and_says_so_once` |
| Each node's liveness and the operations in flight are reported | `mm doctor` and `mm deploy status` list what each host runs, who started it and since when; `mm deploy cancel` stops it on every host. See [follow a deployment, and stop one that is stuck](../remote-daemon.md#follow-a-deployment-and-stop-one-that-is-stuck). | `core/tests/daemon_operations.rs`: `a_deployment_in_flight_is_listed_with_its_id_who_and_since_when`, `cancel_stops_a_deployment_rolls_it_back_and_names_who_did_what`, `a_step_past_its_deadline_ends_with_a_timeout_and_frees_the_environment`; `core/tests/daemon_long_steps.rs`; `core/tests/rollout_walk.rs`: `a_step_that_shows_no_progress_ends_the_wait_with_a_timeout`, `a_long_job_its_host_sees_at_work_is_not_given_up`; `scripts/live-cluster.sh`, stages "no lockout: …" |

## Upgrade

| Check | How | Backed by |
| ----- | --- | --------- |
| An upgrade interrupts no container | [Upgrade mm and your hosts](./upgrade.md): setup replaces `mm` and `mmd` one node at a time. Upgrade between deployments: `mm deploy status`. | `scripts/live-cluster.sh`, stage "an mmd upgrade, one node at a time, under traffic" (container start times unchanged, no request by name failed) |
| A deployment interrupted by a restart is not left locked | The node marks it interrupted when `mmd` starts again; the next deployment needs nothing. | `core/tests/daemon_alive.rs`: `a_daemon_that_starts_marks_the_rollout_it_finds_in_progress_as_interrupted` |

## Deploy and roll back

| Check | How | Backed by |
| ----- | --- | --------- |
| A failed rollout returns every changed node | See [rolling deploys and rollback](../remote-daemon.md#rolling-deploys-and-rollback). | `core/tests/rollout_walk.rs`: `a_failure_on_the_second_node_rolls_both_back_and_never_touches_the_third`; `scripts/live-cluster.sh`, stages "rolling: …" |
| One deployment at a time | A second change waits its turn or is refused with who holds the host. | `core/tests/daemon_access.rs`: `a_second_change_to_the_host_waits_its_turn_and_then_runs`, `a_change_that_does_not_get_its_turn_in_time_says_who_has_it_and_changes_nothing` |
| A node too busy for a change is reported as not changed | The summary lists it under "Not changed" with the reason. | `core/tests/production_readiness.rs`: `a_node_that_was_too_busy_for_a_change_is_listed_as_not_changed` |
| Shared volumes are reachable from every node that mounts them | `mm target storage status`. See [shared volumes across nodes](../remote-daemon.md#shared-volumes-across-nodes). | `core/tests/shared_storage.rs` |
| Jobs and one-off commands run where the service runs | `mm job run` and `mm service restart` at `operate`; `jobs.<name>.run: before-deploy` runs a migration before its service changes, on the node of its service. See [jobs and schedules](../jobs-and-schedules.md). | `core/tests/daemon_operate.rs`: `an_operator_restarts_a_service_and_runs_a_job_and_both_are_audited`; `core/tests/remote_operate.rs`: `a_job_runs_on_the_node_of_its_service_and_runs_are_read_from_every_node`; `core/tests/job_nodes.rs`; `core/tests/rollout_walk.rs`: `a_stage_job_runs_on_its_node_before_any_node_of_the_stage_is_replaced`, `a_job_that_fails_stops_the_deployment_before_its_stack_rolls`; `core/tests/deploy_jobs.rs`; `scripts/live-cluster.sh`, stages "operate: …", "before-deploy: …", "once: …" |
| Traffic reaches every node that runs a service | A stack's `ingress:` routes; the proxy sends each request to a healthy container, and a container leaves routing before it stops. See [route traffic into the cluster](../remote-daemon.md#route-traffic-into-the-cluster). | `core/tests/cluster_ingress.rs`: `the_proxy_file_lists_each_route_with_the_containers_it_sends_to`, `a_leave_waits_for_every_node_then_its_drain_then_stops`; `core/tests/discovery_resolver.rs`: `a_rollback_takes_a_leaving_container_back_into_the_answers`; `scripts/live-cluster.sh`, stages "ingress: …" |
| Podman hosts behave as Docker hosts | `runtime: {driver: podman-compose}`; `mm doctor` names an engine mm cannot drive (`MM_PODMAN_VERSION`, `MM_PODMAN_COMPOSE`). | `core/tests/podman_remote.rs`; `core/tests/convergence_steady.rs`: `a_restart_loop_on_podman_fails_the_wait_within_a_few_looks`; `scripts/live-cluster.sh` with `MM_LIVE_ENGINE=podman` |

## Back up

| Check | How | Backed by |
| ----- | --- | --------- |
| Each host's records are backed up | `mmd state export > FILE` as root; it never holds a key or a secret value. See [back up and restore a host's records](../remote-daemon.md#back-up-and-restore-a-hosts-records). | `core/tests/production_readiness.rs`: `a_hosts_records_are_exported_without_a_key_and_restored_on_the_same_node`, `an_import_refuses_another_nodes_archive_and_anything_an_export_never_holds`, `an_import_is_refused_while_mmd_answers` |
| The audit log does not fill the disk | Rotated at 16 MiB, about 80 MiB kept, with no setting. See [the audit log](../remote-daemon.md#the-audit-log). | `core/tests/production_readiness.rs`: `the_audit_log_rotates_by_size_and_keeps_a_bounded_history` |
| The files of shared volumes are backed up | mm does not copy them. Back up `/var/lib/mm/storage` on the storage node. | Your own backup |
| Secrets can be put back | They stay in your secret store; `mm target setup` pushes them to a rebuilt host again. | `core/tests/host_secrets.rs` |
