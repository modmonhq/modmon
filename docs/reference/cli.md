---
title: CLI command reference
description: Generated from the mm Clap command tree.
---

<!-- Generated from the canonical Clap tree by ./scripts/generate-artifacts.sh. Do not edit. -->

# `mm`

```text
mm compiles one engine-neutral project contract into reviewed plans, then applies them through a selected runtime. Selection is resolved once from CLI flags, private preferences, environment config, project defaults, then target defaults. Missing or contradictory selection is an error. Plans, logs, and JSON never contain secret values.

`--help` never probes a container engine or opens project state.

Usage: mm [OPTIONS] [COMMAND]

Commands:
  init       Initialize a project through a reviewed plan or interactive onboarding
  dev        Set up and operate the local development environment
  config     Validate and edit desired project configuration through typed reviews
  doctor     Check configuration health and resolved runtime capabilities
  ingress    Start or stop this workstation's global public HTTP/HTTPS forwarding
  service    Inspect and operate deployed services
  job        Run finite jobs and operate their schedules
  env        Inspect and deliver non-secret environment values
  secret     Manage secret values, rotation, and provider instances
  access     Manage application access policies, users, routes, and sessions
  build      Plan and execute deterministic OCI image builds
  deploy     Plan and apply the selected environment to its target
  team       Show who may reach remote environments, join, and sync access
  forward    Reach a remote service's port on localhost; save forwards for reuse
  key        Create, show, protect, replace, or remove this device's SSH keys for remote environments
  target     Manage targets, private selection, setup, ingress, and remote access
  agent      Show and trust what AI agents may do, and connect coding tools
  history    List the commands run in this project on this machine, and show what one printed
  workspace  List the project folders mm knows on this machine, and forget the ones that are gone
  update     Update this machine's mm, and the mmd and mm-ingress-helper beside it, from a verified release, and restart the local services that run it
  mcp        Serve the Model Context Protocol server that AI agents use
  help       Print this message or the help of the given subcommand(s)

Options:
      --root <PATH>
          Project root or a path inside the project

  -e, --environment <NAME>
          Environment to operate on (overrides MM_ENVIRONMENT and default-environment)

          [env: MM_ENVIRONMENT=]

      --runtime <RUNTIME>
          Ephemeral runtime adapter for this invocation only

          [possible values: podman-compose, docker-compose]

      --target <NAME>
          Ephemeral target name for this invocation only

      --mm-home <PATH>
          Directory containing mm's local state and target catalog

          [env: MM_HOME=]

      --output <OUTPUT>
          Human text or machine-readable JSON. JSON errors stay JSON

          [default: human]
          [possible values: human, json]

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version

Examples:
  mm init
  mm dev setup
  mm config validate
  mm service status
  mm deploy plan

Selection: --runtime and --target override private preferences and project defaults. Use `mm target preference show` to inspect provenance.
```

## `mm init`

```text
Initialize a project through a reviewed plan or interactive onboarding

Usage: init [OPTIONS]

Options:
      --name <NAME>
          Override the project name inferred from the directory

      --plan <HASH>
          Exact hash printed by the read-only initialization review

      --yes
          Apply the reviewed configuration, Ansible starter, and local-override ignore rule

  -h, --help
          Print help

Examples:
  mm init --name example
  mm init --name example --plan HASH --yes

With an interactive terminal and no arguments, onboarding opens without mutating files before review. Noninteractive use prints a deterministic review. Apply requires the exact plan hash and --yes. Exit codes: 0 success; 1 stale/local state; 2 invalid root/configuration; 3 security failure.
```

## `mm dev`

```text
Set up and operate the local development environment

Usage: dev [COMMAND]

Commands:
  setup      Set up local trust, ingress, Mailpit, Cap, dashboard, and project routes
  up         Review or apply the local development deployment
  enable     Turn optional modules, services or jobs on for this machine; `mm dev up` starts them
  disable    Turn optional units off for this machine, stop them and remove their routes; their volumes stay
  down       Stop the project's containers; volumes, routes, and overrides are kept
  status     Show local project and runtime status
  tls        Inspect and maintain local HTTPS certificates and trust
  services   Manage the installation-wide local service stack
  dashboard  Serve the local developer dashboard
  mounts     Discover, review, and disable source-code mounts
  open       Print a configured local service URL
  help       Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

### `mm dev setup`

```text
Set up local trust, ingress, Mailpit, Cap, dashboard, and project routes

Usage: setup [OPTIONS] [COMMAND]

Commands:
  project  Inspect and deploy an existing clone using its configured development environment
  help     Print this message or the help of the given subcommand(s)

Options:
      --no-trust
          Generate certificates without trusting the root

      --system
          Use the privileged system trust store on macOS or Windows

      --services
          Review and install the shared local service stack

      --smtp-port <PORT>
          Publish SMTP on this explicit loopback port

      --yes
          Apply the displayed service plan without prompting

  -h, --help
          Print help
```

#### `mm dev setup project`

```text
Inspect and deploy an existing clone using its configured development environment

Usage: project [OPTIONS]

Options:
      --plan <HASH>
          Hash printed by the read-only guided workflow review

      --deploy-plan <HASH>
          Hash printed by `mm deploy plan` for the same environment and target

      --yes
          Apply reviewed local dependencies and deployment without prompting

  -h, --help
          Print help

Examples:
  mm dev setup project
  mm dev setup project --plan WORKFLOW_HASH --deploy-plan DEPLOY_HASH --yes

The first command is read-only. Apply requires both reviewed hashes. Secret plaintext is entered separately with `mm secret set`; it is never displayed. Exit codes: 0 success; 1 stale/local state; 2 missing values/configuration; 3 runtime/security failure.
```

### `mm dev up`

```text
Review or apply the local development deployment.

Images that have no local build yet, or whose build inputs changed since their last local build, are built first, as part of the reviewed plan.

Usage: up [OPTIONS]

Options:
      --with <UNIT>
          Turn an optional module, service or job on for this run only; repeat for more

      --plan <HASH>
          Hash printed by the deployment review

      --yes
          Apply the exact reviewed plan without prompting

  -h, --help
          Print help (see a summary with '-h')
```

### `mm dev enable`

```text
Turn optional modules, services or jobs on for this machine; `mm dev up` starts them

Usage: enable <UNIT>...

Arguments:
  <UNIT>...
          A module, service (or MODULE/SERVICE) or job with `development: optional`

Options:
  -h, --help
          Print help

A unit is a module, a service (or MODULE/SERVICE) or a job with `development: optional`. What it needs through Compose `depends_on` comes on with it. The choice is kept in the gitignored .config/mm.overrides.yaml.
```

### `mm dev disable`

```text
Turn optional units off for this machine, stop them and remove their routes; their volumes stay

Usage: disable [OPTIONS] <UNIT>...

Arguments:
  <UNIT>...
          A unit `mm dev enable` turned on

Options:
      --yes
          Apply the deployment that stops them without prompting

  -h, --help
          Print help
```

### `mm dev down`

```text
Stop the project's containers; volumes, routes, and overrides are kept

Usage: down [OPTIONS]

Options:
      --yes
          Confirm stopping every stack of the selected environment

  -h, --help
          Print help
```

### `mm dev status`

```text
Show local project and runtime status

Usage: status

Options:
  -h, --help
          Print help
```

### `mm dev tls`

```text
Inspect and maintain local HTTPS certificates and trust

Usage: tls [OPTIONS] <COMMAND>

Commands:
  status     Query CA files, leaf renewal, and the actual platform trust store
  repair     Restore trust, renew stale leaves, and finish an interrupted rotation
  rotate     Rotate the root with staged trust overlap and reissued leaves
  uninstall  Remove only exact mm-owned trust entries and local private material
  help       Print this message or the help of the given subcommand(s)

Options:
      --system
          Use the privileged system trust store on macOS or Windows

  -h, --help
          Print help
```

#### `mm dev tls status`

```text
Query CA files, leaf renewal, and the actual platform trust store

Usage: status

Options:
  -h, --help
          Print help
```

#### `mm dev tls repair`

```text
Restore trust, renew stale leaves, and finish an interrupted rotation

Usage: repair

Options:
  -h, --help
          Print help
```

#### `mm dev tls rotate`

```text
Rotate the root with staged trust overlap and reissued leaves

Usage: rotate

Options:
  -h, --help
          Print help
```

#### `mm dev tls uninstall`

```text
Remove only exact mm-owned trust entries and local private material

Usage: uninstall

Options:
  -h, --help
          Print help
```

### `mm dev services`

```text
Manage the installation-wide local service stack

Usage: services <COMMAND>

Commands:
  plan     Publish a deterministic reviewed lifecycle plan
  apply    Apply an exact install/convergence plan hash
  status   Show component health, ownership, image, route, certificate, and drift status
  upgrade  Apply an exact version upgrade plan hash
  repair   Recover an interrupted operation or reconcile drift from an exact plan
  remove   Remove only mm-owned runtime resources using an exact reviewed plan
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm dev services plan`

```text
Publish a deterministic reviewed lifecycle plan

Usage: plan [OPTIONS]

Options:
      --operation <OPERATION>
          Lifecycle operation represented by the plan

          [default: apply]
          [possible values: apply, upgrade, repair, remove]

      --smtp-port <PORT>
          Publish SMTP on this loopback port for host-native applications

      --purge
          Include persistent data deletion in a remove plan

      --gate
          Prepare conditional Cap and Valkey components before creating the first gate policy

  -h, --help
          Print help
```

#### `mm dev services apply`

```text
Apply an exact install/convergence plan hash

Usage: apply [OPTIONS] --plan <HASH>

Options:
      --plan <HASH>
          Hash printed by `mm dev services plan` for an apply operation

      --smtp-port <PORT>
          Publish SMTP on this loopback port; must match the reviewed plan

      --gate
          Include the reviewed first-policy Cap and Valkey preparation

      --yes
          Skip the service mutation confirmation prompt

  -h, --help
          Print help
```

#### `mm dev services status`

```text
Show component health, ownership, image, route, certificate, and drift status

Usage: status

Options:
  -h, --help
          Print help
```

#### `mm dev services upgrade`

```text
Apply an exact version upgrade plan hash

Usage: upgrade [OPTIONS] --plan <HASH>

Options:
      --plan <HASH>
          Hash printed by `mm dev services plan --operation upgrade`

      --smtp-port <PORT>
          Change the loopback SMTP port as part of the reviewed upgrade

      --gate
          Include the reviewed Cap and Valkey components

      --yes
          Skip the service mutation confirmation prompt

  -h, --help
          Print help
```

#### `mm dev services repair`

```text
Recover an interrupted operation or reconcile drift from an exact plan

Usage: repair [OPTIONS] --plan <HASH>

Options:
      --plan <HASH>
          Hash printed by `mm dev services plan --operation repair`

      --yes
          Skip the service mutation confirmation prompt

  -h, --help
          Print help
```

#### `mm dev services remove`

```text
Remove only mm-owned runtime resources using an exact reviewed plan

Usage: remove [OPTIONS] --plan <HASH>

Options:
      --plan <HASH>
          Hash printed by `mm dev services plan --operation remove`

      --purge
          Delete retained Mailpit and ingress volumes; must match the reviewed plan

      --yes
          Skip the service mutation confirmation prompt

  -h, --help
          Print help
```

### `mm dev dashboard`

```text
Serve the local developer dashboard

Usage: dashboard <COMMAND>

Commands:
  serve  Run the ingress-authenticated dashboard backend
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm dev dashboard serve`

```text
Run the ingress-authenticated dashboard backend

Usage: serve [OPTIONS]

Options:
      --listen <ADDRESS>
          Control listener used by the managed ingress route

          [default: 0.0.0.0:42831]

  -h, --help
          Print help
```

### `mm dev mounts`

```text
Discover, review, and disable source-code mounts

Usage: mounts <COMMAND>

Commands:
  plan       Produce a deterministic mount proposal with discovery evidence
  configure  Persist the exact reviewed proposal in the project config
  disable    Remove a configured mount after reviewing its current plan
  help       Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm dev mounts plan`

```text
Produce a deterministic mount proposal with discovery evidence

Usage: plan [OPTIONS] <MODULE>

Arguments:
  <MODULE>


Options:
      --source <PATH>
          Override the module-relative portable source proposed by discovery

      --mount-target <PATH>
          Override the absolute container target proposed by discovery

      --service <SERVICE>
          Select a service explicitly; repeat for multiple services

      --preset <PRESET>
          Use a framework preservation preset explicitly

          [possible values: laravel, node, dotnet, rust]

      --read-only
          Mount source read-only instead of read-write

      --safe-exception <PATH>
          Explicitly approve one otherwise-sensitive module-relative path

  -h, --help
          Print help
```

#### `mm dev mounts configure`

```text
Persist the exact reviewed proposal in the project config

Usage: configure [OPTIONS] --plan <HASH> <MODULE>

Arguments:
  <MODULE>


Options:
      --source <PATH>
          Override the module-relative portable source proposed by discovery

      --mount-target <PATH>
          Override the absolute container target proposed by discovery

      --service <SERVICE>
          Select a service explicitly; repeat for multiple services

      --preset <PRESET>
          Use a framework preservation preset explicitly

          [possible values: laravel, node, dotnet, rust]

      --read-only
          Mount source read-only instead of read-write

      --safe-exception <PATH>
          Explicitly approve one otherwise-sensitive module-relative path

      --plan <HASH>
          Hash printed by `mm dev mounts plan` with identical options

  -h, --help
          Print help
```

#### `mm dev mounts disable`

```text
Remove a configured mount after reviewing its current plan

Usage: disable --plan <HASH> <MODULE>

Arguments:
  <MODULE>


Options:
      --plan <HASH>
          Hash printed by `mm dev mounts plan MODULE`

  -h, --help
          Print help
```

### `mm dev open`

```text
Print a configured local service URL

Usage: open [SERVICE]

Arguments:
  [SERVICE]


Options:
  -h, --help
          Print help
```

## `mm config`

```text
Validate and edit desired project configuration through typed reviews

Usage: config <COMMAND>

Commands:
  validate     Validate .config/mm.yaml, its split files, and local overrides, without probing a runtime
  module       Review typed module declarations
  service      Review typed service declarations
  stack        Review stack declarations or compile generated artifacts
  build        Review typed image-build declarations
  environment  Review typed environment declarations
  binding      Review typed environment bindings
  secret       Review secret declarations and references, never material
  ingress      Review ingress declarations or inspect compiled routes
  mount        Review typed development mount declarations
  job          Review typed finite-job declarations
  schedule     Review typed schedule declarations
  access       Review typed application-access declarations
  user         Review who may reach remote environments (the users: key)
  schema       Print the generated JSON Schema for .config/mm.yaml, or for one split file's value
  split        Plan moving a top-level key into its own .config/mm/KEY.yaml; apply only with --plan and --yes
  join         Plan moving a split key back into .config/mm.yaml; apply only with --plan and --yes
  sources      Show which file declares each top-level key
  help         Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

### `mm config validate`

```text
Validate .config/mm.yaml, its split files, and local overrides, without probing a runtime

Usage: validate [OPTIONS]

Options:
      --for-remote
          Also refuse Compose features a remote host blocks, in the services the environment runs; without --environment, in every environment whose target is not `local`

      --host-policy <FILE>
          With --for-remote: check against this copy of a host's host.yaml, its `allow` list and, without --environment, the environments it runs

      --strict
          Fail on keys this mm does not know, which it otherwise ignores with a warning; for CI

  -h, --help
          Print help
```

### `mm config module`

```text
Review typed module declarations

Usage: module <COMMAND>

Commands:
  add     Plan adding a new named resource; apply only with --plan and --yes
  edit    Plan replacing one named resource definition; apply only with --plan and --yes
  remove  Plan removing one named resource; apply only with --plan and --yes
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm config module add`

```text
Plan adding a new named resource; apply only with --plan and --yes

Usage: add [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config module edit`

```text
Plan replacing one named resource definition; apply only with --plan and --yes

Usage: edit [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config module remove`

```text
Plan removing one named resource; apply only with --plan and --yes

Usage: remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --plan <HASH>
          Exact hash printed by an earlier removal review

      --yes
          Apply the exact reviewed removal; never bypasses stale-file checks

  -h, --help
          Print help
```

### `mm config service`

```text
Review typed service declarations

Usage: service <COMMAND>

Commands:
  add     Plan adding a new named resource; apply only with --plan and --yes
  edit    Plan replacing one named resource definition; apply only with --plan and --yes
  remove  Plan removing one named resource; apply only with --plan and --yes
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm config service add`

```text
Plan adding a new named resource; apply only with --plan and --yes

Usage: add [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config service edit`

```text
Plan replacing one named resource definition; apply only with --plan and --yes

Usage: edit [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config service remove`

```text
Plan removing one named resource; apply only with --plan and --yes

Usage: remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --plan <HASH>
          Exact hash printed by an earlier removal review

      --yes
          Apply the exact reviewed removal; never bypasses stale-file checks

  -h, --help
          Print help
```

### `mm config stack`

```text
Review stack declarations or compile generated artifacts

Usage: stack <COMMAND>

Commands:
  add      Plan adding a stack declaration
  edit     Plan replacing a stack declaration
  remove   Plan removing a stack declaration
  compile  Publish deterministic Compose manifests and generated overlays
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm config stack add`

```text
Plan adding a stack declaration

Usage: add [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config stack edit`

```text
Plan replacing a stack declaration

Usage: edit [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config stack remove`

```text
Plan removing a stack declaration

Usage: remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --plan <HASH>
          Exact hash printed by an earlier removal review

      --yes
          Apply the exact reviewed removal; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config stack compile`

```text
Publish deterministic Compose manifests and generated overlays

Usage: compile

Options:
  -h, --help
          Print help
```

### `mm config build`

```text
Review typed image-build declarations

Usage: build <COMMAND>

Commands:
  add     Plan adding a new named resource; apply only with --plan and --yes
  edit    Plan replacing one named resource definition; apply only with --plan and --yes
  remove  Plan removing one named resource; apply only with --plan and --yes
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm config build add`

```text
Plan adding a new named resource; apply only with --plan and --yes

Usage: add [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config build edit`

```text
Plan replacing one named resource definition; apply only with --plan and --yes

Usage: edit [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config build remove`

```text
Plan removing one named resource; apply only with --plan and --yes

Usage: remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --plan <HASH>
          Exact hash printed by an earlier removal review

      --yes
          Apply the exact reviewed removal; never bypasses stale-file checks

  -h, --help
          Print help
```

### `mm config environment`

```text
Review typed environment declarations

Usage: environment <COMMAND>

Commands:
  add     Plan adding a new named resource; apply only with --plan and --yes
  edit    Plan replacing one named resource definition; apply only with --plan and --yes
  remove  Plan removing one named resource; apply only with --plan and --yes
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm config environment add`

```text
Plan adding a new named resource; apply only with --plan and --yes

Usage: add [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config environment edit`

```text
Plan replacing one named resource definition; apply only with --plan and --yes

Usage: edit [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config environment remove`

```text
Plan removing one named resource; apply only with --plan and --yes

Usage: remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --plan <HASH>
          Exact hash printed by an earlier removal review

      --yes
          Apply the exact reviewed removal; never bypasses stale-file checks

  -h, --help
          Print help
```

### `mm config binding`

```text
Review typed environment bindings

Usage: binding <COMMAND>

Commands:
  add     Plan adding a new named resource; apply only with --plan and --yes
  edit    Plan replacing one named resource definition; apply only with --plan and --yes
  remove  Plan removing one named resource; apply only with --plan and --yes
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm config binding add`

```text
Plan adding a new named resource; apply only with --plan and --yes

Usage: add [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config binding edit`

```text
Plan replacing one named resource definition; apply only with --plan and --yes

Usage: edit [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config binding remove`

```text
Plan removing one named resource; apply only with --plan and --yes

Usage: remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --plan <HASH>
          Exact hash printed by an earlier removal review

      --yes
          Apply the exact reviewed removal; never bypasses stale-file checks

  -h, --help
          Print help
```

### `mm config secret`

```text
Review secret declarations and references, never material

Usage: secret <COMMAND>

Commands:
  add     Plan adding a new named resource; apply only with --plan and --yes
  edit    Plan replacing one named resource definition; apply only with --plan and --yes
  remove  Plan removing one named resource; apply only with --plan and --yes
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm config secret add`

```text
Plan adding a new named resource; apply only with --plan and --yes

Usage: add [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config secret edit`

```text
Plan replacing one named resource definition; apply only with --plan and --yes

Usage: edit [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config secret remove`

```text
Plan removing one named resource; apply only with --plan and --yes

Usage: remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --plan <HASH>
          Exact hash printed by an earlier removal review

      --yes
          Apply the exact reviewed removal; never bypasses stale-file checks

  -h, --help
          Print help
```

### `mm config ingress`

```text
Review ingress declarations or inspect compiled routes

Usage: ingress <COMMAND>

Commands:
  add      Plan adding an ingress declaration
  edit     Plan replacing an ingress declaration
  remove   Plan removing an ingress declaration
  list     List every service URL or the reason it was not routed
  explain  Explain ingress selection for one service name
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm config ingress add`

```text
Plan adding an ingress declaration

Usage: add [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config ingress edit`

```text
Plan replacing an ingress declaration

Usage: edit [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config ingress remove`

```text
Plan removing an ingress declaration

Usage: remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --plan <HASH>
          Exact hash printed by an earlier removal review

      --yes
          Apply the exact reviewed removal; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config ingress list`

```text
List every service URL or the reason it was not routed

Usage: list

Options:
  -h, --help
          Print help
```

#### `mm config ingress explain`

```text
Explain ingress selection for one service name

Usage: explain <SERVICE>

Arguments:
  <SERVICE>


Options:
  -h, --help
          Print help
```

### `mm config mount`

```text
Review typed development mount declarations

Usage: mount <COMMAND>

Commands:
  add     Plan adding a new named resource; apply only with --plan and --yes
  edit    Plan replacing one named resource definition; apply only with --plan and --yes
  remove  Plan removing one named resource; apply only with --plan and --yes
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm config mount add`

```text
Plan adding a new named resource; apply only with --plan and --yes

Usage: add [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config mount edit`

```text
Plan replacing one named resource definition; apply only with --plan and --yes

Usage: edit [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config mount remove`

```text
Plan removing one named resource; apply only with --plan and --yes

Usage: remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --plan <HASH>
          Exact hash printed by an earlier removal review

      --yes
          Apply the exact reviewed removal; never bypasses stale-file checks

  -h, --help
          Print help
```

### `mm config job`

```text
Review typed finite-job declarations

Usage: job <COMMAND>

Commands:
  add     Plan adding a new named resource; apply only with --plan and --yes
  edit    Plan replacing one named resource definition; apply only with --plan and --yes
  remove  Plan removing one named resource; apply only with --plan and --yes
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm config job add`

```text
Plan adding a new named resource; apply only with --plan and --yes

Usage: add [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config job edit`

```text
Plan replacing one named resource definition; apply only with --plan and --yes

Usage: edit [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config job remove`

```text
Plan removing one named resource; apply only with --plan and --yes

Usage: remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --plan <HASH>
          Exact hash printed by an earlier removal review

      --yes
          Apply the exact reviewed removal; never bypasses stale-file checks

  -h, --help
          Print help
```

### `mm config schedule`

```text
Review typed schedule declarations

Usage: schedule <COMMAND>

Commands:
  add     Plan adding a new named resource; apply only with --plan and --yes
  edit    Plan replacing one named resource definition; apply only with --plan and --yes
  remove  Plan removing one named resource; apply only with --plan and --yes
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm config schedule add`

```text
Plan adding a new named resource; apply only with --plan and --yes

Usage: add [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config schedule edit`

```text
Plan replacing one named resource definition; apply only with --plan and --yes

Usage: edit [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config schedule remove`

```text
Plan removing one named resource; apply only with --plan and --yes

Usage: remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --plan <HASH>
          Exact hash printed by an earlier removal review

      --yes
          Apply the exact reviewed removal; never bypasses stale-file checks

  -h, --help
          Print help
```

### `mm config access`

```text
Review typed application-access declarations

Usage: access <COMMAND>

Commands:
  add     Plan adding a new named resource; apply only with --plan and --yes
  edit    Plan replacing one named resource definition; apply only with --plan and --yes
  remove  Plan removing one named resource; apply only with --plan and --yes
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm config access add`

```text
Plan adding a new named resource; apply only with --plan and --yes

Usage: add [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config access edit`

```text
Plan replacing one named resource definition; apply only with --plan and --yes

Usage: edit [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config access remove`

```text
Plan removing one named resource; apply only with --plan and --yes

Usage: remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --plan <HASH>
          Exact hash printed by an earlier removal review

      --yes
          Apply the exact reviewed removal; never bypasses stale-file checks

  -h, --help
          Print help
```

### `mm config user`

```text
Review who may reach remote environments (the users: key)

Usage: user <COMMAND>

Commands:
  add     Plan adding a new named resource; apply only with --plan and --yes
  edit    Plan replacing one named resource definition; apply only with --plan and --yes
  remove  Plan removing one named resource; apply only with --plan and --yes
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm config user add`

```text
Plan adding a new named resource; apply only with --plan and --yes

Usage: add [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config user edit`

```text
Plan replacing one named resource definition; apply only with --plan and --yes

Usage: edit [OPTIONS] --definition <DOCUMENT> <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --definition <DOCUMENT>
          Public JSON or YAML definition. Secret plaintext is rejected

      --plan <HASH>
          Exact hash printed by an earlier review with identical arguments

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

#### `mm config user remove`

```text
Plan removing one named resource; apply only with --plan and --yes

Usage: remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          Portable resource name

Options:
      --parent <NAME>
          Owning module, stack, or environment when the resource is nested

      --plan <HASH>
          Exact hash printed by an earlier removal review

      --yes
          Apply the exact reviewed removal; never bypasses stale-file checks

  -h, --help
          Print help
```

### `mm config schema`

```text
Print the generated JSON Schema for .config/mm.yaml, or for one split file's value

Usage: schema [KEY]

Arguments:
  [KEY]
          Top-level key whose split-file schema to print, for .config/mm/KEY.yaml

          [possible values: project, default-environment, target, runtime, build, networks, stacks, modules, providers, secrets, bindings, environment-delivery, environments, ingress, development-mounts, development-mail, jobs, schedules, gates, users, agents, targets, registries, hardening, release-inputs]

Options:
  -h, --help
          Print help
```

### `mm config split`

```text
Plan moving a top-level key into its own .config/mm/KEY.yaml; apply only with --plan and --yes

Usage: split [OPTIONS] <KEY>

Arguments:
  <KEY>
          Top-level configuration key, such as stacks or environments

          [possible values: project, default-environment, target, runtime, build, networks, stacks, modules, providers, secrets, bindings, environment-delivery, environments, ingress, development-mounts, development-mail, jobs, schedules, gates, users, agents, targets, registries, hardening, release-inputs]

Options:
      --plan <HASH>
          Exact hash printed by an earlier review of the same move

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

### `mm config join`

```text
Plan moving a split key back into .config/mm.yaml; apply only with --plan and --yes

Usage: join [OPTIONS] <KEY>

Arguments:
  <KEY>
          Top-level configuration key, such as stacks or environments

          [possible values: project, default-environment, target, runtime, build, networks, stacks, modules, providers, secrets, bindings, environment-delivery, environments, ingress, development-mounts, development-mail, jobs, schedules, gates, users, agents, targets, registries, hardening, release-inputs]

Options:
      --plan <HASH>
          Exact hash printed by an earlier review of the same move

      --yes
          Apply the exact reviewed plan; never bypasses stale-file checks

  -h, --help
          Print help
```

### `mm config sources`

```text
Show which file declares each top-level key

Usage: sources

Options:
  -h, --help
          Print help
```

## `mm doctor`

```text
Check configuration health and resolved runtime capabilities

Usage: doctor [OPTIONS] [COMMAND]

Commands:
  recover-state  Review recovery of pre-baseline state; apply requires the reviewed hash and --yes
  help           Print this message or the help of the given subcommand(s)

Options:
      --check-updates
          Fetch the stable release manifest and compare the installed version

  -h, --help
          Print help

Examples:
  mm doctor --output json
  mm doctor recover-state

Inspection is read-only. State recovery applies only with an exact reviewed hash and --yes. Exit codes: 0 healthy; 1 state/I/O failure; 2 invalid configuration; 3 unavailable runtime or capability.
```

### `mm doctor recover-state`

```text
Review recovery of pre-baseline state; apply requires the reviewed hash and --yes

Usage: recover-state [OPTIONS]

Options:
      --plan <HASH>
          Exact hash from the recovery review

      --yes
          Confirm backup and replacement of the reviewed legacy state

  -h, --help
          Print help
```

## `mm ingress`

```text
Start or stop this workstation's global public HTTP/HTTPS forwarding

Usage: ingress <COMMAND>

Commands:
  start    Ensure ingress is installed and healthy, then start public forwarding on ports 80 and 443
  stop     Stop global public forwarding and keep it stopped across reboots and deploys; keep project containers running
  status   Show public forwarding state and the health of the services it depends on (read-only)
  restart  Reconcile ingress prerequisites, then restart public forwarding on ports 80 and 443
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help

Examples:
  mm ingress stop
  mm ingress start

Global to this workstation; no project is required. Start ensures the required host helper, TLS, and shared ingress are installed, offering reviewed installation or repair when necessary. Stop only stops public forwarding. Interactive use requests sudo authorization when needed; JSON/noninteractive installation requires --yes and never prompts. DNS is configured by install.sh workstation setup.
```

### `mm ingress start`

```text
Ensure ingress is installed and healthy, then start public forwarding on ports 80 and 443

Usage: start [OPTIONS]

Options:
      --yes
          Approve required installation or repair plans without prompting

  -h, --help
          Print help
```

### `mm ingress stop`

```text
Stop global public forwarding and keep it stopped across reboots and deploys; keep project containers running

Usage: stop

Options:
  -h, --help
          Print help
```

### `mm ingress status`

```text
Show public forwarding state and the health of the services it depends on (read-only)

Usage: status

Options:
  -h, --help
          Print help
```

### `mm ingress restart`

```text
Reconcile ingress prerequisites, then restart public forwarding on ports 80 and 443

Usage: restart [OPTIONS]

Options:
      --yes
          Approve required installation or repair plans without prompting

  -h, --help
          Print help
```

## `mm service`

```text
Inspect and operate deployed services

Usage: service <COMMAND>

Commands:
  status   Show desired and observed state, optionally for one service
  logs     Read a service's log, or every service's, from all its containers in the order the lines were written
  exec     Execute an exact argv vector; no shell is implied
  shell    Open an explicitly approved high-risk shell
  restart  Restart one service through runtime policy
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

### `mm service status`

```text
Show desired and observed state, optionally for one service

Usage: status [SERVICE]

Arguments:
  [SERVICE]


Options:
  -h, --help
          Print help
```

### `mm service logs`

```text
Read a service's log, or every service's, from all its containers in the order the lines were written

Usage: logs [OPTIONS] [SERVICE]

Arguments:
  [SERVICE]
          Service whose containers are read. Leave it out to read every service of the environment

Options:
      --stack <STACK>
          Restrict the service to one configured stack

      --node <NODE>
          On a cluster target, read only this node. Repeat it for more

      --follow
          Continue streaming new records until interrupted

      --tail <COUNT>
          Number of recent records requested before following

          [default: 100]

      --since <UNIX_MS>
          Include records at or after this Unix millisecond timestamp

      --until <UNIX_MS>
          Include records before this Unix millisecond timestamp

      --timestamps
          Show when the engine recorded each line

      --stdout
          Include only standard output records

      --stderr
          Include only standard error records

      --max-line-bytes <MAX_LINE_BYTES>
          Maximum bytes retained from any one log line

          [default: 65536]

      --max-bytes-per-second <MAX_BYTES_PER_SECOND>
          Maximum aggregate log bytes accepted per second

          [default: 1048576]

      --max-lines-per-second <MAX_LINES_PER_SECOND>
          Maximum aggregate log records accepted per second

          [default: 10000]

      --raw
          Disable redaction. Requires --approve-high-risk

      --approve-high-risk
          Confirm that raw application logs can disclose arbitrary secret values

  -h, --help
          Print help
```

### `mm service exec`

```text
Execute an exact argv vector; no shell is implied

Usage: exec [OPTIONS] <SERVICE> -- <COMMAND>...

Arguments:
  <SERVICE>
          Service containing the command target

  <COMMAND>...
          Exact executable and argument vector; no shell is implied

Options:
      --stack <STACK>
          Restrict the service to one configured stack

      --container <CONTAINER>
          Select one exact container when the service has multiple replicas

      --user <USER>
          User or numeric identity used inside the container

      --workdir <PATH>
          Working directory used inside the container

      --env <NAME=REFERENCE>
          Set NAME from a secret reference (NAME=provider://location); plaintext is rejected

      --tty
          Allocate an interactive terminal; output capture is disabled

      --stdin
          Forward standard input to the remote command

      --timeout <SECONDS>
          Maximum command runtime before cancellation

          [default: 30]

  -h, --help
          Print help
```

### `mm service shell`

```text
Open an explicitly approved high-risk shell

Usage: shell [OPTIONS] --approve-high-risk <SERVICE>

Arguments:
  <SERVICE>
          Service in which to start the interactive shell

Options:
      --stack <STACK>
          Restrict the service to one configured stack

      --container <CONTAINER>
          Select one exact container when the service has multiple replicas

      --user <USER>
          User or numeric identity used inside the container

      --workdir <PATH>
          Working directory used inside the container

      --approve-high-risk
          Confirm the shell's high-risk, authorization-gated behavior

  -h, --help
          Print help
```

### `mm service restart`

```text
Restart one service through runtime policy

Usage: restart <SERVICE>

Arguments:
  <SERVICE>


Options:
  -h, --help
          Print help
```

## `mm job`

```text
Run finite jobs and operate their schedules

Usage: job <COMMAND>

Commands:
  list      List the declared jobs of the environment: when a deployment runs each, on which node, and how its last run ended
  run       Enqueue a named job and wait for its durable terminal status
  status    Show one durable run by UUID
  history   List bounded run history, optionally for one job
  cancel    Request cancellation of a queued or active run
  retry     Enqueue a new run from the current definition of a terminal run
  schedule  Operate schedules for configured jobs
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

### `mm job list`

```text
List the declared jobs of the environment: when a deployment runs each, on which node, and how its last run ended

Usage: list

Options:
  -h, --help
          Print help
```

### `mm job run`

```text
Enqueue a named job and wait for its durable terminal status

Usage: run [OPTIONS] <JOB>

Arguments:
  <JOB>


Options:
      --no-wait
          Return after durable enqueue instead of driving execution

  -h, --help
          Print help
```

### `mm job status`

```text
Show one durable run by UUID

Usage: status <RUN_ID>

Arguments:
  <RUN_ID>


Options:
  -h, --help
          Print help
```

### `mm job history`

```text
List bounded run history, optionally for one job

Usage: history [OPTIONS] [JOB]

Arguments:
  [JOB]


Options:
      --limit <LIMIT>
          Maximum number of newest runs to return

          [default: 100]

  -h, --help
          Print help
```

### `mm job cancel`

```text
Request cancellation of a queued or active run

Usage: cancel <RUN_ID>

Arguments:
  <RUN_ID>


Options:
  -h, --help
          Print help
```

### `mm job retry`

```text
Enqueue a new run from the current definition of a terminal run

Usage: retry [OPTIONS] <RUN_ID>

Arguments:
  <RUN_ID>


Options:
      --no-wait
          Return after durable enqueue instead of driving execution

  -h, --help
          Print help
```

### `mm job schedule`

```text
Operate schedules for configured jobs

Usage: schedule <COMMAND>

Commands:
  reconcile  Plan or apply transactional definition reconciliation
  list       List schedules and their durable next due time
  show       Show one schedule
  enable     Enable one reconciled schedule
  disable    Disable one reconciled schedule without deleting history
  run-now    Enqueue the schedule's job immediately, independent of enabled state
  next       Show the durable next due time for one schedule
  history    List bounded history for one schedule
  help       Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm job schedule reconcile`

```text
Plan or apply transactional definition reconciliation

Usage: reconcile [OPTIONS]

Options:
      --plan <HASH>
          Exact hash printed by the schedule reconciliation review

      --yes
          Apply the reviewed reconciliation plan

  -h, --help
          Print help
```

#### `mm job schedule list`

```text
List schedules and their durable next due time

Usage: list

Options:
  -h, --help
          Print help
```

#### `mm job schedule show`

```text
Show one schedule

Usage: show <NAME>

Arguments:
  <NAME>


Options:
  -h, --help
          Print help
```

#### `mm job schedule enable`

```text
Enable one reconciled schedule

Usage: enable <NAME>

Arguments:
  <NAME>


Options:
  -h, --help
          Print help
```

#### `mm job schedule disable`

```text
Disable one reconciled schedule without deleting history

Usage: disable <NAME>

Arguments:
  <NAME>


Options:
  -h, --help
          Print help
```

#### `mm job schedule run-now`

```text
Enqueue the schedule's job immediately, independent of enabled state

Usage: run-now [OPTIONS] <NAME>

Arguments:
  <NAME>


Options:
      --no-wait
          Return after durable enqueue instead of driving execution

  -h, --help
          Print help
```

#### `mm job schedule next`

```text
Show the durable next due time for one schedule

Usage: next <NAME>

Arguments:
  <NAME>


Options:
  -h, --help
          Print help
```

#### `mm job schedule history`

```text
List bounded history for one schedule

Usage: history [OPTIONS] <NAME>

Arguments:
  <NAME>


Options:
      --limit <LIMIT>
          Maximum number of newest runs to return

          [default: 100]

  -h, --help
          Print help
```

## `mm env`

```text
Inspect and deliver non-secret environment values

Usage: env <COMMAND>

Commands:
  status       Show effective values, provenance, and secret status
  explain      Explain every use of one global key or local variable
  set          Set a workspace-local non-secret override. On a remote environment the value is staged on every host for the next deployment
  unset        Remove a workspace-local override. On a remote environment the removal is staged for the next deployment
  discard      Drop a variable change staged on a remote environment's hosts before it is deployed
  default      Get or change the project-wide default environment
  check        Resolve and compile every service environment in memory
  materialize  Explicitly write a module's conventional .env file
  diff         Compare a module's .env with the effective compiled environment
  review       Explain source and delivery without revealing secret values
  setup        Show missing bindings and the resolved delivery mode
  delivery     Show or persist environment delivery mode
  help         Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

### `mm env status`

```text
Show effective values, provenance, and secret status

Usage: status [OPTIONS]

Options:
      --no-check-secrets
          Do not contact secret providers

  -h, --help
          Print help
```

### `mm env explain`

```text
Explain every use of one global key or local variable

Usage: explain [OPTIONS] <KEY>

Arguments:
  <KEY>


Options:
      --no-check-secrets
          Do not contact secret providers

  -h, --help
          Print help
```

### `mm env set`

```text
Set a workspace-local non-secret override. On a remote environment the value is staged on every host for the next deployment

Usage: set [OPTIONS] <KEY> [VALUE]

Arguments:
  <KEY>


  [VALUE]


Options:
      --stdin
          Read the value from stdin instead of an argument

      --expected-revision <EXPECTED_REVISION>
          Fail if the existing override has another revision

  -h, --help
          Print help
```

### `mm env unset`

```text
Remove a workspace-local override. On a remote environment the removal is staged for the next deployment

Usage: unset <KEY>

Arguments:
  <KEY>


Options:
  -h, --help
          Print help
```

### `mm env discard`

```text
Drop a variable change staged on a remote environment's hosts before it is deployed

Usage: discard <KEY>

Arguments:
  <KEY>


Options:
  -h, --help
          Print help
```

### `mm env default`

```text
Get or change the project-wide default environment

Usage: default <COMMAND>

Commands:
  show   Print the configured project default environment
  set    Persist a new project default environment
  unset  Remove the project default environment
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm env default show`

```text
Print the configured project default environment

Usage: show

Options:
  -h, --help
          Print help
```

#### `mm env default set`

```text
Persist a new project default environment

Usage: set <ENVIRONMENT>

Arguments:
  <ENVIRONMENT>


Options:
  -h, --help
          Print help
```

#### `mm env default unset`

```text
Remove the project default environment

Usage: unset

Options:
  -h, --help
          Print help
```

### `mm env check`

```text
Resolve and compile every service environment in memory

Usage: check

Options:
  -h, --help
          Print help
```

### `mm env materialize`

```text
Explicitly write a module's conventional .env file

Usage: materialize [OPTIONS] <MODULE>

Arguments:
  <MODULE>


Options:
      --service <SERVICE>
          Limit materialization to one service in the module

      --force
          Overwrite an existing .env file

  -h, --help
          Print help
```

### `mm env diff`

```text
Compare a module's .env with the effective compiled environment

Usage: diff [OPTIONS] <MODULE>

Arguments:
  <MODULE>


Options:
      --service <SERVICE>
          Limit the diff to one service in the module

  -h, --help
          Print help
```

### `mm env review`

```text
Explain source and delivery without revealing secret values

Usage: review

Options:
  -h, --help
          Print help
```

### `mm env setup`

```text
Show missing bindings and the resolved delivery mode

Usage: setup

Options:
  -h, --help
          Print help
```

### `mm env delivery`

```text
Show or persist environment delivery mode

Usage: delivery <COMMAND>

Commands:
  show  Show the resolved delivery mode and provenance
  set   Persist an explicit delivery mode for one environment
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm env delivery show`

```text
Show the resolved delivery mode and provenance

Usage: show

Options:
  -h, --help
          Print help
```

#### `mm env delivery set`

```text
Persist an explicit delivery mode for one environment

Usage: set <ENVIRONMENT> <MODE>

Arguments:
  <ENVIRONMENT>


  <MODE>
          [possible values: direct, env-file]

Options:
  -h, --help
          Print help
```

## `mm secret`

```text
Manage secret values, rotation, and provider instances

Usage: secret <COMMAND>

Commands:
  providers  List available provider capabilities
  init       Initialize a secret provider for this project
  status     Show secret set/unset/default state without revealing values. On a remote environment: what each host holds and has staged
  set        Set a configured secret using a hidden prompt or stdin. On a remote environment the value is staged on every host for the next deployment
  unset      Remove a configured secret. On a remote environment the removal is staged for the next deployment
  discard    Drop a change staged on a remote environment's hosts before it is deployed
  generate   Create configured generated secrets without overwriting existing values
  rotate     Explicitly replace one secret using its configured source
  migrate    Move a cluster environment's secrets from the age store into the store mm runs on its nodes. Each becomes a version there with the revision it has now, so pointing the secrets at the store afterwards replaces no container. Safe to run again
  rotation   Plan or apply scheduled rotations
  provider   Manage OpenBao and Vault provider instances
  help       Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

### `mm secret providers`

```text
List available provider capabilities

Usage: providers

Options:
  -h, --help
          Print help
```

### `mm secret init`

```text
Initialize a secret provider for this project

Usage: init <PROVIDER>

Arguments:
  <PROVIDER>
          [possible values: age]

Options:
  -h, --help
          Print help
```

### `mm secret status`

```text
Show secret set/unset/default state without revealing values. On a remote environment: what each host holds and has staged

Usage: status [OPTIONS] [KEY]

Arguments:
  [KEY]


Options:
      --local
          Read this machine's store, the one `mm target setup` pushes, even for a remote environment

  -h, --help
          Print help
```

### `mm secret set`

```text
Set a configured secret using a hidden prompt or stdin. On a remote environment the value is staged on every host for the next deployment

Usage: set [OPTIONS] <KEY>

Arguments:
  <KEY>


Options:
      --stdin
          Read exactly one value from stdin without prompting

      --local
          Write this machine's store, the one `mm target setup` pushes, even for a remote environment

  -h, --help
          Print help
```

### `mm secret unset`

```text
Remove a configured secret. On a remote environment the removal is staged for the next deployment

Usage: unset [OPTIONS] <KEY>

Arguments:
  <KEY>


Options:
      --yes
          Skip the destructive confirmation prompt

      --local
          Remove it from this machine's store, even for a remote environment

  -h, --help
          Print help
```

### `mm secret discard`

```text
Drop a change staged on a remote environment's hosts before it is deployed

Usage: discard <KEY>

Arguments:
  <KEY>


Options:
  -h, --help
          Print help
```

### `mm secret generate`

```text
Create configured generated secrets without overwriting existing values.

With --missing or --all, a secret whose provider has no connection from this machine is listed as skipped with the reason, and the others are still created.

Usage: generate <KEY|--missing|--all>

Arguments:
  [KEY]
          Logical secret name to generate

Options:
      --missing
          Generate the secrets the selected environment uses: those its bindings name, its services mount, and its registries, TLS, tunnel and deploy key read

      --all
          Generate every configured generated secret, whichever environment uses it

  -h, --help
          Print help (see a summary with '-h')
```

### `mm secret rotate`

```text
Explicitly replace one secret using its configured source

Usage: rotate [OPTIONS] <KEY>

Arguments:
  <KEY>


Options:
      --stdin
          Read a prompted replacement from stdin

      --yes
          Apply the exact displayed rotation plan without an interactive confirmation

  -h, --help
          Print help
```

### `mm secret migrate`

```text
Move a cluster environment's secrets from the age store into the store mm runs on its nodes. Each becomes a version there with the revision it has now, so pointing the secrets at the store afterwards replaces no container. Safe to run again

Usage: migrate --from <FROM> --to <NAME>

Options:
      --from <FROM>
          The store the values come from

          [possible values: age]

      --to <NAME>
          The managed store's name under `providers:`

  -h, --help
          Print help
```

### `mm secret rotation`

```text
Plan or apply scheduled rotations

Usage: rotation <COMMAND>

Commands:
  plan   Inspect metadata and produce an approval hash without changing secrets
  apply  Apply the exact reviewed plan using revision-guarded writes
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm secret rotation plan`

```text
Inspect metadata and produce an approval hash without changing secrets

Usage: plan

Options:
  -h, --help
          Print help
```

#### `mm secret rotation apply`

```text
Apply the exact reviewed plan using revision-guarded writes

Usage: apply [OPTIONS] --plan <HASH>

Options:
      --plan <HASH>
          Hash printed by `mm secret rotation plan`

      --yes
          Skip the confirmation prompt

  -h, --help
          Print help
```

### `mm secret provider`

```text
Manage OpenBao and Vault provider instances

Usage: provider <COMMAND>

Commands:
  list      List configured provider instances and registered drivers
  show      Show one provider instance with its typed descriptor and capabilities
  add       Configure a declared provider instance
  edit      Reconfigure a declared provider instance without changing its driver or mode
  test      Test one external provider instance
  status    Show health and capabilities for one provider instance
  plan      Produce a deterministic provider lifecycle plan
  apply     Apply an exact provider lifecycle plan
  backup    Write an encrypted provider snapshot after exact review
  restore   Restore an encrypted provider snapshot after exact review. For the store mm runs on a cluster's nodes: put a snapshot from `snapshot` back on every node; without --plan it shows the plan
  snapshot  Take a snapshot of the store mm runs on a cluster environment's nodes: its data and the keys it opens with, sealed to you
  recovery  The recovery copy of the store mm runs on a cluster environment's nodes
  remove    Remove a provider through its exact reviewed lifecycle plan
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm secret provider list`

```text
List configured provider instances and registered drivers

Usage: list

Options:
  -h, --help
          Print help
```

#### `mm secret provider show`

```text
Show one provider instance with its typed descriptor and capabilities

Usage: show <NAME>

Arguments:
  <NAME>
          Stable provider instance name

Options:
  -h, --help
          Print help
```

#### `mm secret provider add`

```text
Configure a declared provider instance

Usage: add [OPTIONS] --driver <DRIVER> --mode <MODE> <NAME>

Arguments:
  <NAME>
          Stable provider instance name already declared in mm.yaml

Options:
      --driver <DRIVER>
          Registered provider driver ID

      --mode <MODE>
          Provider operating mode

          [possible values: managed, external]

      --scope <SCOPE>
          Provider ownership scope

          [possible values: installation, project]

      --bootstrap
          Enable missing mounts for an external provider

      --stdin
          Read an external bootstrap credential from stdin instead of a hidden prompt

      --yes
          Apply after displaying the exact setup or install plan

  -h, --help
          Print help
```

#### `mm secret provider edit`

```text
Reconfigure a declared provider instance without changing its driver or mode

Usage: edit [OPTIONS] <NAME>

Arguments:
  <NAME>
          Stable provider instance name

Options:
      --driver <DRIVER>
          Assert the existing driver; changing it requires replacement

      --mode <MODE>
          Assert the existing mode; changing it requires replacement

          [possible values: managed, external]

      --bootstrap
          Enable missing mounts for an external provider

      --stdin
          Read the bootstrap credential from stdin instead of a hidden prompt

      --yes
          Apply after displaying the exact setup plan

  -h, --help
          Print help
```

#### `mm secret provider test`

```text
Test one external provider instance

Usage: test <NAME>

Arguments:
  <NAME>
          Stable external provider instance name

Options:
  -h, --help
          Print help
```

#### `mm secret provider status`

```text
Show health and capabilities for one provider instance

Usage: status <NAME>

Arguments:
  <NAME>
          Stable provider instance name

Options:
  -h, --help
          Print help
```

#### `mm secret provider plan`

```text
Produce a deterministic provider lifecycle plan

Usage: plan [OPTIONS] --operation <OPERATION> <NAME>

Arguments:
  <NAME>
          Stable managed provider instance name

Options:
      --operation <OPERATION>
          Lifecycle operation represented by the plan

          [possible values: install, restart, upgrade, seal, unseal, remove]

      --purge
          Delete owned persistent data when removing the provider

  -h, --help
          Print help
```

#### `mm secret provider apply`

```text
Apply an exact provider lifecycle plan

Usage: apply [OPTIONS] --operation <OPERATION> --plan <HASH> <NAME>

Arguments:
  <NAME>
          Stable managed provider instance name

Options:
      --operation <OPERATION>
          Lifecycle operation represented by the plan

          [possible values: install, restart, upgrade, seal, unseal, remove]

      --purge
          Delete owned persistent data when removing the provider

      --plan <HASH>
          Hash printed by the identical provider lifecycle plan

      --yes
          Apply the exact reviewed plan without prompting

  -h, --help
          Print help
```

#### `mm secret provider backup`

```text
Write an encrypted provider snapshot after exact review

Usage: backup [OPTIONS] --file <PATH> <NAME>

Arguments:
  <NAME>
          Stable managed provider instance name

Options:
      --file <PATH>
          Encrypted snapshot source or destination

      --plan <HASH>
          Hash for the exact current state and selected path

      --yes
          Apply the exact reviewed backup or restore

  -h, --help
          Print help
```

#### `mm secret provider restore`

```text
Restore an encrypted provider snapshot after exact review. For the store mm runs on a cluster's nodes: put a snapshot from `snapshot` back on every node; without --plan it shows the plan

Usage: restore [OPTIONS] --file <PATH> <NAME>

Arguments:
  <NAME>
          Stable managed provider instance name

Options:
      --file <PATH>
          Encrypted snapshot source or destination

      --plan <HASH>
          Hash for the exact current state and selected path

      --yes
          Apply the exact reviewed backup or restore

  -h, --help
          Print help
```

#### `mm secret provider snapshot`

```text
Take a snapshot of the store mm runs on a cluster environment's nodes: its data and the keys it opens with, sealed to you

Usage: snapshot [OPTIONS] <NAME>

Arguments:
  <NAME>
          The store's name under `providers:`

Options:
      --to <FILE>
          Write it to this new file instead of standard output

  -h, --help
          Print help
```

#### `mm secret provider recovery`

```text
The recovery copy of the store mm runs on a cluster environment's nodes

Usage: recovery <COMMAND>

Commands:
  export  Write the recovery copy of an environment's managed store (its unseal key, mmd's logins and its certificate authority), sealed with age to you or to another operator, as ASCII armour
  import  Keep a recovery copy another machine exported to you, so setup from this machine can give a reinstalled node its keys
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

##### `mm secret provider recovery export`

```text
Write the recovery copy of an environment's managed store (its unseal key, mmd's logins and its certificate authority), sealed with age to you or to another operator, as ASCII armour

Usage: export [OPTIONS] <NAME>

Arguments:
  <NAME>
          The store's name under `providers:`

Options:
      --recipient <AGE_RECIPIENT>
          Seal it to this age recipient (`age1…`) instead of to this machine's identity

      --to <FILE>
          Write it to this new file instead of standard output

  -h, --help
          Print help
```

##### `mm secret provider recovery import`

```text
Keep a recovery copy another machine exported to you, so setup from this machine can give a reinstalled node its keys

Usage: import <NAME> <FILE>

Arguments:
  <NAME>
          The store's name under `providers:`

  <FILE>
          The file `recovery export --recipient` wrote

Options:
  -h, --help
          Print help
```

#### `mm secret provider remove`

```text
Remove a provider through its exact reviewed lifecycle plan

Usage: remove [OPTIONS] --plan <HASH> <NAME>

Arguments:
  <NAME>
          Stable managed provider instance name

Options:
      --plan <HASH>
          Hash printed by the corresponding remove plan

      --yes
          Apply the exact reviewed removal

      --purge
          Delete provider-owned persistent data

  -h, --help
          Print help
```

## `mm access`

```text
Manage application access policies, users, routes, and sessions

Usage: access <COMMAND>

Commands:
  add      Review or create a password-cap policy using the managed local Cap service
  remove   Review or remove an unattached policy and its owned secret material
  show     Show redacted policy metadata
  attach   Review or attach one policy middleware to an owned router
  detach   Review or detach one policy middleware from an owned router
  user     Manage application-access principals (separate from deployment RBAC users)
  session  List, revoke, or rotate opaque sessions
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

### `mm access add`

```text
Review or create a password-cap policy using the managed local Cap service

Usage: add [OPTIONS] <POLICY>

Arguments:
  <POLICY>
          Unique policy name in the selected project and environment

Options:
      --mode <MODE>
          Driver evaluation mode for this policy

          [default: any]
          [possible values: any, all]

      --session-days <SESSION_DAYS>
          Session lifetime in days (default seven, maximum thirty)

          [default: 7]

      --secret-provider <SECRET_PROVIDER>
          Secret storage provider for generated gate keys and pepper material

          [default: age]

      --plan <HASH>
          Exact hash printed by the read-only policy review

      --yes
          Apply the exact reviewed policy plan

  -h, --help
          Print help
```

### `mm access remove`

```text
Review or remove an unattached policy and its owned secret material

Usage: remove [OPTIONS] <POLICY>

Arguments:
  <POLICY>
          Policy to remove

Options:
      --plan <HASH>
          Exact hash printed by the read-only removal review

      --yes
          Apply the exact reviewed removal plan

  -h, --help
          Print help
```

### `mm access show`

```text
Show redacted policy metadata

Usage: show [POLICY]

Arguments:
  [POLICY]
          Optional policy name; omit to show every policy in the selected scope

Options:
  -h, --help
          Print help
```

### `mm access attach`

```text
Review or attach one policy middleware to an owned router

Usage: attach [OPTIONS] --target-id <UUID> <POLICY> <ROUTER>

Arguments:
  <POLICY>
          Policy to attach or detach

  <ROUTER>
          Owned ingress router to update

Options:
      --target-id <UUID>
          Target that owns the ingress router

      --plan <HASH>
          Exact hash printed by the read-only route review

      --yes
          Apply the exact reviewed route plan

  -h, --help
          Print help
```

### `mm access detach`

```text
Review or detach one policy middleware from an owned router

Usage: detach [OPTIONS] --target-id <UUID> <POLICY> <ROUTER>

Arguments:
  <POLICY>
          Policy to attach or detach

  <ROUTER>
          Owned ingress router to update

Options:
      --target-id <UUID>
          Target that owns the ingress router

      --plan <HASH>
          Exact hash printed by the read-only route review

      --yes
          Apply the exact reviewed route plan

  -h, --help
          Print help
```

### `mm access user`

```text
Manage application-access principals (separate from deployment RBAC users)

Usage: user <COMMAND>

Commands:
  add             Review or create an application-access user with a hidden password
  disable         Review or disable an application-access user and revoke active sessions
  enable          Review or re-enable an application-access user
  password-reset  Review or replace an application-access user's hidden password
  remove          Review or remove an application-access user and revoke active sessions
  list            Show redacted application-user metadata for one policy
  help            Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm access user add`

```text
Review or create an application-access user with a hidden password

Usage: add [OPTIONS] <POLICY> <USERNAME>

Arguments:
  <POLICY>
          Policy that owns the application user

  <USERNAME>
          Application-access username

Options:
      --stdin
          Read the password from standard input instead of prompting

      --plan <HASH>
          Exact hash printed by the read-only user review

      --yes
          Apply the exact reviewed user plan

  -h, --help
          Print help
```

#### `mm access user disable`

```text
Review or disable an application-access user and revoke active sessions

Usage: disable [OPTIONS] <POLICY> <USERNAME>

Arguments:
  <POLICY>
          Policy that owns the application user

  <USERNAME>
          Application-access username

Options:
      --plan <HASH>
          Exact hash printed by the read-only user review

      --yes
          Apply the exact reviewed user plan

  -h, --help
          Print help
```

#### `mm access user enable`

```text
Review or re-enable an application-access user

Usage: enable [OPTIONS] <POLICY> <USERNAME>

Arguments:
  <POLICY>
          Policy that owns the application user

  <USERNAME>
          Application-access username

Options:
      --plan <HASH>
          Exact hash printed by the read-only user review

      --yes
          Apply the exact reviewed user plan

  -h, --help
          Print help
```

#### `mm access user password-reset`

```text
Review or replace an application-access user's hidden password

Usage: password-reset [OPTIONS] <POLICY> <USERNAME>

Arguments:
  <POLICY>
          Policy that owns the application user

  <USERNAME>
          Application-access username

Options:
      --stdin
          Read the password from standard input instead of prompting

      --plan <HASH>
          Exact hash printed by the read-only user review

      --yes
          Apply the exact reviewed user plan

  -h, --help
          Print help
```

#### `mm access user remove`

```text
Review or remove an application-access user and revoke active sessions

Usage: remove [OPTIONS] <POLICY> <USERNAME>

Arguments:
  <POLICY>
          Policy that owns the application user

  <USERNAME>
          Application-access username

Options:
      --plan <HASH>
          Exact hash printed by the read-only user review

      --yes
          Apply the exact reviewed user plan

  -h, --help
          Print help
```

#### `mm access user list`

```text
Show redacted application-user metadata for one policy

Usage: list <POLICY>

Arguments:
  <POLICY>
          Policy whose application users will be listed

Options:
  -h, --help
          Print help
```

### `mm access session`

```text
List, revoke, or rotate opaque sessions

Usage: session <COMMAND>

Commands:
  list        Show redacted active-session metadata for one policy
  revoke      Review or revoke one active session
  rotate-key  Review or rotate a policy's session encryption key
  help        Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm access session list`

```text
Show redacted active-session metadata for one policy

Usage: list <POLICY>

Arguments:
  <POLICY>
          Policy whose active sessions will be listed

Options:
  -h, --help
          Print help
```

#### `mm access session revoke`

```text
Review or revoke one active session

Usage: revoke [OPTIONS] <POLICY> <SESSION_ID>

Arguments:
  <POLICY>
          Policy that owns the session

  <SESSION_ID>
          Opaque session identifier to revoke

Options:
      --plan <HASH>
          Exact hash printed by the read-only revocation review

      --yes
          Apply the exact reviewed revocation plan

  -h, --help
          Print help
```

#### `mm access session rotate-key`

```text
Review or rotate a policy's session encryption key

Usage: rotate-key [OPTIONS] <POLICY>

Arguments:
  <POLICY>
          Policy whose session encryption key will be rotated

Options:
      --plan <HASH>
          Exact hash printed by the read-only key-rotation review

      --yes
          Apply the exact reviewed key-rotation plan

  -h, --help
          Print help
```

## `mm build`

```text
Plan and execute deterministic OCI image builds

Usage: build [OPTIONS] [COMMAND]

Commands:
  plan     Print the deterministic image build plan without running Docker
  apply    Execute an exact plan hash printed by `mm build plan`
  receipt  Write the latest published build of this checkout and environment to FILE, for `mm deploy plan --receipt` or `mm build import` on another machine
  import   Review or apply importing a build receipt from `mm build receipt`
  help     Print this message or the help of the given subcommand(s)

Options:
      --publish
          Push a multi-platform image index and reviewed tags to the registry.

          Also builds the modules that environments on remote targets run with the same build variant, so its receipt can be promoted to them.

      --all
          Build every module that has a build, not only the ones the selected environment runs and the bases they build FROM

      --yes
          Skip the build confirmation prompt

  -h, --help
          Print help (see a summary with '-h')

Examples:
  mm build plan
  mm build apply --plan HASH --yes
  mm -e production build --publish plan

Without --publish, an environment whose target is this machine gets its images loaded into the local container engine, where `mm deploy` and `mm dev up` run them, and nothing is pushed. Remote environments run published images.
```

### `mm build plan`

```text
Print the deterministic image build plan without running Docker

Usage: plan

Options:
  -h, --help
          Print help
```

### `mm build apply`

```text
Execute an exact plan hash printed by `mm build plan`

Usage: apply --plan <HASH>

Options:
      --plan <HASH>
          Hash printed by `mm build plan`

  -h, --help
          Print help
```

### `mm build receipt`

```text
Write the latest published build of this checkout and environment to FILE, for `mm deploy plan --receipt` or `mm build import` on another machine

Usage: receipt <FILE>

Arguments:
  <FILE>


Options:
  -h, --help
          Print help
```

### `mm build import`

```text
Review or apply importing a build receipt from `mm build receipt`.

Without `--plan`, prints the review and its hash. A normal import records the receipt as this checkout's published build for the environment, so deploy plans and status use its digests; it must be a build of HEAD, for this environment or one that builds the same units. `--cache-only` accepts a build of any commit and only lets later `mm build --publish` runs reuse its unchanged images.

Usage: import [OPTIONS] <FILE>

Arguments:
  <FILE>


Options:
      --cache-only
          Only seed build reuse; never select deployment images

      --plan <HASH>
          Apply the import with this hash, printed by the review

  -h, --help
          Print help (see a summary with '-h')

Examples:
  mm build import build-receipt.json
  mm build import build-receipt.json --plan HASH --yes
  mm build import baseline-receipt.json --cache-only --plan HASH --yes
```

## `mm deploy`

```text
Plan and apply the selected environment to its target

Usage: deploy [OPTIONS] [COMMAND]

Commands:
  plan      Print the deterministic deployment plan without applying it
  apply     Apply an exact plan hash printed by `mm deploy plan`
  rollback  Plan a return to what a remote environment ran before, and apply it after review
  redeploy  Plan the commit a remote environment runs now again, and apply it after review, so that secrets and variables staged on its hosts go live
  status    Show what each host of a remote environment last deployed, what it ran before, a rollout that is running or was left unfinished, and what runs on each host right now
  logs      Show the output of the jobs the last deployment ran, node by node
  cancel    Stop a deployment or other operation that runs on a remote environment, end its processes on every host, and return the hosts it changed to what they ran before
  help      Print this message or the help of the given subcommand(s)

Options:
      --yes
          Skip the deployment confirmation prompt

      --nodes <NODE,...>
          On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so

      --release <NAME>
          The release's name, which templates read as MM.RELEASE. Defaults to sha- and the commit's first 12 characters

      --release-input <NAME=VALUE>
          A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input

  -h, --help
          Print help

Examples:
  mm deploy plan
  mm deploy apply --plan HASH --yes
  mm deploy plan --nodes app-1,app-2
  mm deploy status
  mm deploy rollback
```

### `mm deploy plan`

```text
Print the deterministic deployment plan without applying it

Usage: plan [OPTIONS]

Options:
      --commit <SHA>
          Commit a remote environment deploys; the host fetches it from the project's origin. Defaults to this checkout's HEAD

      --receipt <FILE>
          Build receipt from `mm build receipt`: deploy its images, built from the deployed commit for this or another environment

  -h, --help
          Print help
```

### `mm deploy apply`

```text
Apply an exact plan hash printed by `mm deploy plan`

Usage: apply [OPTIONS] --plan <HASH>

Options:
      --plan <HASH>
          Hash printed by `mm deploy plan`

      --receipt <FILE>
          The build receipt the local plan was made with. A remote plan already carries its receipt

  -h, --help
          Print help
```

### `mm deploy rollback`

```text
Plan a return to what a remote environment ran before, and apply it after review.

Undoes a rollout that was left unfinished; otherwise undoes the last finished deployment. The hosts plan the earlier commit from their own copy of the repository, so this checkout does not need it.

Usage: rollback

Options:
  -h, --help
          Print help (see a summary with '-h')
```

### `mm deploy redeploy`

```text
Plan the commit a remote environment runs now again, and apply it after review, so that secrets and variables staged on its hosts go live.

Rolling and drained like any deployment; nothing is built, and the hosts use the build they run.

Usage: redeploy

Options:
  -h, --help
          Print help (see a summary with '-h')
```

### `mm deploy status`

```text
Show what each host of a remote environment last deployed, what it ran before, a rollout that is running or was left unfinished, and what runs on each host right now

Usage: status

Options:
  -h, --help
          Print help
```

### `mm deploy logs`

```text
Show the output of the jobs the last deployment ran, node by node

Usage: logs

Options:
  -h, --help
          Print help
```

### `mm deploy cancel`

```text
Stop a deployment or other operation that runs on a remote environment, end its processes on every host, and return the hosts it changed to what they ran before.

Anyone who may deploy to the environment may cancel. `mm deploy status` shows the operation's ID; without one, whatever runs is stopped.

Usage: cancel [OPERATION]

Arguments:
  [OPERATION]
          The operation's ID from `mm deploy status` or `mm deploy apply`, or its first characters (at least six)

Options:
  -h, --help
          Print help (see a summary with '-h')
```

## `mm team`

```text
Show who may reach remote environments, join, and sync access

Usage: team <COMMAND>

Commands:
  join    Add yourself to the users file with this device's key (reviewed edit)
  add     Add a user with a given public key (reviewed edit)
  remove  Remove a user from the users file (reviewed edit)
  show    Show the users file: each user's keys and access per environment
  whoami  Show who the remote environment thinks you are and what you may do
  sync    Reconcile the selected remote environment's access from a commit's users file without deploying
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help

Examples:
  mm key create
  mm team join --request staging=deploy
  mm --environment production team whoami
  mm --environment staging team sync
```

### `mm team join`

```text
Add yourself to the users file with this device's key (reviewed edit)

Usage: join [OPTIONS]

Options:
      --name <NAME>
          User name, [a-z][a-z0-9-]*. Defaults to $USER

      --request <ENV=LEVEL>
          Access to request, for example production=view. Repeatable

      --agent
          Add this device's agent key to your existing entry instead, marked `agent: true`. Create it first with `mm key create --agent`. After `mm key rotate --agent` it takes the place of the old agent key

      --plan <HASH>
          Exact hash printed by an earlier review of the same change

      --yes
          Apply the exact reviewed plan

  -h, --help
          Print help
```

### `mm team add`

```text
Add a user with a given public key (reviewed edit)

Usage: add [OPTIONS] --key <PUBLIC_KEY> <NAME>

Arguments:
  <NAME>


Options:
      --key <PUBLIC_KEY>
          OpenSSH public key line, `ssh-ed25519 AAAA… comment`

      --machine
          A CI or automation identity rather than a person

      --access <ENV=LEVEL>
          Access per environment, for example staging=deploy or *=view. Repeatable

      --plan <HASH>
          Exact hash printed by an earlier review of the same change

      --yes
          Apply the exact reviewed plan

  -h, --help
          Print help
```

### `mm team remove`

```text
Remove a user from the users file (reviewed edit)

Usage: remove [OPTIONS] <NAME>

Arguments:
  <NAME>


Options:
      --plan <HASH>
          Exact hash printed by an earlier review of the same change

      --yes
          Apply the exact reviewed plan

  -h, --help
          Print help
```

### `mm team show`

```text
Show the users file: each user's keys and access per environment

Usage: show

Options:
  -h, --help
          Print help
```

### `mm team whoami`

```text
Show who the remote environment thinks you are and what you may do

Usage: whoami

Options:
  -h, --help
          Print help
```

### `mm team sync`

```text
Reconcile the selected remote environment's access from a commit's users file without deploying

Usage: sync [OPTIONS]

Options:
      --commit <SHA>
          Commit whose users file to apply; defaults to HEAD

      --nodes <NODE,...>
          On a target that lists nodes, plan only these nodes. The others keep the access they enforce now

      --plan <HASH>
          Exact hash printed by an earlier review of the same change

      --yes
          Apply the exact reviewed plan

  -h, --help
          Print help
```

## `mm forward`

```text
Reach a remote service's port on localhost; save forwards for reuse

Usage: forward [OPTIONS] [SERVICE[:PORT]]...
       forward <COMMAND>

Commands:
  save    Save a forward for this project and environment, with a pinned local port
  list    List this project's saved forwards
  remove  Remove a saved forward
  up      Start every saved forward marked --auto
  help    Print this message or the help of the given subcommand(s)

Arguments:
  [SERVICE[:PORT]]...
          SERVICE, SERVICE:PORT, or the name of a saved forward. Repeatable

Options:
      --local <PORT>
          Local port; defaults to the service port, or a free port if that is busy

      --bind <ADDRESS>
          Local address to listen on. Anything but loopback exposes the service to your network

  -h, --help
          Print help

Examples:
  mm --environment staging forward postgres
  mm --environment staging forward postgres:5432 --local 15432
  mm --environment staging forward save db postgres --local 15432 --auto
  mm forward up
```

### `mm forward save`

```text
Save a forward for this project and environment, with a pinned local port

Usage: save [OPTIONS] <NAME> <SERVICE[:PORT]>

Arguments:
  <NAME>
          Name to start it by, for example `mm forward prod-db`

  <SERVICE[:PORT]>
          Service and optional container port

Options:
      --local <PORT>
          Local port to pin; defaults to the service port

      --bind <ADDRESS>
          Local address to listen on; defaults to 127.0.0.1

      --auto
          Start it with `mm forward up`

  -h, --help
          Print help
```

### `mm forward list`

```text
List this project's saved forwards

Usage: list

Options:
  -h, --help
          Print help
```

### `mm forward remove`

```text
Remove a saved forward

Usage: remove <NAME>

Arguments:
  <NAME>
          Name of the saved forward

Options:
  -h, --help
          Print help
```

### `mm forward up`

```text
Start every saved forward marked --auto

Usage: up

Options:
  -h, --help
          Print help
```

## `mm key`

```text
Create, show, protect, replace, or remove this device's SSH keys for remote environments

Usage: key <COMMAND>

Commands:
  create   Create this device's SSH key under MM_HOME, encrypted under a passphrase mm keeps in the system keychain. mm never uses ~/.ssh
  show     Print this device's public key and fingerprint for the users file, and how the key is protected
  protect  Encrypt this device's plain keys in place (reviewed). Public keys stay the same, so no host access changes
  rotate   Replace a key with a new one (reviewed), without losing access: the new key is made beside the current one and mm offers both until you finish the rotation
  remove   Delete a key and its passphrase from this device (reviewed)
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help

Examples:
  mm key create
  mm key show
  mm key protect
  mm key rotate

mm keeps each key encrypted under a passphrase it generates and stores in the system keychain, and is the only program that uses the key: never pass the private key file to ssh, scp or git.
```

### `mm key create`

```text
Create this device's SSH key under MM_HOME, encrypted under a passphrase mm keeps in the system keychain. mm never uses ~/.ssh

Usage: create [OPTIONS]

Options:
      --security-key
          Use a FIDO2 security key (sk-ssh-ed25519); touch it when asked

      --agent
          Create the key `mm mcp serve` uses instead: a separate key for your AI agent, limited on the host by the `agents:` block

  -h, --help
          Print help
```

### `mm key show`

```text
Print this device's public key and fingerprint for the users file, and how the key is protected

Usage: show [OPTIONS]

Options:
      --agent
          Print the agent key instead

  -h, --help
          Print help
```

### `mm key protect`

```text
Encrypt this device's plain keys in place (reviewed). Public keys stay the same, so no host access changes

Usage: protect [OPTIONS]

Options:
      --plan <HASH>
          Exact hash printed by an earlier review of the same change

      --yes
          Apply the exact reviewed plan

  -h, --help
          Print help

Examples:
  mm key protect
  mm key protect --plan HASH --yes
```

### `mm key rotate`

```text
Replace a key with a new one (reviewed), without losing access: the new key is made beside the current one and mm offers both until you finish the rotation

Usage: rotate [OPTIONS]

Options:
      --agent
          Rotate the agent key instead

      --finish
          Put the new key in place of the current one and delete the old key. Hosts whose users file does not list the new key refuse this device from then on

      --unlisted
          Finish even though the users file here does not list the new key

      --plan <HASH>
          Exact hash printed by an earlier review of the same change

      --yes
          Apply the exact reviewed plan

  -h, --help
          Print help

Examples:
  mm key rotate
  mm key rotate --plan HASH --yes
  mm team join
  mm team sync
  mm key rotate --finish
  mm key rotate --finish --plan HASH --yes

While a rotation is in progress, `mm team join` puts the new key in your entry in place of the current one, and `mm team sync` installs it on the hosts; it authenticates with the current key, so this works for a sole admin. Then `--finish` drops the old key. A security key is replaced at once.
```

### `mm key remove`

```text
Delete a key and its passphrase from this device (reviewed)

Usage: remove [OPTIONS]

Options:
      --agent
          Delete the agent key instead

      --plan <HASH>
          Exact hash printed by an earlier review of the same change

      --yes
          Apply the exact reviewed plan

  -h, --help
          Print help

Examples:
  mm key remove --agent
  mm key remove --agent --plan HASH --yes
```

## `mm target`

```text
Manage targets, private selection, setup, ingress, and remote access

Usage: target <COMMAND>

Commands:
  local        Register this machine as the local target, preserving existing targets
  list         List targets configured beneath MM_HOME
  show         Show one target from the shared catalog
  nodes        List every node of the selected environment's target: whether it answers (and why not), its mmd version, its mesh address, and the commit and time of its last applied deployment
  mesh         The WireGuard mesh between the nodes of the selected environment's target: look at it, or replace one node's key
  storage      The shared volumes of the selected environment's target: the node that keeps them, their sizes, and which nodes mount them; or delete one volume's files
  connections  List the SSH connections this device keeps open to remote hosts between commands, or close them
  trust        Review a repo-declared target's host key and daemon identity, and accept a change with --yes
  add          Add an SSH target to the shared catalog
  update       Update an SSH target without changing its stable identity
  remove       Remove a target only if its reviewed fingerprint still matches
  select       Select a private target preference for this project
  preference   Show or change private runtime and target preferences
  setup        Plan or apply target provisioning and project enrollment
  ingress      Configure and plan the target-owned ingress stack
  access       Administer remote developer access on a target
  help         Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

### `mm target local`

```text
Register this machine as the local target, preserving existing targets

Usage: local

Options:
  -h, --help
          Print help
```

### `mm target list`

```text
List targets configured beneath MM_HOME

Usage: list

Options:
  -h, --help
          Print help
```

### `mm target show`

```text
Show one target from the shared catalog

Usage: show <NAME>

Arguments:
  <NAME>


Options:
  -h, --help
          Print help
```

### `mm target nodes`

```text
List every node of the selected environment's target: whether it answers (and why not), its mmd version, its mesh address, and the commit and time of its last applied deployment

Usage: nodes

Options:
  -h, --help
          Print help
```

### `mm target mesh`

```text
The WireGuard mesh between the nodes of the selected environment's target: look at it, or replace one node's key

Usage: mesh <COMMAND>

Commands:
  status  Show every node's mesh interface and, per peer, its endpoint, last handshake, and traffic. Lists what is wrong with what to do about it, such as the UDP port to open between two hosts. Exits 3 when a node does not answer or the mesh has a problem
  rotate  Replace one node's WireGuard key pair. Without --plan this prints the plan: the node makes a new key and every other node replaces its one entry for it. Nothing else changes. The private key never leaves the node
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm target mesh status`

```text
Show every node's mesh interface and, per peer, its endpoint, last handshake, and traffic. Lists what is wrong with what to do about it, such as the UDP port to open between two hosts. Exits 3 when a node does not answer or the mesh has a problem

Usage: status

Options:
  -h, --help
          Print help
```

#### `mm target mesh rotate`

```text
Replace one node's WireGuard key pair. Without --plan this prints the plan: the node makes a new key and every other node replaces its one entry for it. Nothing else changes. The private key never leaves the node

Usage: rotate [OPTIONS] <NODE>

Arguments:
  <NODE>
          The node whose key is replaced

Options:
      --plan <HASH>
          Apply the plan with this hash, as printed without --plan

      --yes
          Skip the confirmation prompt

  -h, --help
          Print help
```

### `mm target storage`

```text
The shared volumes of the selected environment's target: the node that keeps them, their sizes, and which nodes mount them; or delete one volume's files

Usage: storage <COMMAND>

Commands:
  status  Show the node that keeps shared volumes, each volume with its size and the nodes that have it mounted, and whether every node reaches the storage node. Also lists volumes that are no longer in the configuration and still have files. Exits 3 when a node does not answer or cannot reach the storage node
  delete  Delete a shared volume's files from the storage node. Without --plan this prints what would be removed. Refused while a container on any node uses the volume. This is the only command that removes shared data: taking a volume out of `shared-volumes` keeps it
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm target storage status`

```text
Show the node that keeps shared volumes, each volume with its size and the nodes that have it mounted, and whether every node reaches the storage node. Also lists volumes that are no longer in the configuration and still have files. Exits 3 when a node does not answer or cannot reach the storage node

Usage: status

Options:
  -h, --help
          Print help
```

#### `mm target storage delete`

```text
Delete a shared volume's files from the storage node. Without --plan this prints what would be removed. Refused while a container on any node uses the volume. This is the only command that removes shared data: taking a volume out of `shared-volumes` keeps it

Usage: delete [OPTIONS] <VOLUME>

Arguments:
  <VOLUME>
          The volume's name, as listed under `shared-volumes`

Options:
      --stack <STACK>
          The stack the volume belongs to, when more than one stack keeps a volume of this name

      --plan <HASH>
          Delete what the plan with this hash showed, as printed without --plan

      --yes
          Skip the confirmation prompt

  -h, --help
          Print help
```

### `mm target connections`

```text
List the SSH connections this device keeps open to remote hosts between commands, or close them

Usage: connections [OPTIONS]

Options:
      --close
          Close every kept connection

  -h, --help
          Print help

Examples:
  mm target connections
  mm target connections --close

The first command to a host leaves its connection open for five idle minutes, so the next ones start faster. While it is open, a program running as you can reach that host's mm relay through it. MM_SSH_REUSE=off keeps none.
```

### `mm target trust`

```text
Review a repo-declared target's host key and daemon identity, and accept a change with --yes

Usage: trust [OPTIONS] <NAME>

Arguments:
  <NAME>


Options:
      --yes
          Accept the repo's current keys after reviewing them

  -h, --help
          Print help
```

### `mm target add`

```text
Add an SSH target to the shared catalog

Usage: add [OPTIONS] --host <HOST> --user <USER> --host-key-fingerprint <HOST_KEY_FINGERPRINT> <NAME>

Arguments:
  <NAME>


Options:
      --host <HOST>
          SSH hostname or address

      --user <USER>
          SSH user

      --port <PORT>
          SSH port

          [default: 22]

      --host-key-fingerprint <HOST_KEY_FINGERPRINT>
          OpenSSH SHA256 fingerprint accepted for this host

      --host-key <TYPE BASE64>
          Full reviewed OpenSSH host public key used for strict known-hosts verification

      --identity-file <PATH>
          Absolute private-key path used for public-key client authentication

      --remote-root <PATH>
          Absolute project checkout path on the remote host

      --daemon-fingerprint <BLAKE3>
          BLAKE3 daemon identity printed by `mmd --print-identity`

  -h, --help
          Print help
```

### `mm target update`

```text
Update an SSH target without changing its stable identity

Usage: update [OPTIONS] --host <HOST> --user <USER> --host-key-fingerprint <HOST_KEY_FINGERPRINT> --expected-fingerprint <EXPECTED_FINGERPRINT> <NAME>

Arguments:
  <NAME>


Options:
      --host <HOST>
          SSH hostname or address

      --user <USER>
          SSH user

      --port <PORT>
          SSH port

          [default: 22]

      --host-key-fingerprint <HOST_KEY_FINGERPRINT>
          OpenSSH SHA256 fingerprint accepted for this host

      --host-key <TYPE BASE64>
          Full reviewed OpenSSH host public key used for strict known-hosts verification

      --identity-file <PATH>
          Absolute private-key path used for public-key client authentication

      --remote-root <PATH>
          Absolute project checkout path on the remote host

      --daemon-fingerprint <BLAKE3>
          BLAKE3 daemon identity printed by `mmd --print-identity`

      --expected-fingerprint <EXPECTED_FINGERPRINT>
          Fingerprint from the reviewed target list

  -h, --help
          Print help
```

### `mm target remove`

```text
Remove a target only if its reviewed fingerprint still matches

Usage: remove [OPTIONS] --expected-fingerprint <EXPECTED_FINGERPRINT> <NAME>

Arguments:
  <NAME>


Options:
      --expected-fingerprint <EXPECTED_FINGERPRINT>
          Fingerprint from `mm target show`

      --yes
          Confirm catalog mutation

  -h, --help
          Print help
```

### `mm target select`

```text
Select a private target preference for this project

Usage: select [OPTIONS] <TARGET>

Arguments:
  <TARGET>


Options:
      --environment <NAME>
          Limit the preference to one environment

  -h, --help
          Print help
```

### `mm target preference`

```text
Show or change private runtime and target preferences

Usage: preference <COMMAND>

Commands:
  show           Show resolved private runtime and target preferences
  set-runtime    Persist a private runtime preference
  unset-runtime  Remove a private runtime preference
  set-target     Persist a private target preference
  unset-target   Remove a private target preference
  set-address    Connect this device to a cluster node at an address of its own, such as a VPN address, instead of the node's host in targets.yaml. The node's pinned host key and daemon identity must still match
  unset-address  Connect to a cluster node at its host in targets.yaml again
  help           Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm target preference show`

```text
Show resolved private runtime and target preferences

Usage: show

Options:
  -h, --help
          Print help
```

#### `mm target preference set-runtime`

```text
Persist a private runtime preference

Usage: set-runtime [OPTIONS] <RUNTIME>

Arguments:
  <RUNTIME>
          [possible values: podman-compose, docker-compose]

Options:
      --environment <NAME>
          Limit the preference to one environment

  -h, --help
          Print help
```

#### `mm target preference unset-runtime`

```text
Remove a private runtime preference

Usage: unset-runtime [OPTIONS]

Options:
      --environment <NAME>
          Limit the unset to one environment

  -h, --help
          Print help
```

#### `mm target preference set-target`

```text
Persist a private target preference

Usage: set-target [OPTIONS] <TARGET>

Arguments:
  <TARGET>


Options:
      --environment <NAME>
          Limit the preference to one environment

  -h, --help
          Print help
```

#### `mm target preference unset-target`

```text
Remove a private target preference

Usage: unset-target [OPTIONS]

Options:
      --environment <NAME>
          Limit the unset to one environment

  -h, --help
          Print help
```

#### `mm target preference set-address`

```text
Connect this device to a cluster node at an address of its own, such as a VPN address, instead of the node's host in targets.yaml. The node's pinned host key and daemon identity must still match

Usage: set-address <ENVIRONMENT> <NODE> <HOST[:PORT]>

Arguments:
  <ENVIRONMENT>
          The environment whose target lists the node

  <NODE>
          The node's name under `nodes:`

  <HOST[:PORT]>
          A hostname, IPv4 or IPv6 address, with an optional port

Options:
  -h, --help
          Print help

Examples:
  mm target preference set-address staging app-1 10.0.0.3
  mm target preference set-address staging app-2 node-2.internal:2222
  mm target preference set-address staging app-3 '[fd00::5]:22'

The address is kept in this device's mm home and nowhere else. Without a port, mm uses the node's port from targets.yaml.
```

#### `mm target preference unset-address`

```text
Connect to a cluster node at its host in targets.yaml again

Usage: unset-address <ENVIRONMENT> <NODE>

Arguments:
  <ENVIRONMENT>
          The environment whose target lists the node

  <NODE>
          The node's name under `nodes:`

Options:
  -h, --help
          Print help
```

### `mm target setup`

```text
Plan or apply target provisioning and project enrollment

Usage: setup <COMMAND>

Commands:
  plan        Produce a deterministic target setup plan. For a cluster target (`nodes:`), the plan also scans each node's SSH host key
  apply       Execute an exact plan hash printed by `mm target setup plan`
  deploy-key  Give the environment its cluster target's read-only git deploy key, stored in the secret `setup.deploy-key.secret` names, and print its public half. A target has one key: it is copied from another environment of the target that holds it, and created only when none does. Prints the existing key when there is one
  record      Write each node's SSH host key, daemon identity, and private address from the last cluster setup into `.config/mm/targets.yaml`
  help        Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm target setup plan`

```text
Produce a deterministic target setup plan. For a cluster target (`nodes:`), the plan also scans each node's SSH host key

Usage: plan [OPTIONS]

Options:
  -h, --help
          Print help

Remote access (pass the same flags to plan and apply):
      --origin <URL>
          Git URL the host fetches the project from with its own read-only deploy key. Turns on the remote access layer

      --delivery <MODE>
          `push`: the host fetches nothing and mm sends it each commit's files, so it needs no origin and no deploy key. Turns on the remote access layer, like --origin, and takes its place

          [possible values: push]

      --origin-host-key <KEY>
          The git server's SSH host key, `<type> <base64>`, for SSH origins

      --break-glass <NAME=KEY>
          A recovery key for the host policy, NAME='ssh-ed25519 AAAA… comment'. Repeat for more. Keep the private half offline

      --host-binaries <DIR>
          Directory with `mm` and `mmd` built for the host, such as an extracted Linux release. Default: for each host's CPU, the directory of this `mm` when it holds them, otherwise this mm's own release, downloaded, checked, and kept under the mm home

Cluster targets (nodes:):
      --trust-new-host-keys
          Accept SSH host keys that differ from the ones the repo records, such as after rebuilding a node. The plan shows each key's fingerprint

      --commit <SHA>
          Commit every node syncs the users file from (default: HEAD)

      --nodes <NODE,...>
          Set up only these nodes, for example while another one cannot be reached. The others are not contacted and keep the version they run

Releases (pass the same flags to plan and apply):
      --downgrade
          Install this release on hosts that run a newer one

      --upgrade
          Install this mm and mmd on hosts that run another build of it or an older release. Without it, setup refuses to replace what the hosts run, unless deploy.mm-version pins this release
```

#### `mm target setup apply`

```text
Execute an exact plan hash printed by `mm target setup plan`

Usage: apply [OPTIONS] --plan <HASH>

Options:
      --plan <HASH>
          Hash printed by `mm target setup plan`

      --yes
          Skip the bootstrap confirmation prompt

  -h, --help
          Print help

Remote access (pass the same flags to plan and apply):
      --origin <URL>
          Git URL the host fetches the project from with its own read-only deploy key. Turns on the remote access layer

      --delivery <MODE>
          `push`: the host fetches nothing and mm sends it each commit's files, so it needs no origin and no deploy key. Turns on the remote access layer, like --origin, and takes its place

          [possible values: push]

      --origin-host-key <KEY>
          The git server's SSH host key, `<type> <base64>`, for SSH origins

      --break-glass <NAME=KEY>
          A recovery key for the host policy, NAME='ssh-ed25519 AAAA… comment'. Repeat for more. Keep the private half offline

      --host-binaries <DIR>
          Directory with `mm` and `mmd` built for the host, such as an extracted Linux release. Default: for each host's CPU, the directory of this `mm` when it holds them, otherwise this mm's own release, downloaded, checked, and kept under the mm home

Cluster targets (nodes:):
      --trust-new-host-keys
          Accept SSH host keys that differ from the ones the repo records, such as after rebuilding a node. The plan shows each key's fingerprint

      --commit <SHA>
          Commit every node syncs the users file from (default: HEAD)

      --nodes <NODE,...>
          Set up only these nodes, for example while another one cannot be reached. The others are not contacted and keep the version they run

Releases (pass the same flags to plan and apply):
      --downgrade
          Install this release on hosts that run a newer one

      --upgrade
          Install this mm and mmd on hosts that run another build of it or an older release. Without it, setup refuses to replace what the hosts run, unless deploy.mm-version pins this release
```

#### `mm target setup deploy-key`

```text
Give the environment its cluster target's read-only git deploy key, stored in the secret `setup.deploy-key.secret` names, and print its public half. A target has one key: it is copied from another environment of the target that holds it, and created only when none does. Prints the existing key when there is one

Usage: deploy-key

Options:
  -h, --help
          Print help
```

#### `mm target setup record`

```text
Write each node's SSH host key, daemon identity, and private address from the last cluster setup into `.config/mm/targets.yaml`

Usage: record

Options:
  -h, --help
          Print help
```

### `mm target ingress`

```text
Configure and plan the target-owned ingress stack

Usage: ingress <COMMAND>

Commands:
  configure  Configure digest-pinned Traefik and optional ACME metadata
  plan       Publish a deterministic target-owned ingress artifact and plan
  help       Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm target ingress configure`

```text
Configure digest-pinned Traefik and optional ACME metadata

Usage: configure [OPTIONS] --image <IMAGE> --expected-fingerprint <EXPECTED_FINGERPRINT> <TARGET>

Arguments:
  <TARGET>


Options:
      --image <IMAGE>
          Digest-pinned Traefik image reference

      --acme-email <ACME_EMAIL>
          Optional ACME account email

      --acme-ca-server <URL>
          ACME directory URL, such as Let's Encrypt staging or an internal ACME server

      --acme-ca-certificate <PATH>
          PEM file of the CA that signs the ACME server's HTTPS certificate

      --expected-fingerprint <EXPECTED_FINGERPRINT>
          Fingerprint from the reviewed target list

  -h, --help
          Print help
```

#### `mm target ingress plan`

```text
Publish a deterministic target-owned ingress artifact and plan

Usage: plan <TARGET>

Arguments:
  <TARGET>


Options:
  -h, --help
          Print help
```

### `mm target access`

```text
Administer remote developer access on a target

Usage: access <COMMAND>

Commands:
  status  Explain the privileged remote-access administration boundary
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

#### `mm target access status`

```text
Explain the privileged remote-access administration boundary

Usage: status

Options:
  -h, --help
          Print help
```

## `mm agent`

```text
Show and trust what AI agents may do, and connect coding tools

Usage: agent <COMMAND>

Commands:
  status  Show what agents may do in each environment and whether this device trusts the project's `agents:` block
  trust   Review the project's `agents:` block and trust it on this device. Until then, and after any edit to the block, agents can only view
  setup   Connect coding tools to `mm mcp serve`
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help

Examples:
  mm agent status
  mm agent trust
  mm agent setup --client claude-code --yes

Agents work through `mm mcp serve`, limited per environment by the project's `agents:` block. mm refuses to run directly for an agent.
```

### `mm agent status`

```text
Show what agents may do in each environment and whether this device trusts the project's `agents:` block

Usage: status

Options:
  -h, --help
          Print help
```

### `mm agent trust`

```text
Review the project's `agents:` block and trust it on this device. Until then, and after any edit to the block, agents can only view

Usage: trust [OPTIONS]

Options:
      --plan <HASH>
          Exact hash printed by the review of the block

      --yes
          Trust the reviewed block

  -h, --help
          Print help

Examples:
  mm agent trust
  mm agent trust --plan HASH --yes
```

### `mm agent setup`

```text
Connect coding tools to `mm mcp serve`

Usage: setup [OPTIONS]

Options:
      --client <TOOL>
          Tool to connect. Repeatable. Defaults to the tools found on PATH

          [possible values: claude-code, opencode, codex, cursor]

      --user
          Register the server for this user instead of this project

      --yes
          Make the changes shown

  -h, --help
          Print help

Examples:
  mm agent setup
  mm agent setup --client claude-code --yes
  mm agent setup --user --yes

Without --yes the changes are only shown. In a project, the files are meant to be committed: the MCP server entry, instructions in AGENTS.md, and for Claude Code a rule that denies running mm directly. With --user, mm registers the server in each tool's own configuration for every project.
```

## `mm history`

```text
List the commands run in this project on this machine, and show what one printed

Usage: history [OPTIONS] [COMMAND]

Commands:
  show  Show one command: what it printed, its steps and its result
  help  Print this message or the help of the given subcommand(s)

Options:
      --outcome <OUTCOME>
          Only commands that ended this way: succeeded, failed or interrupted

      --since <AGE>
          Only commands that started within this long, such as 30m, 2h or 3d

      --limit <N>
          How many commands to list

          [default: 20]

      --before <ID>
          List the commands that started before this one

  -h, --help
          Print help

Examples:
  mm history
  mm history --outcome failed --since 2d
  mm -e production history
  mm history show 20261115T201530123Z-4f2a

Records stay on this machine, under the mm home. Secret values are never recorded.
```

### `mm history show`

```text
Show one command: what it printed, its steps and its result

Usage: show <ID>

Arguments:
  <ID>
          The command's ID from `mm history`, or its first characters

Options:
  -h, --help
          Print help
```

## `mm workspace`

```text
List the project folders mm knows on this machine, and forget the ones that are gone

Usage: workspace <COMMAND>

Commands:
  list    List the project folders mm has run in on this machine, when each was last used, and which are gone
  forget  Forget registered project folders through a reviewed plan
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help

Examples:
  mm workspace list
  mm workspace forget --stale
  mm workspace forget --stale --plan HASH --yes

mm registers a project folder the first time it runs there. Forgetting one drops its operation history and `mm env set` values on this machine; no file is deleted.
```

### `mm workspace list`

```text
List the project folders mm has run in on this machine, when each was last used, and which are gone

Usage: list

Options:
  -h, --help
          Print help
```

### `mm workspace forget`

```text
Forget registered project folders through a reviewed plan.

mm drops their operation history and `mm env set` values on this machine. No file is deleted.

Usage: forget [OPTIONS] [ROOT|ID]

Arguments:
  [ROOT|ID]
          A registered folder, or a project or workspace ID from `mm workspace list --output json`

Options:
      --stale
          Forget every registered workspace whose folder or mm.yaml is gone

      --plan <HASH>
          Exact hash printed by an earlier review of the same change

      --yes
          Apply the exact reviewed plan

  -h, --help
          Print help (see a summary with '-h')

Examples:
  mm workspace forget --stale
  mm workspace forget --stale --plan HASH --yes
  mm workspace forget ~/src/old-checkout

A folder that still exists is registered again the next time mm runs there.
```

## `mm update`

```text
Update this machine's mm, and the mmd and mm-ingress-helper beside it, from a verified release, and restart the local services that run it

Usage: update [OPTIONS]

Options:
      --check
          Only say whether a newer stable release exists; change nothing

      --version <vX.Y.Z>
          Install this release instead of the latest stable one, such as v0.1.2, or a release candidate such as v0.1.3-rc.1

      --force
          Install an older release named with --version, install the same release again, or replace an mm built from a checkout

      --yes
          Replace and restart without asking. Needed when standard input is not a terminal

  -h, --help
          Print help

Examples:
  mm update --check
  mm update
  mm update --yes
  mm update --version v0.1.2 --force

mm update downloads the release's archive for this platform, checks its SHA-256 and size against the release's manifest.json (SHA256SUMS for a release candidate), runs the new mm, and then replaces every binary at once or none. It replaces mm, and mmd and mm-ingress-helper when they sit beside it, then restarts the local user services that run mm (the dashboard control API) and checks they come back. It ends with what runs which release: this mm, those services, `mm mcp serve` processes left on an older mm, and, run in a project, each remote environment's nodes, with the commands that upgrade older hosts. Hosts keep the release they run until you run those. Exit codes: 0 updated or nothing to do; 1 the release could not be read or installed, this installation is refused, or a restarted service did not come back; 3 the archive failed its checks.
```

## `mm mcp`

```text
Serve the Model Context Protocol server that AI agents use

Usage: mcp <COMMAND>

Commands:
  serve  Serve MCP on standard input/output for the project in the current directory (or --root). No network listener is opened
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help
          Print help
```

### `mm mcp serve`

```text
Serve MCP on standard input/output for the project in the current directory (or --root). No network listener is opened

Usage: serve [OPTIONS]

Options:
      --config <PATH>
          Private limits for this device. Defaults to $MM_HOME/mcp.json, which is optional

  -h, --help
          Print help
```
