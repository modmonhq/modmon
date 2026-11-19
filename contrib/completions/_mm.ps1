# Generated from the canonical Clap tree by ./scripts/generate-artifacts.sh. Do not edit.

using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'mm' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'mm'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'mm' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Initialize a project through a reviewed plan or interactive onboarding')
            [CompletionResult]::new('dev', 'dev', [CompletionResultType]::ParameterValue, 'Set up and operate the local development environment')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Validate and edit desired project configuration through typed reviews')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'Check configuration health and resolved runtime capabilities')
            [CompletionResult]::new('ingress', 'ingress', [CompletionResultType]::ParameterValue, 'Start or stop this workstation''s global public HTTP/HTTPS forwarding')
            [CompletionResult]::new('service', 'service', [CompletionResultType]::ParameterValue, 'Inspect and operate deployed services')
            [CompletionResult]::new('job', 'job', [CompletionResultType]::ParameterValue, 'Run finite jobs and operate their schedules')
            [CompletionResult]::new('env', 'env', [CompletionResultType]::ParameterValue, 'Inspect and deliver non-secret environment values')
            [CompletionResult]::new('secret', 'secret', [CompletionResultType]::ParameterValue, 'Manage secret values, rotation, and provider instances')
            [CompletionResult]::new('access', 'access', [CompletionResultType]::ParameterValue, 'Manage application access policies, users, routes, and sessions')
            [CompletionResult]::new('build', 'build', [CompletionResultType]::ParameterValue, 'Plan and execute deterministic OCI image builds')
            [CompletionResult]::new('deploy', 'deploy', [CompletionResultType]::ParameterValue, 'Plan and apply the selected environment to its target')
            [CompletionResult]::new('team', 'team', [CompletionResultType]::ParameterValue, 'Show who may reach remote environments, join, and sync access')
            [CompletionResult]::new('forward', 'forward', [CompletionResultType]::ParameterValue, 'Reach a remote service''s port on localhost; save forwards for reuse')
            [CompletionResult]::new('key', 'key', [CompletionResultType]::ParameterValue, 'Create, show, protect, replace, or remove this device''s SSH keys for remote environments')
            [CompletionResult]::new('target', 'target', [CompletionResultType]::ParameterValue, 'Manage targets, private selection, setup, ingress, and remote access')
            [CompletionResult]::new('agent', 'agent', [CompletionResultType]::ParameterValue, 'Show and trust what AI agents may do, and connect coding tools')
            [CompletionResult]::new('history', 'history', [CompletionResultType]::ParameterValue, 'List the commands run in this project on this machine, and show what one printed')
            [CompletionResult]::new('workspace', 'workspace', [CompletionResultType]::ParameterValue, 'List the project folders mm knows on this machine, and forget the ones that are gone')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'Update this machine''s mm, and the mmd and mm-ingress-helper beside it, from a verified release')
            [CompletionResult]::new('mcp', 'mcp', [CompletionResultType]::ParameterValue, 'Serve the Model Context Protocol server that AI agents use')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;init' {
            [CompletionResult]::new('--name', '--name', [CompletionResultType]::ParameterName, 'Override the project name inferred from the directory')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by the read-only initialization review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the reviewed configuration, Ansible starter, and local-override ignore rule')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('setup', 'setup', [CompletionResultType]::ParameterValue, 'Set up local trust, ingress, Mailpit, Cap, dashboard, and project routes')
            [CompletionResult]::new('up', 'up', [CompletionResultType]::ParameterValue, 'Review or apply the local development deployment')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Turn optional modules, services or jobs on for this machine; `mm dev up` starts them')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Turn optional units off for this machine, stop them and remove their routes; their volumes stay')
            [CompletionResult]::new('down', 'down', [CompletionResultType]::ParameterValue, 'Stop the project''s containers; volumes, routes, and overrides are kept')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show local project and runtime status')
            [CompletionResult]::new('tls', 'tls', [CompletionResultType]::ParameterValue, 'Inspect and maintain local HTTPS certificates and trust')
            [CompletionResult]::new('services', 'services', [CompletionResultType]::ParameterValue, 'Manage the installation-wide local service stack')
            [CompletionResult]::new('dashboard', 'dashboard', [CompletionResultType]::ParameterValue, 'Serve the local developer dashboard')
            [CompletionResult]::new('mounts', 'mounts', [CompletionResultType]::ParameterValue, 'Discover, review, and disable source-code mounts')
            [CompletionResult]::new('open', 'open', [CompletionResultType]::ParameterValue, 'Print a configured local service URL')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;dev;setup' {
            [CompletionResult]::new('--smtp-port', '--smtp-port', [CompletionResultType]::ParameterName, 'Publish SMTP on this explicit loopback port')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--no-trust', '--no-trust', [CompletionResultType]::ParameterName, 'Generate certificates without trusting the root')
            [CompletionResult]::new('--system', '--system', [CompletionResultType]::ParameterName, 'Use the privileged system trust store on macOS or Windows')
            [CompletionResult]::new('--services', '--services', [CompletionResultType]::ParameterName, 'Review and install the shared local service stack')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the displayed service plan without prompting')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('project', 'project', [CompletionResultType]::ParameterValue, 'Inspect and deploy an existing clone using its configured development environment')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;dev;setup;project' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash printed by the read-only guided workflow review')
            [CompletionResult]::new('--deploy-plan', '--deploy-plan', [CompletionResultType]::ParameterName, 'Hash printed by `mm deploy plan` for the same environment and target')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply reviewed local dependencies and deployment without prompting')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;setup;help' {
            [CompletionResult]::new('project', 'project', [CompletionResultType]::ParameterValue, 'Inspect and deploy an existing clone using its configured development environment')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;dev;setup;help;project' {
            break
        }
        'mm;dev;setup;help;help' {
            break
        }
        'mm;dev;up' {
            [CompletionResult]::new('--with', '--with', [CompletionResultType]::ParameterName, 'Turn an optional module, service or job on for this run only; repeat for more')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash printed by the deployment review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan without prompting')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;enable' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;disable' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the deployment that stops them without prompting')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;down' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Confirm stopping every stack of the selected environment')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;status' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;tls' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--system', '--system', [CompletionResultType]::ParameterName, 'Use the privileged system trust store on macOS or Windows')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Query CA files, leaf renewal, and the actual platform trust store')
            [CompletionResult]::new('repair', 'repair', [CompletionResultType]::ParameterValue, 'Restore trust, renew stale leaves, and finish an interrupted rotation')
            [CompletionResult]::new('rotate', 'rotate', [CompletionResultType]::ParameterValue, 'Rotate the root with staged trust overlap and reissued leaves')
            [CompletionResult]::new('uninstall', 'uninstall', [CompletionResultType]::ParameterValue, 'Remove only exact mm-owned trust entries and local private material')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;dev;tls;status' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--system', '--system', [CompletionResultType]::ParameterName, 'Use the privileged system trust store on macOS or Windows')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;tls;repair' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--system', '--system', [CompletionResultType]::ParameterName, 'Use the privileged system trust store on macOS or Windows')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;tls;rotate' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--system', '--system', [CompletionResultType]::ParameterName, 'Use the privileged system trust store on macOS or Windows')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;tls;uninstall' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--system', '--system', [CompletionResultType]::ParameterName, 'Use the privileged system trust store on macOS or Windows')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;tls;help' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Query CA files, leaf renewal, and the actual platform trust store')
            [CompletionResult]::new('repair', 'repair', [CompletionResultType]::ParameterValue, 'Restore trust, renew stale leaves, and finish an interrupted rotation')
            [CompletionResult]::new('rotate', 'rotate', [CompletionResultType]::ParameterValue, 'Rotate the root with staged trust overlap and reissued leaves')
            [CompletionResult]::new('uninstall', 'uninstall', [CompletionResultType]::ParameterValue, 'Remove only exact mm-owned trust entries and local private material')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;dev;tls;help;status' {
            break
        }
        'mm;dev;tls;help;repair' {
            break
        }
        'mm;dev;tls;help;rotate' {
            break
        }
        'mm;dev;tls;help;uninstall' {
            break
        }
        'mm;dev;tls;help;help' {
            break
        }
        'mm;dev;services' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Publish a deterministic reviewed lifecycle plan')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Apply an exact install/convergence plan hash')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show component health, ownership, image, route, certificate, and drift status')
            [CompletionResult]::new('upgrade', 'upgrade', [CompletionResultType]::ParameterValue, 'Apply an exact version upgrade plan hash')
            [CompletionResult]::new('repair', 'repair', [CompletionResultType]::ParameterValue, 'Recover an interrupted operation or reconcile drift from an exact plan')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove only mm-owned runtime resources using an exact reviewed plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;dev;services;plan' {
            [CompletionResult]::new('--operation', '--operation', [CompletionResultType]::ParameterName, 'Lifecycle operation represented by the plan')
            [CompletionResult]::new('--smtp-port', '--smtp-port', [CompletionResultType]::ParameterName, 'Publish SMTP on this loopback port for host-native applications')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--purge', '--purge', [CompletionResultType]::ParameterName, 'Include persistent data deletion in a remove plan')
            [CompletionResult]::new('--gate', '--gate', [CompletionResultType]::ParameterName, 'Prepare conditional Cap and Valkey components before creating the first gate policy')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;services;apply' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash printed by `mm dev services plan` for an apply operation')
            [CompletionResult]::new('--smtp-port', '--smtp-port', [CompletionResultType]::ParameterName, 'Publish SMTP on this loopback port; must match the reviewed plan')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--gate', '--gate', [CompletionResultType]::ParameterName, 'Include the reviewed first-policy Cap and Valkey preparation')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the service mutation confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;services;status' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;services;upgrade' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash printed by `mm dev services plan --operation upgrade`')
            [CompletionResult]::new('--smtp-port', '--smtp-port', [CompletionResultType]::ParameterName, 'Change the loopback SMTP port as part of the reviewed upgrade')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--gate', '--gate', [CompletionResultType]::ParameterName, 'Include the reviewed Cap and Valkey components')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the service mutation confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;services;repair' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash printed by `mm dev services plan --operation repair`')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the service mutation confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;services;remove' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash printed by `mm dev services plan --operation remove`')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--purge', '--purge', [CompletionResultType]::ParameterName, 'Delete retained Mailpit and ingress volumes; must match the reviewed plan')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the service mutation confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;services;help' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Publish a deterministic reviewed lifecycle plan')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Apply an exact install/convergence plan hash')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show component health, ownership, image, route, certificate, and drift status')
            [CompletionResult]::new('upgrade', 'upgrade', [CompletionResultType]::ParameterValue, 'Apply an exact version upgrade plan hash')
            [CompletionResult]::new('repair', 'repair', [CompletionResultType]::ParameterValue, 'Recover an interrupted operation or reconcile drift from an exact plan')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove only mm-owned runtime resources using an exact reviewed plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;dev;services;help;plan' {
            break
        }
        'mm;dev;services;help;apply' {
            break
        }
        'mm;dev;services;help;status' {
            break
        }
        'mm;dev;services;help;upgrade' {
            break
        }
        'mm;dev;services;help;repair' {
            break
        }
        'mm;dev;services;help;remove' {
            break
        }
        'mm;dev;services;help;help' {
            break
        }
        'mm;dev;dashboard' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('serve', 'serve', [CompletionResultType]::ParameterValue, 'Run the ingress-authenticated dashboard backend')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;dev;dashboard;serve' {
            [CompletionResult]::new('--listen', '--listen', [CompletionResultType]::ParameterName, 'Control listener used by the managed ingress route')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;dashboard;help' {
            [CompletionResult]::new('serve', 'serve', [CompletionResultType]::ParameterValue, 'Run the ingress-authenticated dashboard backend')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;dev;dashboard;help;serve' {
            break
        }
        'mm;dev;dashboard;help;help' {
            break
        }
        'mm;dev;mounts' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Produce a deterministic mount proposal with discovery evidence')
            [CompletionResult]::new('configure', 'configure', [CompletionResultType]::ParameterValue, 'Persist the exact reviewed proposal in the project config')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Remove a configured mount after reviewing its current plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;dev;mounts;plan' {
            [CompletionResult]::new('--source', '--source', [CompletionResultType]::ParameterName, 'Override the module-relative portable source proposed by discovery')
            [CompletionResult]::new('--mount-target', '--mount-target', [CompletionResultType]::ParameterName, 'Override the absolute container target proposed by discovery')
            [CompletionResult]::new('--service', '--service', [CompletionResultType]::ParameterName, 'Select a service explicitly; repeat for multiple services')
            [CompletionResult]::new('--preset', '--preset', [CompletionResultType]::ParameterName, 'Use a framework preservation preset explicitly')
            [CompletionResult]::new('--safe-exception', '--safe-exception', [CompletionResultType]::ParameterName, 'Explicitly approve one otherwise-sensitive module-relative path')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--read-only', '--read-only', [CompletionResultType]::ParameterName, 'Mount source read-only instead of read-write')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;mounts;configure' {
            [CompletionResult]::new('--source', '--source', [CompletionResultType]::ParameterName, 'Override the module-relative portable source proposed by discovery')
            [CompletionResult]::new('--mount-target', '--mount-target', [CompletionResultType]::ParameterName, 'Override the absolute container target proposed by discovery')
            [CompletionResult]::new('--service', '--service', [CompletionResultType]::ParameterName, 'Select a service explicitly; repeat for multiple services')
            [CompletionResult]::new('--preset', '--preset', [CompletionResultType]::ParameterName, 'Use a framework preservation preset explicitly')
            [CompletionResult]::new('--safe-exception', '--safe-exception', [CompletionResultType]::ParameterName, 'Explicitly approve one otherwise-sensitive module-relative path')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash printed by `mm dev mounts plan` with identical options')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--read-only', '--read-only', [CompletionResultType]::ParameterName, 'Mount source read-only instead of read-write')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;mounts;disable' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash printed by `mm dev mounts plan MODULE`')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;mounts;help' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Produce a deterministic mount proposal with discovery evidence')
            [CompletionResult]::new('configure', 'configure', [CompletionResultType]::ParameterValue, 'Persist the exact reviewed proposal in the project config')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Remove a configured mount after reviewing its current plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;dev;mounts;help;plan' {
            break
        }
        'mm;dev;mounts;help;configure' {
            break
        }
        'mm;dev;mounts;help;disable' {
            break
        }
        'mm;dev;mounts;help;help' {
            break
        }
        'mm;dev;open' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;dev;help' {
            [CompletionResult]::new('setup', 'setup', [CompletionResultType]::ParameterValue, 'Set up local trust, ingress, Mailpit, Cap, dashboard, and project routes')
            [CompletionResult]::new('up', 'up', [CompletionResultType]::ParameterValue, 'Review or apply the local development deployment')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Turn optional modules, services or jobs on for this machine; `mm dev up` starts them')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Turn optional units off for this machine, stop them and remove their routes; their volumes stay')
            [CompletionResult]::new('down', 'down', [CompletionResultType]::ParameterValue, 'Stop the project''s containers; volumes, routes, and overrides are kept')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show local project and runtime status')
            [CompletionResult]::new('tls', 'tls', [CompletionResultType]::ParameterValue, 'Inspect and maintain local HTTPS certificates and trust')
            [CompletionResult]::new('services', 'services', [CompletionResultType]::ParameterValue, 'Manage the installation-wide local service stack')
            [CompletionResult]::new('dashboard', 'dashboard', [CompletionResultType]::ParameterValue, 'Serve the local developer dashboard')
            [CompletionResult]::new('mounts', 'mounts', [CompletionResultType]::ParameterValue, 'Discover, review, and disable source-code mounts')
            [CompletionResult]::new('open', 'open', [CompletionResultType]::ParameterValue, 'Print a configured local service URL')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;dev;help;setup' {
            [CompletionResult]::new('project', 'project', [CompletionResultType]::ParameterValue, 'Inspect and deploy an existing clone using its configured development environment')
            break
        }
        'mm;dev;help;setup;project' {
            break
        }
        'mm;dev;help;up' {
            break
        }
        'mm;dev;help;enable' {
            break
        }
        'mm;dev;help;disable' {
            break
        }
        'mm;dev;help;down' {
            break
        }
        'mm;dev;help;status' {
            break
        }
        'mm;dev;help;tls' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Query CA files, leaf renewal, and the actual platform trust store')
            [CompletionResult]::new('repair', 'repair', [CompletionResultType]::ParameterValue, 'Restore trust, renew stale leaves, and finish an interrupted rotation')
            [CompletionResult]::new('rotate', 'rotate', [CompletionResultType]::ParameterValue, 'Rotate the root with staged trust overlap and reissued leaves')
            [CompletionResult]::new('uninstall', 'uninstall', [CompletionResultType]::ParameterValue, 'Remove only exact mm-owned trust entries and local private material')
            break
        }
        'mm;dev;help;tls;status' {
            break
        }
        'mm;dev;help;tls;repair' {
            break
        }
        'mm;dev;help;tls;rotate' {
            break
        }
        'mm;dev;help;tls;uninstall' {
            break
        }
        'mm;dev;help;services' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Publish a deterministic reviewed lifecycle plan')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Apply an exact install/convergence plan hash')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show component health, ownership, image, route, certificate, and drift status')
            [CompletionResult]::new('upgrade', 'upgrade', [CompletionResultType]::ParameterValue, 'Apply an exact version upgrade plan hash')
            [CompletionResult]::new('repair', 'repair', [CompletionResultType]::ParameterValue, 'Recover an interrupted operation or reconcile drift from an exact plan')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove only mm-owned runtime resources using an exact reviewed plan')
            break
        }
        'mm;dev;help;services;plan' {
            break
        }
        'mm;dev;help;services;apply' {
            break
        }
        'mm;dev;help;services;status' {
            break
        }
        'mm;dev;help;services;upgrade' {
            break
        }
        'mm;dev;help;services;repair' {
            break
        }
        'mm;dev;help;services;remove' {
            break
        }
        'mm;dev;help;dashboard' {
            [CompletionResult]::new('serve', 'serve', [CompletionResultType]::ParameterValue, 'Run the ingress-authenticated dashboard backend')
            break
        }
        'mm;dev;help;dashboard;serve' {
            break
        }
        'mm;dev;help;mounts' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Produce a deterministic mount proposal with discovery evidence')
            [CompletionResult]::new('configure', 'configure', [CompletionResultType]::ParameterValue, 'Persist the exact reviewed proposal in the project config')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Remove a configured mount after reviewing its current plan')
            break
        }
        'mm;dev;help;mounts;plan' {
            break
        }
        'mm;dev;help;mounts;configure' {
            break
        }
        'mm;dev;help;mounts;disable' {
            break
        }
        'mm;dev;help;open' {
            break
        }
        'mm;dev;help;help' {
            break
        }
        'mm;config' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('validate', 'validate', [CompletionResultType]::ParameterValue, 'Validate .config/mm.yaml, its split files, and local overrides, without probing a runtime')
            [CompletionResult]::new('module', 'module', [CompletionResultType]::ParameterValue, 'Review typed module declarations')
            [CompletionResult]::new('service', 'service', [CompletionResultType]::ParameterValue, 'Review typed service declarations')
            [CompletionResult]::new('stack', 'stack', [CompletionResultType]::ParameterValue, 'Review stack declarations or compile generated artifacts')
            [CompletionResult]::new('build', 'build', [CompletionResultType]::ParameterValue, 'Review typed image-build declarations')
            [CompletionResult]::new('environment', 'environment', [CompletionResultType]::ParameterValue, 'Review typed environment declarations')
            [CompletionResult]::new('binding', 'binding', [CompletionResultType]::ParameterValue, 'Review typed environment bindings')
            [CompletionResult]::new('secret', 'secret', [CompletionResultType]::ParameterValue, 'Review secret declarations and references, never material')
            [CompletionResult]::new('ingress', 'ingress', [CompletionResultType]::ParameterValue, 'Review ingress declarations or inspect compiled routes')
            [CompletionResult]::new('mount', 'mount', [CompletionResultType]::ParameterValue, 'Review typed development mount declarations')
            [CompletionResult]::new('job', 'job', [CompletionResultType]::ParameterValue, 'Review typed finite-job declarations')
            [CompletionResult]::new('schedule', 'schedule', [CompletionResultType]::ParameterValue, 'Review typed schedule declarations')
            [CompletionResult]::new('access', 'access', [CompletionResultType]::ParameterValue, 'Review typed application-access declarations')
            [CompletionResult]::new('user', 'user', [CompletionResultType]::ParameterValue, 'Review who may reach remote environments (the users: key)')
            [CompletionResult]::new('schema', 'schema', [CompletionResultType]::ParameterValue, 'Print the generated JSON Schema for .config/mm.yaml, or for one split file''s value')
            [CompletionResult]::new('split', 'split', [CompletionResultType]::ParameterValue, 'Plan moving a top-level key into its own .config/mm/KEY.yaml; apply only with --plan and --yes')
            [CompletionResult]::new('join', 'join', [CompletionResultType]::ParameterValue, 'Plan moving a split key back into .config/mm.yaml; apply only with --plan and --yes')
            [CompletionResult]::new('sources', 'sources', [CompletionResultType]::ParameterValue, 'Show which file declares each top-level key')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;validate' {
            [CompletionResult]::new('--host-policy', '--host-policy', [CompletionResultType]::ParameterName, 'With --for-remote: check against this copy of a host''s host.yaml, its `allow` list and, without --environment, the environments it runs')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--for-remote', '--for-remote', [CompletionResultType]::ParameterName, 'Also refuse Compose features a remote host blocks, in the services the environment runs; without --environment, in every environment whose target is not `local`')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;module' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;module;add' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;module;edit' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;module;remove' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier removal review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed removal; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;module;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;module;help;add' {
            break
        }
        'mm;config;module;help;edit' {
            break
        }
        'mm;config;module;help;remove' {
            break
        }
        'mm;config;module;help;help' {
            break
        }
        'mm;config;service' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;service;add' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;service;edit' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;service;remove' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier removal review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed removal; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;service;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;service;help;add' {
            break
        }
        'mm;config;service;help;edit' {
            break
        }
        'mm;config;service;help;remove' {
            break
        }
        'mm;config;service;help;help' {
            break
        }
        'mm;config;stack' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a stack declaration')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing a stack declaration')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing a stack declaration')
            [CompletionResult]::new('compile', 'compile', [CompletionResultType]::ParameterValue, 'Publish deterministic Compose manifests and generated overlays')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;stack;add' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;stack;edit' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;stack;remove' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier removal review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed removal; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;stack;compile' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;stack;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a stack declaration')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing a stack declaration')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing a stack declaration')
            [CompletionResult]::new('compile', 'compile', [CompletionResultType]::ParameterValue, 'Publish deterministic Compose manifests and generated overlays')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;stack;help;add' {
            break
        }
        'mm;config;stack;help;edit' {
            break
        }
        'mm;config;stack;help;remove' {
            break
        }
        'mm;config;stack;help;compile' {
            break
        }
        'mm;config;stack;help;help' {
            break
        }
        'mm;config;build' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;build;add' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;build;edit' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;build;remove' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier removal review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed removal; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;build;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;build;help;add' {
            break
        }
        'mm;config;build;help;edit' {
            break
        }
        'mm;config;build;help;remove' {
            break
        }
        'mm;config;build;help;help' {
            break
        }
        'mm;config;environment' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;environment;add' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;environment;edit' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;environment;remove' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier removal review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed removal; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;environment;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;environment;help;add' {
            break
        }
        'mm;config;environment;help;edit' {
            break
        }
        'mm;config;environment;help;remove' {
            break
        }
        'mm;config;environment;help;help' {
            break
        }
        'mm;config;binding' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;binding;add' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;binding;edit' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;binding;remove' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier removal review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed removal; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;binding;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;binding;help;add' {
            break
        }
        'mm;config;binding;help;edit' {
            break
        }
        'mm;config;binding;help;remove' {
            break
        }
        'mm;config;binding;help;help' {
            break
        }
        'mm;config;secret' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;secret;add' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;secret;edit' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;secret;remove' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier removal review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed removal; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;secret;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;secret;help;add' {
            break
        }
        'mm;config;secret;help;edit' {
            break
        }
        'mm;config;secret;help;remove' {
            break
        }
        'mm;config;secret;help;help' {
            break
        }
        'mm;config;ingress' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding an ingress declaration')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing an ingress declaration')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing an ingress declaration')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List every service URL or the reason it was not routed')
            [CompletionResult]::new('explain', 'explain', [CompletionResultType]::ParameterValue, 'Explain ingress selection for one service name')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;ingress;add' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;ingress;edit' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;ingress;remove' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier removal review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed removal; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;ingress;list' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;ingress;explain' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;ingress;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding an ingress declaration')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing an ingress declaration')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing an ingress declaration')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List every service URL or the reason it was not routed')
            [CompletionResult]::new('explain', 'explain', [CompletionResultType]::ParameterValue, 'Explain ingress selection for one service name')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;ingress;help;add' {
            break
        }
        'mm;config;ingress;help;edit' {
            break
        }
        'mm;config;ingress;help;remove' {
            break
        }
        'mm;config;ingress;help;list' {
            break
        }
        'mm;config;ingress;help;explain' {
            break
        }
        'mm;config;ingress;help;help' {
            break
        }
        'mm;config;mount' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;mount;add' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;mount;edit' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;mount;remove' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier removal review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed removal; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;mount;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;mount;help;add' {
            break
        }
        'mm;config;mount;help;edit' {
            break
        }
        'mm;config;mount;help;remove' {
            break
        }
        'mm;config;mount;help;help' {
            break
        }
        'mm;config;job' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;job;add' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;job;edit' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;job;remove' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier removal review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed removal; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;job;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;job;help;add' {
            break
        }
        'mm;config;job;help;edit' {
            break
        }
        'mm;config;job;help;remove' {
            break
        }
        'mm;config;job;help;help' {
            break
        }
        'mm;config;schedule' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;schedule;add' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;schedule;edit' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;schedule;remove' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier removal review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed removal; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;schedule;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;schedule;help;add' {
            break
        }
        'mm;config;schedule;help;edit' {
            break
        }
        'mm;config;schedule;help;remove' {
            break
        }
        'mm;config;schedule;help;help' {
            break
        }
        'mm;config;access' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;access;add' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;access;edit' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;access;remove' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier removal review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed removal; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;access;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;access;help;add' {
            break
        }
        'mm;config;access;help;edit' {
            break
        }
        'mm;config;access;help;remove' {
            break
        }
        'mm;config;access;help;help' {
            break
        }
        'mm;config;user' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;user;add' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;user;edit' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--definition', '--definition', [CompletionResultType]::ParameterName, 'Public JSON or YAML definition. Secret plaintext is rejected')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review with identical arguments')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;user;remove' {
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Owning module, stack, or environment when the resource is nested')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier removal review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed removal; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;user;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;user;help;add' {
            break
        }
        'mm;config;user;help;edit' {
            break
        }
        'mm;config;user;help;remove' {
            break
        }
        'mm;config;user;help;help' {
            break
        }
        'mm;config;schema' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;split' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review of the same move')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;join' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review of the same move')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan; never bypasses stale-file checks')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;sources' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;config;help' {
            [CompletionResult]::new('validate', 'validate', [CompletionResultType]::ParameterValue, 'Validate .config/mm.yaml, its split files, and local overrides, without probing a runtime')
            [CompletionResult]::new('module', 'module', [CompletionResultType]::ParameterValue, 'Review typed module declarations')
            [CompletionResult]::new('service', 'service', [CompletionResultType]::ParameterValue, 'Review typed service declarations')
            [CompletionResult]::new('stack', 'stack', [CompletionResultType]::ParameterValue, 'Review stack declarations or compile generated artifacts')
            [CompletionResult]::new('build', 'build', [CompletionResultType]::ParameterValue, 'Review typed image-build declarations')
            [CompletionResult]::new('environment', 'environment', [CompletionResultType]::ParameterValue, 'Review typed environment declarations')
            [CompletionResult]::new('binding', 'binding', [CompletionResultType]::ParameterValue, 'Review typed environment bindings')
            [CompletionResult]::new('secret', 'secret', [CompletionResultType]::ParameterValue, 'Review secret declarations and references, never material')
            [CompletionResult]::new('ingress', 'ingress', [CompletionResultType]::ParameterValue, 'Review ingress declarations or inspect compiled routes')
            [CompletionResult]::new('mount', 'mount', [CompletionResultType]::ParameterValue, 'Review typed development mount declarations')
            [CompletionResult]::new('job', 'job', [CompletionResultType]::ParameterValue, 'Review typed finite-job declarations')
            [CompletionResult]::new('schedule', 'schedule', [CompletionResultType]::ParameterValue, 'Review typed schedule declarations')
            [CompletionResult]::new('access', 'access', [CompletionResultType]::ParameterValue, 'Review typed application-access declarations')
            [CompletionResult]::new('user', 'user', [CompletionResultType]::ParameterValue, 'Review who may reach remote environments (the users: key)')
            [CompletionResult]::new('schema', 'schema', [CompletionResultType]::ParameterValue, 'Print the generated JSON Schema for .config/mm.yaml, or for one split file''s value')
            [CompletionResult]::new('split', 'split', [CompletionResultType]::ParameterValue, 'Plan moving a top-level key into its own .config/mm/KEY.yaml; apply only with --plan and --yes')
            [CompletionResult]::new('join', 'join', [CompletionResultType]::ParameterValue, 'Plan moving a split key back into .config/mm.yaml; apply only with --plan and --yes')
            [CompletionResult]::new('sources', 'sources', [CompletionResultType]::ParameterValue, 'Show which file declares each top-level key')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;config;help;validate' {
            break
        }
        'mm;config;help;module' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;config;help;module;add' {
            break
        }
        'mm;config;help;module;edit' {
            break
        }
        'mm;config;help;module;remove' {
            break
        }
        'mm;config;help;service' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;config;help;service;add' {
            break
        }
        'mm;config;help;service;edit' {
            break
        }
        'mm;config;help;service;remove' {
            break
        }
        'mm;config;help;stack' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a stack declaration')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing a stack declaration')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing a stack declaration')
            [CompletionResult]::new('compile', 'compile', [CompletionResultType]::ParameterValue, 'Publish deterministic Compose manifests and generated overlays')
            break
        }
        'mm;config;help;stack;add' {
            break
        }
        'mm;config;help;stack;edit' {
            break
        }
        'mm;config;help;stack;remove' {
            break
        }
        'mm;config;help;stack;compile' {
            break
        }
        'mm;config;help;build' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;config;help;build;add' {
            break
        }
        'mm;config;help;build;edit' {
            break
        }
        'mm;config;help;build;remove' {
            break
        }
        'mm;config;help;environment' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;config;help;environment;add' {
            break
        }
        'mm;config;help;environment;edit' {
            break
        }
        'mm;config;help;environment;remove' {
            break
        }
        'mm;config;help;binding' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;config;help;binding;add' {
            break
        }
        'mm;config;help;binding;edit' {
            break
        }
        'mm;config;help;binding;remove' {
            break
        }
        'mm;config;help;secret' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;config;help;secret;add' {
            break
        }
        'mm;config;help;secret;edit' {
            break
        }
        'mm;config;help;secret;remove' {
            break
        }
        'mm;config;help;ingress' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding an ingress declaration')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing an ingress declaration')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing an ingress declaration')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List every service URL or the reason it was not routed')
            [CompletionResult]::new('explain', 'explain', [CompletionResultType]::ParameterValue, 'Explain ingress selection for one service name')
            break
        }
        'mm;config;help;ingress;add' {
            break
        }
        'mm;config;help;ingress;edit' {
            break
        }
        'mm;config;help;ingress;remove' {
            break
        }
        'mm;config;help;ingress;list' {
            break
        }
        'mm;config;help;ingress;explain' {
            break
        }
        'mm;config;help;mount' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;config;help;mount;add' {
            break
        }
        'mm;config;help;mount;edit' {
            break
        }
        'mm;config;help;mount;remove' {
            break
        }
        'mm;config;help;job' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;config;help;job;add' {
            break
        }
        'mm;config;help;job;edit' {
            break
        }
        'mm;config;help;job;remove' {
            break
        }
        'mm;config;help;schedule' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;config;help;schedule;add' {
            break
        }
        'mm;config;help;schedule;edit' {
            break
        }
        'mm;config;help;schedule;remove' {
            break
        }
        'mm;config;help;access' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;config;help;access;add' {
            break
        }
        'mm;config;help;access;edit' {
            break
        }
        'mm;config;help;access;remove' {
            break
        }
        'mm;config;help;user' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;config;help;user;add' {
            break
        }
        'mm;config;help;user;edit' {
            break
        }
        'mm;config;help;user;remove' {
            break
        }
        'mm;config;help;schema' {
            break
        }
        'mm;config;help;split' {
            break
        }
        'mm;config;help;join' {
            break
        }
        'mm;config;help;sources' {
            break
        }
        'mm;config;help;help' {
            break
        }
        'mm;doctor' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--check-updates', '--check-updates', [CompletionResultType]::ParameterName, 'Fetch the stable release manifest and compare the installed version')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('recover-state', 'recover-state', [CompletionResultType]::ParameterValue, 'Review recovery of pre-baseline state; apply requires the reviewed hash and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;doctor;recover-state' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash from the recovery review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Confirm backup and replacement of the reviewed legacy state')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;doctor;help' {
            [CompletionResult]::new('recover-state', 'recover-state', [CompletionResultType]::ParameterValue, 'Review recovery of pre-baseline state; apply requires the reviewed hash and --yes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;doctor;help;recover-state' {
            break
        }
        'mm;doctor;help;help' {
            break
        }
        'mm;ingress' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('start', 'start', [CompletionResultType]::ParameterValue, 'Ensure ingress is installed and healthy, then start public forwarding on ports 80 and 443')
            [CompletionResult]::new('stop', 'stop', [CompletionResultType]::ParameterValue, 'Stop global public forwarding to free ports 80 and 443; keep project containers running')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show public forwarding state and the health of the services it depends on (read-only)')
            [CompletionResult]::new('restart', 'restart', [CompletionResultType]::ParameterValue, 'Reconcile ingress prerequisites, then restart public forwarding on ports 80 and 443')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;ingress;start' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Approve required installation or repair plans without prompting')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;ingress;stop' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;ingress;status' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;ingress;restart' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Approve required installation or repair plans without prompting')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;ingress;help' {
            [CompletionResult]::new('start', 'start', [CompletionResultType]::ParameterValue, 'Ensure ingress is installed and healthy, then start public forwarding on ports 80 and 443')
            [CompletionResult]::new('stop', 'stop', [CompletionResultType]::ParameterValue, 'Stop global public forwarding to free ports 80 and 443; keep project containers running')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show public forwarding state and the health of the services it depends on (read-only)')
            [CompletionResult]::new('restart', 'restart', [CompletionResultType]::ParameterValue, 'Reconcile ingress prerequisites, then restart public forwarding on ports 80 and 443')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;ingress;help;start' {
            break
        }
        'mm;ingress;help;stop' {
            break
        }
        'mm;ingress;help;status' {
            break
        }
        'mm;ingress;help;restart' {
            break
        }
        'mm;ingress;help;help' {
            break
        }
        'mm;service' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show desired and observed state, optionally for one service')
            [CompletionResult]::new('logs', 'logs', [CompletionResultType]::ParameterValue, 'Read a service''s log, or every service''s, from all its containers in the order the lines were written')
            [CompletionResult]::new('exec', 'exec', [CompletionResultType]::ParameterValue, 'Execute an exact argv vector; no shell is implied')
            [CompletionResult]::new('shell', 'shell', [CompletionResultType]::ParameterValue, 'Open an explicitly approved high-risk shell')
            [CompletionResult]::new('restart', 'restart', [CompletionResultType]::ParameterValue, 'Restart one service through runtime policy')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;service;status' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;service;logs' {
            [CompletionResult]::new('--stack', '--stack', [CompletionResultType]::ParameterName, 'Restrict the service to one configured stack')
            [CompletionResult]::new('--node', '--node', [CompletionResultType]::ParameterName, 'On a cluster target, read only this node. Repeat it for more')
            [CompletionResult]::new('--tail', '--tail', [CompletionResultType]::ParameterName, 'Number of recent records requested before following')
            [CompletionResult]::new('--since', '--since', [CompletionResultType]::ParameterName, 'Include records at or after this Unix millisecond timestamp')
            [CompletionResult]::new('--until', '--until', [CompletionResultType]::ParameterName, 'Include records before this Unix millisecond timestamp')
            [CompletionResult]::new('--max-line-bytes', '--max-line-bytes', [CompletionResultType]::ParameterName, 'Maximum bytes retained from any one log line')
            [CompletionResult]::new('--max-bytes-per-second', '--max-bytes-per-second', [CompletionResultType]::ParameterName, 'Maximum aggregate log bytes accepted per second')
            [CompletionResult]::new('--max-lines-per-second', '--max-lines-per-second', [CompletionResultType]::ParameterName, 'Maximum aggregate log records accepted per second')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--follow', '--follow', [CompletionResultType]::ParameterName, 'Continue streaming new records until interrupted')
            [CompletionResult]::new('--timestamps', '--timestamps', [CompletionResultType]::ParameterName, 'Show when the engine recorded each line')
            [CompletionResult]::new('--stdout', '--stdout', [CompletionResultType]::ParameterName, 'Include only standard output records')
            [CompletionResult]::new('--stderr', '--stderr', [CompletionResultType]::ParameterName, 'Include only standard error records')
            [CompletionResult]::new('--raw', '--raw', [CompletionResultType]::ParameterName, 'Disable redaction. Requires --approve-high-risk')
            [CompletionResult]::new('--approve-high-risk', '--approve-high-risk', [CompletionResultType]::ParameterName, 'Confirm that raw application logs can disclose arbitrary secret values')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;service;exec' {
            [CompletionResult]::new('--stack', '--stack', [CompletionResultType]::ParameterName, 'Restrict the service to one configured stack')
            [CompletionResult]::new('--container', '--container', [CompletionResultType]::ParameterName, 'Select one exact container when the service has multiple replicas')
            [CompletionResult]::new('--user', '--user', [CompletionResultType]::ParameterName, 'User or numeric identity used inside the container')
            [CompletionResult]::new('--workdir', '--workdir', [CompletionResultType]::ParameterName, 'Working directory used inside the container')
            [CompletionResult]::new('--env', '--env', [CompletionResultType]::ParameterName, 'Set NAME from a secret reference (NAME=provider://location); plaintext is rejected')
            [CompletionResult]::new('--timeout', '--timeout', [CompletionResultType]::ParameterName, 'Maximum command runtime before cancellation')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--tty', '--tty', [CompletionResultType]::ParameterName, 'Allocate an interactive terminal; output capture is disabled')
            [CompletionResult]::new('--stdin', '--stdin', [CompletionResultType]::ParameterName, 'Forward standard input to the remote command')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;service;shell' {
            [CompletionResult]::new('--stack', '--stack', [CompletionResultType]::ParameterName, 'Restrict the service to one configured stack')
            [CompletionResult]::new('--container', '--container', [CompletionResultType]::ParameterName, 'Select one exact container when the service has multiple replicas')
            [CompletionResult]::new('--user', '--user', [CompletionResultType]::ParameterName, 'User or numeric identity used inside the container')
            [CompletionResult]::new('--workdir', '--workdir', [CompletionResultType]::ParameterName, 'Working directory used inside the container')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--approve-high-risk', '--approve-high-risk', [CompletionResultType]::ParameterName, 'Confirm the shell''s high-risk, authorization-gated behavior')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;service;restart' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;service;help' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show desired and observed state, optionally for one service')
            [CompletionResult]::new('logs', 'logs', [CompletionResultType]::ParameterValue, 'Read a service''s log, or every service''s, from all its containers in the order the lines were written')
            [CompletionResult]::new('exec', 'exec', [CompletionResultType]::ParameterValue, 'Execute an exact argv vector; no shell is implied')
            [CompletionResult]::new('shell', 'shell', [CompletionResultType]::ParameterValue, 'Open an explicitly approved high-risk shell')
            [CompletionResult]::new('restart', 'restart', [CompletionResultType]::ParameterValue, 'Restart one service through runtime policy')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;service;help;status' {
            break
        }
        'mm;service;help;logs' {
            break
        }
        'mm;service;help;exec' {
            break
        }
        'mm;service;help;shell' {
            break
        }
        'mm;service;help;restart' {
            break
        }
        'mm;service;help;help' {
            break
        }
        'mm;job' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List the declared jobs of the environment: when a deployment runs each, on which node, and how its last run ended')
            [CompletionResult]::new('run', 'run', [CompletionResultType]::ParameterValue, 'Enqueue a named job and wait for its durable terminal status')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show one durable run by UUID')
            [CompletionResult]::new('history', 'history', [CompletionResultType]::ParameterValue, 'List bounded run history, optionally for one job')
            [CompletionResult]::new('cancel', 'cancel', [CompletionResultType]::ParameterValue, 'Request cancellation of a queued or active run')
            [CompletionResult]::new('retry', 'retry', [CompletionResultType]::ParameterValue, 'Enqueue a new run from the current definition of a terminal run')
            [CompletionResult]::new('schedule', 'schedule', [CompletionResultType]::ParameterValue, 'Operate schedules for configured jobs')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;job;list' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;run' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--no-wait', '--no-wait', [CompletionResultType]::ParameterName, 'Return after durable enqueue instead of driving execution')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;status' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;history' {
            [CompletionResult]::new('--limit', '--limit', [CompletionResultType]::ParameterName, 'Maximum number of newest runs to return')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;cancel' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;retry' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--no-wait', '--no-wait', [CompletionResultType]::ParameterName, 'Return after durable enqueue instead of driving execution')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;schedule' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('reconcile', 'reconcile', [CompletionResultType]::ParameterValue, 'Plan or apply transactional definition reconciliation')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List schedules and their durable next due time')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show one schedule')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Enable one reconciled schedule')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Disable one reconciled schedule without deleting history')
            [CompletionResult]::new('run-now', 'run-now', [CompletionResultType]::ParameterValue, 'Enqueue the schedule''s job immediately, independent of enabled state')
            [CompletionResult]::new('next', 'next', [CompletionResultType]::ParameterValue, 'Show the durable next due time for one schedule')
            [CompletionResult]::new('history', 'history', [CompletionResultType]::ParameterValue, 'List bounded history for one schedule')
            [CompletionResult]::new('tick', 'tick', [CompletionResultType]::ParameterValue, 'Execute one fake-clock-friendly controller cycle')
            [CompletionResult]::new('daemon', 'daemon', [CompletionResultType]::ParameterValue, 'Run the scheduler controller until interrupted')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;job;schedule;reconcile' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by the schedule reconciliation review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the reviewed reconciliation plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;schedule;list' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;schedule;show' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;schedule;enable' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;schedule;disable' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;schedule;run-now' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--no-wait', '--no-wait', [CompletionResultType]::ParameterName, 'Return after durable enqueue instead of driving execution')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;schedule;next' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;schedule;history' {
            [CompletionResult]::new('--limit', '--limit', [CompletionResultType]::ParameterName, 'Maximum number of newest runs to return')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;schedule;tick' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;schedule;daemon' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;job;schedule;help' {
            [CompletionResult]::new('reconcile', 'reconcile', [CompletionResultType]::ParameterValue, 'Plan or apply transactional definition reconciliation')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List schedules and their durable next due time')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show one schedule')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Enable one reconciled schedule')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Disable one reconciled schedule without deleting history')
            [CompletionResult]::new('run-now', 'run-now', [CompletionResultType]::ParameterValue, 'Enqueue the schedule''s job immediately, independent of enabled state')
            [CompletionResult]::new('next', 'next', [CompletionResultType]::ParameterValue, 'Show the durable next due time for one schedule')
            [CompletionResult]::new('history', 'history', [CompletionResultType]::ParameterValue, 'List bounded history for one schedule')
            [CompletionResult]::new('tick', 'tick', [CompletionResultType]::ParameterValue, 'Execute one fake-clock-friendly controller cycle')
            [CompletionResult]::new('daemon', 'daemon', [CompletionResultType]::ParameterValue, 'Run the scheduler controller until interrupted')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;job;schedule;help;reconcile' {
            break
        }
        'mm;job;schedule;help;list' {
            break
        }
        'mm;job;schedule;help;show' {
            break
        }
        'mm;job;schedule;help;enable' {
            break
        }
        'mm;job;schedule;help;disable' {
            break
        }
        'mm;job;schedule;help;run-now' {
            break
        }
        'mm;job;schedule;help;next' {
            break
        }
        'mm;job;schedule;help;history' {
            break
        }
        'mm;job;schedule;help;tick' {
            break
        }
        'mm;job;schedule;help;daemon' {
            break
        }
        'mm;job;schedule;help;help' {
            break
        }
        'mm;job;help' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List the declared jobs of the environment: when a deployment runs each, on which node, and how its last run ended')
            [CompletionResult]::new('run', 'run', [CompletionResultType]::ParameterValue, 'Enqueue a named job and wait for its durable terminal status')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show one durable run by UUID')
            [CompletionResult]::new('history', 'history', [CompletionResultType]::ParameterValue, 'List bounded run history, optionally for one job')
            [CompletionResult]::new('cancel', 'cancel', [CompletionResultType]::ParameterValue, 'Request cancellation of a queued or active run')
            [CompletionResult]::new('retry', 'retry', [CompletionResultType]::ParameterValue, 'Enqueue a new run from the current definition of a terminal run')
            [CompletionResult]::new('schedule', 'schedule', [CompletionResultType]::ParameterValue, 'Operate schedules for configured jobs')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;job;help;list' {
            break
        }
        'mm;job;help;run' {
            break
        }
        'mm;job;help;status' {
            break
        }
        'mm;job;help;history' {
            break
        }
        'mm;job;help;cancel' {
            break
        }
        'mm;job;help;retry' {
            break
        }
        'mm;job;help;schedule' {
            [CompletionResult]::new('reconcile', 'reconcile', [CompletionResultType]::ParameterValue, 'Plan or apply transactional definition reconciliation')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List schedules and their durable next due time')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show one schedule')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Enable one reconciled schedule')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Disable one reconciled schedule without deleting history')
            [CompletionResult]::new('run-now', 'run-now', [CompletionResultType]::ParameterValue, 'Enqueue the schedule''s job immediately, independent of enabled state')
            [CompletionResult]::new('next', 'next', [CompletionResultType]::ParameterValue, 'Show the durable next due time for one schedule')
            [CompletionResult]::new('history', 'history', [CompletionResultType]::ParameterValue, 'List bounded history for one schedule')
            [CompletionResult]::new('tick', 'tick', [CompletionResultType]::ParameterValue, 'Execute one fake-clock-friendly controller cycle')
            [CompletionResult]::new('daemon', 'daemon', [CompletionResultType]::ParameterValue, 'Run the scheduler controller until interrupted')
            break
        }
        'mm;job;help;schedule;reconcile' {
            break
        }
        'mm;job;help;schedule;list' {
            break
        }
        'mm;job;help;schedule;show' {
            break
        }
        'mm;job;help;schedule;enable' {
            break
        }
        'mm;job;help;schedule;disable' {
            break
        }
        'mm;job;help;schedule;run-now' {
            break
        }
        'mm;job;help;schedule;next' {
            break
        }
        'mm;job;help;schedule;history' {
            break
        }
        'mm;job;help;schedule;tick' {
            break
        }
        'mm;job;help;schedule;daemon' {
            break
        }
        'mm;job;help;help' {
            break
        }
        'mm;env' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show effective values, provenance, and secret status')
            [CompletionResult]::new('explain', 'explain', [CompletionResultType]::ParameterValue, 'Explain every use of one global key or local variable')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set a workspace-local non-secret override. On a remote environment the value is staged on every host for the next deployment')
            [CompletionResult]::new('unset', 'unset', [CompletionResultType]::ParameterValue, 'Remove a workspace-local override. On a remote environment the removal is staged for the next deployment')
            [CompletionResult]::new('discard', 'discard', [CompletionResultType]::ParameterValue, 'Drop a variable change staged on a remote environment''s hosts before it is deployed')
            [CompletionResult]::new('default', 'default', [CompletionResultType]::ParameterValue, 'Get or change the project-wide default environment')
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'Resolve and compile every service environment in memory')
            [CompletionResult]::new('materialize', 'materialize', [CompletionResultType]::ParameterValue, 'Explicitly write a module''s conventional .env file')
            [CompletionResult]::new('diff', 'diff', [CompletionResultType]::ParameterValue, 'Compare a module''s .env with the effective compiled environment')
            [CompletionResult]::new('review', 'review', [CompletionResultType]::ParameterValue, 'Explain source and delivery without revealing secret values')
            [CompletionResult]::new('setup', 'setup', [CompletionResultType]::ParameterValue, 'Show missing bindings and the resolved delivery mode')
            [CompletionResult]::new('delivery', 'delivery', [CompletionResultType]::ParameterValue, 'Show or persist environment delivery mode')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;env;status' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--no-check-secrets', '--no-check-secrets', [CompletionResultType]::ParameterName, 'Do not contact secret providers')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;env;explain' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--no-check-secrets', '--no-check-secrets', [CompletionResultType]::ParameterName, 'Do not contact secret providers')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;env;set' {
            [CompletionResult]::new('--expected-revision', '--expected-revision', [CompletionResultType]::ParameterName, 'Fail if the existing override has another revision')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--stdin', '--stdin', [CompletionResultType]::ParameterName, 'Read the value from stdin instead of an argument')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;env;unset' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;env;discard' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;env;default' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Print the configured project default environment')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Persist a new project default environment')
            [CompletionResult]::new('unset', 'unset', [CompletionResultType]::ParameterValue, 'Remove the project default environment')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;env;default;show' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;env;default;set' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;env;default;unset' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;env;default;help' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Print the configured project default environment')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Persist a new project default environment')
            [CompletionResult]::new('unset', 'unset', [CompletionResultType]::ParameterValue, 'Remove the project default environment')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;env;default;help;show' {
            break
        }
        'mm;env;default;help;set' {
            break
        }
        'mm;env;default;help;unset' {
            break
        }
        'mm;env;default;help;help' {
            break
        }
        'mm;env;check' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;env;materialize' {
            [CompletionResult]::new('--service', '--service', [CompletionResultType]::ParameterName, 'Limit materialization to one service in the module')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--force', '--force', [CompletionResultType]::ParameterName, 'Overwrite an existing .env file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;env;diff' {
            [CompletionResult]::new('--service', '--service', [CompletionResultType]::ParameterName, 'Limit the diff to one service in the module')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;env;review' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;env;setup' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;env;delivery' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show the resolved delivery mode and provenance')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Persist an explicit delivery mode for one environment')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;env;delivery;show' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;env;delivery;set' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;env;delivery;help' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show the resolved delivery mode and provenance')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Persist an explicit delivery mode for one environment')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;env;delivery;help;show' {
            break
        }
        'mm;env;delivery;help;set' {
            break
        }
        'mm;env;delivery;help;help' {
            break
        }
        'mm;env;help' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show effective values, provenance, and secret status')
            [CompletionResult]::new('explain', 'explain', [CompletionResultType]::ParameterValue, 'Explain every use of one global key or local variable')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set a workspace-local non-secret override. On a remote environment the value is staged on every host for the next deployment')
            [CompletionResult]::new('unset', 'unset', [CompletionResultType]::ParameterValue, 'Remove a workspace-local override. On a remote environment the removal is staged for the next deployment')
            [CompletionResult]::new('discard', 'discard', [CompletionResultType]::ParameterValue, 'Drop a variable change staged on a remote environment''s hosts before it is deployed')
            [CompletionResult]::new('default', 'default', [CompletionResultType]::ParameterValue, 'Get or change the project-wide default environment')
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'Resolve and compile every service environment in memory')
            [CompletionResult]::new('materialize', 'materialize', [CompletionResultType]::ParameterValue, 'Explicitly write a module''s conventional .env file')
            [CompletionResult]::new('diff', 'diff', [CompletionResultType]::ParameterValue, 'Compare a module''s .env with the effective compiled environment')
            [CompletionResult]::new('review', 'review', [CompletionResultType]::ParameterValue, 'Explain source and delivery without revealing secret values')
            [CompletionResult]::new('setup', 'setup', [CompletionResultType]::ParameterValue, 'Show missing bindings and the resolved delivery mode')
            [CompletionResult]::new('delivery', 'delivery', [CompletionResultType]::ParameterValue, 'Show or persist environment delivery mode')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;env;help;status' {
            break
        }
        'mm;env;help;explain' {
            break
        }
        'mm;env;help;set' {
            break
        }
        'mm;env;help;unset' {
            break
        }
        'mm;env;help;discard' {
            break
        }
        'mm;env;help;default' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Print the configured project default environment')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Persist a new project default environment')
            [CompletionResult]::new('unset', 'unset', [CompletionResultType]::ParameterValue, 'Remove the project default environment')
            break
        }
        'mm;env;help;default;show' {
            break
        }
        'mm;env;help;default;set' {
            break
        }
        'mm;env;help;default;unset' {
            break
        }
        'mm;env;help;check' {
            break
        }
        'mm;env;help;materialize' {
            break
        }
        'mm;env;help;diff' {
            break
        }
        'mm;env;help;review' {
            break
        }
        'mm;env;help;setup' {
            break
        }
        'mm;env;help;delivery' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show the resolved delivery mode and provenance')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Persist an explicit delivery mode for one environment')
            break
        }
        'mm;env;help;delivery;show' {
            break
        }
        'mm;env;help;delivery;set' {
            break
        }
        'mm;env;help;help' {
            break
        }
        'mm;secret' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('providers', 'providers', [CompletionResultType]::ParameterValue, 'List available provider capabilities')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Initialize a secret provider for this project')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show secret set/unset state without revealing values. On a remote environment: what each host holds and has staged')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set a configured secret using a hidden prompt or stdin. On a remote environment the value is staged on every host for the next deployment')
            [CompletionResult]::new('unset', 'unset', [CompletionResultType]::ParameterValue, 'Remove a configured secret. On a remote environment the removal is staged for the next deployment')
            [CompletionResult]::new('discard', 'discard', [CompletionResultType]::ParameterValue, 'Drop a change staged on a remote environment''s hosts before it is deployed')
            [CompletionResult]::new('generate', 'generate', [CompletionResultType]::ParameterValue, 'Create configured generated secrets without overwriting existing values')
            [CompletionResult]::new('rotate', 'rotate', [CompletionResultType]::ParameterValue, 'Explicitly replace one secret using its configured source')
            [CompletionResult]::new('migrate', 'migrate', [CompletionResultType]::ParameterValue, 'Move a cluster environment''s secrets from the age store into the store mm runs on its nodes. Each becomes a version there with the revision it has now, so pointing the secrets at the store afterwards replaces no container. Safe to run again')
            [CompletionResult]::new('rotation', 'rotation', [CompletionResultType]::ParameterValue, 'Plan or apply scheduled rotations')
            [CompletionResult]::new('provider', 'provider', [CompletionResultType]::ParameterValue, 'Manage OpenBao and Vault provider instances')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;secret;providers' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;init' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;status' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--local', '--local', [CompletionResultType]::ParameterName, 'Read this machine''s store, the one `mm target setup` pushes, even for a remote environment')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;set' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--stdin', '--stdin', [CompletionResultType]::ParameterName, 'Read exactly one value from stdin without prompting')
            [CompletionResult]::new('--local', '--local', [CompletionResultType]::ParameterName, 'Write this machine''s store, the one `mm target setup` pushes, even for a remote environment')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;unset' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the destructive confirmation prompt')
            [CompletionResult]::new('--local', '--local', [CompletionResultType]::ParameterName, 'Remove it from this machine''s store, even for a remote environment')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;discard' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;generate' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--missing', '--missing', [CompletionResultType]::ParameterName, 'Generate the secrets the selected environment uses: those its bindings name, its services mount, and its registries, TLS, tunnel and deploy key read')
            [CompletionResult]::new('--all', '--all', [CompletionResultType]::ParameterName, 'Generate every configured generated secret, whichever environment uses it')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;rotate' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--stdin', '--stdin', [CompletionResultType]::ParameterName, 'Read a prompted replacement from stdin')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact displayed rotation plan without an interactive confirmation')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;migrate' {
            [CompletionResult]::new('--from', '--from', [CompletionResultType]::ParameterName, 'The store the values come from')
            [CompletionResult]::new('--to', '--to', [CompletionResultType]::ParameterName, 'The managed store''s name under `providers:`')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;rotation' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Inspect metadata and produce an approval hash without changing secrets')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Apply the exact reviewed plan using revision-guarded writes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;secret;rotation;plan' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;rotation;apply' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash printed by `mm secret rotation plan`')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;rotation;help' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Inspect metadata and produce an approval hash without changing secrets')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Apply the exact reviewed plan using revision-guarded writes')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;secret;rotation;help;plan' {
            break
        }
        'mm;secret;rotation;help;apply' {
            break
        }
        'mm;secret;rotation;help;help' {
            break
        }
        'mm;secret;provider' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List configured provider instances and registered drivers')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show one provider instance with its typed descriptor and capabilities')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Configure a declared provider instance')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Reconfigure a declared provider instance without changing its driver or mode')
            [CompletionResult]::new('test', 'test', [CompletionResultType]::ParameterValue, 'Test one external provider instance')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show health and capabilities for one provider instance')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Produce a deterministic provider lifecycle plan')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Apply an exact provider lifecycle plan')
            [CompletionResult]::new('backup', 'backup', [CompletionResultType]::ParameterValue, 'Write an encrypted provider snapshot after exact review')
            [CompletionResult]::new('restore', 'restore', [CompletionResultType]::ParameterValue, 'Restore an encrypted provider snapshot after exact review. For the store mm runs on a cluster''s nodes: put a snapshot from `snapshot` back on every node; without --plan it shows the plan')
            [CompletionResult]::new('snapshot', 'snapshot', [CompletionResultType]::ParameterValue, 'Take a snapshot of the store mm runs on a cluster environment''s nodes: its data and the keys it opens with, sealed to you')
            [CompletionResult]::new('recovery', 'recovery', [CompletionResultType]::ParameterValue, 'The recovery copy of the store mm runs on a cluster environment''s nodes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a provider through its exact reviewed lifecycle plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;secret;provider;list' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;provider;show' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;provider;add' {
            [CompletionResult]::new('--driver', '--driver', [CompletionResultType]::ParameterName, 'Registered provider driver ID')
            [CompletionResult]::new('--mode', '--mode', [CompletionResultType]::ParameterName, 'Provider operating mode')
            [CompletionResult]::new('--scope', '--scope', [CompletionResultType]::ParameterName, 'Provider ownership scope')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--bootstrap', '--bootstrap', [CompletionResultType]::ParameterName, 'Enable missing mounts for an external provider')
            [CompletionResult]::new('--stdin', '--stdin', [CompletionResultType]::ParameterName, 'Read an external bootstrap credential from stdin instead of a hidden prompt')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply after displaying the exact setup or install plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;provider;edit' {
            [CompletionResult]::new('--driver', '--driver', [CompletionResultType]::ParameterName, 'Assert the existing driver; changing it requires replacement')
            [CompletionResult]::new('--mode', '--mode', [CompletionResultType]::ParameterName, 'Assert the existing mode; changing it requires replacement')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--bootstrap', '--bootstrap', [CompletionResultType]::ParameterName, 'Enable missing mounts for an external provider')
            [CompletionResult]::new('--stdin', '--stdin', [CompletionResultType]::ParameterName, 'Read the bootstrap credential from stdin instead of a hidden prompt')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply after displaying the exact setup plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;provider;test' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;provider;status' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;provider;plan' {
            [CompletionResult]::new('--operation', '--operation', [CompletionResultType]::ParameterName, 'Lifecycle operation represented by the plan')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--purge', '--purge', [CompletionResultType]::ParameterName, 'Delete owned persistent data when removing the provider')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;provider;apply' {
            [CompletionResult]::new('--operation', '--operation', [CompletionResultType]::ParameterName, 'Lifecycle operation represented by the plan')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash printed by the identical provider lifecycle plan')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--purge', '--purge', [CompletionResultType]::ParameterName, 'Delete owned persistent data when removing the provider')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan without prompting')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;provider;backup' {
            [CompletionResult]::new('--file', '--file', [CompletionResultType]::ParameterName, 'Encrypted snapshot source or destination')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash for the exact current state and selected path')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed backup or restore')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;provider;restore' {
            [CompletionResult]::new('--file', '--file', [CompletionResultType]::ParameterName, 'Encrypted snapshot source or destination')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash for the exact current state and selected path')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed backup or restore')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;provider;snapshot' {
            [CompletionResult]::new('--to', '--to', [CompletionResultType]::ParameterName, 'Write it to this new file instead of standard output')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;provider;recovery' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('export', 'export', [CompletionResultType]::ParameterValue, 'Write the recovery copy of an environment''s managed store (its unseal key, mmd''s logins and its certificate authority), sealed with age to you or to another operator, as ASCII armour')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Keep a recovery copy another machine exported to you, so setup from this machine can give a reinstalled node its keys')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;secret;provider;recovery;export' {
            [CompletionResult]::new('--recipient', '--recipient', [CompletionResultType]::ParameterName, 'Seal it to this age recipient (`age1…`) instead of to this machine''s identity')
            [CompletionResult]::new('--to', '--to', [CompletionResultType]::ParameterName, 'Write it to this new file instead of standard output')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;provider;recovery;import' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;provider;recovery;help' {
            [CompletionResult]::new('export', 'export', [CompletionResultType]::ParameterValue, 'Write the recovery copy of an environment''s managed store (its unseal key, mmd''s logins and its certificate authority), sealed with age to you or to another operator, as ASCII armour')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Keep a recovery copy another machine exported to you, so setup from this machine can give a reinstalled node its keys')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;secret;provider;recovery;help;export' {
            break
        }
        'mm;secret;provider;recovery;help;import' {
            break
        }
        'mm;secret;provider;recovery;help;help' {
            break
        }
        'mm;secret;provider;remove' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash printed by the corresponding remove plan')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed removal')
            [CompletionResult]::new('--purge', '--purge', [CompletionResultType]::ParameterName, 'Delete provider-owned persistent data')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;secret;provider;help' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List configured provider instances and registered drivers')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show one provider instance with its typed descriptor and capabilities')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Configure a declared provider instance')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Reconfigure a declared provider instance without changing its driver or mode')
            [CompletionResult]::new('test', 'test', [CompletionResultType]::ParameterValue, 'Test one external provider instance')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show health and capabilities for one provider instance')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Produce a deterministic provider lifecycle plan')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Apply an exact provider lifecycle plan')
            [CompletionResult]::new('backup', 'backup', [CompletionResultType]::ParameterValue, 'Write an encrypted provider snapshot after exact review')
            [CompletionResult]::new('restore', 'restore', [CompletionResultType]::ParameterValue, 'Restore an encrypted provider snapshot after exact review. For the store mm runs on a cluster''s nodes: put a snapshot from `snapshot` back on every node; without --plan it shows the plan')
            [CompletionResult]::new('snapshot', 'snapshot', [CompletionResultType]::ParameterValue, 'Take a snapshot of the store mm runs on a cluster environment''s nodes: its data and the keys it opens with, sealed to you')
            [CompletionResult]::new('recovery', 'recovery', [CompletionResultType]::ParameterValue, 'The recovery copy of the store mm runs on a cluster environment''s nodes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a provider through its exact reviewed lifecycle plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;secret;provider;help;list' {
            break
        }
        'mm;secret;provider;help;show' {
            break
        }
        'mm;secret;provider;help;add' {
            break
        }
        'mm;secret;provider;help;edit' {
            break
        }
        'mm;secret;provider;help;test' {
            break
        }
        'mm;secret;provider;help;status' {
            break
        }
        'mm;secret;provider;help;plan' {
            break
        }
        'mm;secret;provider;help;apply' {
            break
        }
        'mm;secret;provider;help;backup' {
            break
        }
        'mm;secret;provider;help;restore' {
            break
        }
        'mm;secret;provider;help;snapshot' {
            break
        }
        'mm;secret;provider;help;recovery' {
            [CompletionResult]::new('export', 'export', [CompletionResultType]::ParameterValue, 'Write the recovery copy of an environment''s managed store (its unseal key, mmd''s logins and its certificate authority), sealed with age to you or to another operator, as ASCII armour')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Keep a recovery copy another machine exported to you, so setup from this machine can give a reinstalled node its keys')
            break
        }
        'mm;secret;provider;help;recovery;export' {
            break
        }
        'mm;secret;provider;help;recovery;import' {
            break
        }
        'mm;secret;provider;help;remove' {
            break
        }
        'mm;secret;provider;help;help' {
            break
        }
        'mm;secret;help' {
            [CompletionResult]::new('providers', 'providers', [CompletionResultType]::ParameterValue, 'List available provider capabilities')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Initialize a secret provider for this project')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show secret set/unset state without revealing values. On a remote environment: what each host holds and has staged')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set a configured secret using a hidden prompt or stdin. On a remote environment the value is staged on every host for the next deployment')
            [CompletionResult]::new('unset', 'unset', [CompletionResultType]::ParameterValue, 'Remove a configured secret. On a remote environment the removal is staged for the next deployment')
            [CompletionResult]::new('discard', 'discard', [CompletionResultType]::ParameterValue, 'Drop a change staged on a remote environment''s hosts before it is deployed')
            [CompletionResult]::new('generate', 'generate', [CompletionResultType]::ParameterValue, 'Create configured generated secrets without overwriting existing values')
            [CompletionResult]::new('rotate', 'rotate', [CompletionResultType]::ParameterValue, 'Explicitly replace one secret using its configured source')
            [CompletionResult]::new('migrate', 'migrate', [CompletionResultType]::ParameterValue, 'Move a cluster environment''s secrets from the age store into the store mm runs on its nodes. Each becomes a version there with the revision it has now, so pointing the secrets at the store afterwards replaces no container. Safe to run again')
            [CompletionResult]::new('rotation', 'rotation', [CompletionResultType]::ParameterValue, 'Plan or apply scheduled rotations')
            [CompletionResult]::new('provider', 'provider', [CompletionResultType]::ParameterValue, 'Manage OpenBao and Vault provider instances')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;secret;help;providers' {
            break
        }
        'mm;secret;help;init' {
            break
        }
        'mm;secret;help;status' {
            break
        }
        'mm;secret;help;set' {
            break
        }
        'mm;secret;help;unset' {
            break
        }
        'mm;secret;help;discard' {
            break
        }
        'mm;secret;help;generate' {
            break
        }
        'mm;secret;help;rotate' {
            break
        }
        'mm;secret;help;migrate' {
            break
        }
        'mm;secret;help;rotation' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Inspect metadata and produce an approval hash without changing secrets')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Apply the exact reviewed plan using revision-guarded writes')
            break
        }
        'mm;secret;help;rotation;plan' {
            break
        }
        'mm;secret;help;rotation;apply' {
            break
        }
        'mm;secret;help;provider' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List configured provider instances and registered drivers')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show one provider instance with its typed descriptor and capabilities')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Configure a declared provider instance')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Reconfigure a declared provider instance without changing its driver or mode')
            [CompletionResult]::new('test', 'test', [CompletionResultType]::ParameterValue, 'Test one external provider instance')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show health and capabilities for one provider instance')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Produce a deterministic provider lifecycle plan')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Apply an exact provider lifecycle plan')
            [CompletionResult]::new('backup', 'backup', [CompletionResultType]::ParameterValue, 'Write an encrypted provider snapshot after exact review')
            [CompletionResult]::new('restore', 'restore', [CompletionResultType]::ParameterValue, 'Restore an encrypted provider snapshot after exact review. For the store mm runs on a cluster''s nodes: put a snapshot from `snapshot` back on every node; without --plan it shows the plan')
            [CompletionResult]::new('snapshot', 'snapshot', [CompletionResultType]::ParameterValue, 'Take a snapshot of the store mm runs on a cluster environment''s nodes: its data and the keys it opens with, sealed to you')
            [CompletionResult]::new('recovery', 'recovery', [CompletionResultType]::ParameterValue, 'The recovery copy of the store mm runs on a cluster environment''s nodes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a provider through its exact reviewed lifecycle plan')
            break
        }
        'mm;secret;help;provider;list' {
            break
        }
        'mm;secret;help;provider;show' {
            break
        }
        'mm;secret;help;provider;add' {
            break
        }
        'mm;secret;help;provider;edit' {
            break
        }
        'mm;secret;help;provider;test' {
            break
        }
        'mm;secret;help;provider;status' {
            break
        }
        'mm;secret;help;provider;plan' {
            break
        }
        'mm;secret;help;provider;apply' {
            break
        }
        'mm;secret;help;provider;backup' {
            break
        }
        'mm;secret;help;provider;restore' {
            break
        }
        'mm;secret;help;provider;snapshot' {
            break
        }
        'mm;secret;help;provider;recovery' {
            [CompletionResult]::new('export', 'export', [CompletionResultType]::ParameterValue, 'Write the recovery copy of an environment''s managed store (its unseal key, mmd''s logins and its certificate authority), sealed with age to you or to another operator, as ASCII armour')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Keep a recovery copy another machine exported to you, so setup from this machine can give a reinstalled node its keys')
            break
        }
        'mm;secret;help;provider;recovery;export' {
            break
        }
        'mm;secret;help;provider;recovery;import' {
            break
        }
        'mm;secret;help;provider;remove' {
            break
        }
        'mm;secret;help;help' {
            break
        }
        'mm;access' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Review or create a password-cap policy using the managed local Cap service')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Review or remove an unattached policy and its owned secret material')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show redacted policy metadata')
            [CompletionResult]::new('attach', 'attach', [CompletionResultType]::ParameterValue, 'Review or attach one policy middleware to an owned router')
            [CompletionResult]::new('detach', 'detach', [CompletionResultType]::ParameterValue, 'Review or detach one policy middleware from an owned router')
            [CompletionResult]::new('user', 'user', [CompletionResultType]::ParameterValue, 'Manage application-access principals (separate from deployment RBAC users)')
            [CompletionResult]::new('session', 'session', [CompletionResultType]::ParameterValue, 'List, revoke, or rotate opaque sessions')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;access;add' {
            [CompletionResult]::new('--mode', '--mode', [CompletionResultType]::ParameterName, 'Driver evaluation mode for this policy')
            [CompletionResult]::new('--session-days', '--session-days', [CompletionResultType]::ParameterName, 'Session lifetime in days (default seven, maximum thirty)')
            [CompletionResult]::new('--secret-provider', '--secret-provider', [CompletionResultType]::ParameterName, 'Secret storage provider for generated gate keys and pepper material')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by the read-only policy review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed policy plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;access;remove' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by the read-only removal review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed removal plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;access;show' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;access;attach' {
            [CompletionResult]::new('--target-id', '--target-id', [CompletionResultType]::ParameterName, 'Target that owns the ingress router')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by the read-only route review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed route plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;access;detach' {
            [CompletionResult]::new('--target-id', '--target-id', [CompletionResultType]::ParameterName, 'Target that owns the ingress router')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by the read-only route review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed route plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;access;user' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Review or create an application-access user with a hidden password')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Review or disable an application-access user and revoke active sessions')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Review or re-enable an application-access user')
            [CompletionResult]::new('password-reset', 'password-reset', [CompletionResultType]::ParameterValue, 'Review or replace an application-access user''s hidden password')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Review or remove an application-access user and revoke active sessions')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'Show redacted application-user metadata for one policy')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;access;user;add' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by the read-only user review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--stdin', '--stdin', [CompletionResultType]::ParameterName, 'Read the password from standard input instead of prompting')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed user plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;access;user;disable' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by the read-only user review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed user plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;access;user;enable' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by the read-only user review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed user plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;access;user;password-reset' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by the read-only user review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--stdin', '--stdin', [CompletionResultType]::ParameterName, 'Read the password from standard input instead of prompting')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed user plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;access;user;remove' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by the read-only user review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed user plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;access;user;list' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;access;user;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Review or create an application-access user with a hidden password')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Review or disable an application-access user and revoke active sessions')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Review or re-enable an application-access user')
            [CompletionResult]::new('password-reset', 'password-reset', [CompletionResultType]::ParameterValue, 'Review or replace an application-access user''s hidden password')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Review or remove an application-access user and revoke active sessions')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'Show redacted application-user metadata for one policy')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;access;user;help;add' {
            break
        }
        'mm;access;user;help;disable' {
            break
        }
        'mm;access;user;help;enable' {
            break
        }
        'mm;access;user;help;password-reset' {
            break
        }
        'mm;access;user;help;remove' {
            break
        }
        'mm;access;user;help;list' {
            break
        }
        'mm;access;user;help;help' {
            break
        }
        'mm;access;session' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'Show redacted active-session metadata for one policy')
            [CompletionResult]::new('revoke', 'revoke', [CompletionResultType]::ParameterValue, 'Review or revoke one active session')
            [CompletionResult]::new('rotate-key', 'rotate-key', [CompletionResultType]::ParameterValue, 'Review or rotate a policy''s session encryption key')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;access;session;list' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;access;session;revoke' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by the read-only revocation review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed revocation plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;access;session;rotate-key' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by the read-only key-rotation review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed key-rotation plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;access;session;help' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'Show redacted active-session metadata for one policy')
            [CompletionResult]::new('revoke', 'revoke', [CompletionResultType]::ParameterValue, 'Review or revoke one active session')
            [CompletionResult]::new('rotate-key', 'rotate-key', [CompletionResultType]::ParameterValue, 'Review or rotate a policy''s session encryption key')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;access;session;help;list' {
            break
        }
        'mm;access;session;help;revoke' {
            break
        }
        'mm;access;session;help;rotate-key' {
            break
        }
        'mm;access;session;help;help' {
            break
        }
        'mm;access;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Review or create a password-cap policy using the managed local Cap service')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Review or remove an unattached policy and its owned secret material')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show redacted policy metadata')
            [CompletionResult]::new('attach', 'attach', [CompletionResultType]::ParameterValue, 'Review or attach one policy middleware to an owned router')
            [CompletionResult]::new('detach', 'detach', [CompletionResultType]::ParameterValue, 'Review or detach one policy middleware from an owned router')
            [CompletionResult]::new('user', 'user', [CompletionResultType]::ParameterValue, 'Manage application-access principals (separate from deployment RBAC users)')
            [CompletionResult]::new('session', 'session', [CompletionResultType]::ParameterValue, 'List, revoke, or rotate opaque sessions')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;access;help;add' {
            break
        }
        'mm;access;help;remove' {
            break
        }
        'mm;access;help;show' {
            break
        }
        'mm;access;help;attach' {
            break
        }
        'mm;access;help;detach' {
            break
        }
        'mm;access;help;user' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Review or create an application-access user with a hidden password')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Review or disable an application-access user and revoke active sessions')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Review or re-enable an application-access user')
            [CompletionResult]::new('password-reset', 'password-reset', [CompletionResultType]::ParameterValue, 'Review or replace an application-access user''s hidden password')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Review or remove an application-access user and revoke active sessions')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'Show redacted application-user metadata for one policy')
            break
        }
        'mm;access;help;user;add' {
            break
        }
        'mm;access;help;user;disable' {
            break
        }
        'mm;access;help;user;enable' {
            break
        }
        'mm;access;help;user;password-reset' {
            break
        }
        'mm;access;help;user;remove' {
            break
        }
        'mm;access;help;user;list' {
            break
        }
        'mm;access;help;session' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'Show redacted active-session metadata for one policy')
            [CompletionResult]::new('revoke', 'revoke', [CompletionResultType]::ParameterValue, 'Review or revoke one active session')
            [CompletionResult]::new('rotate-key', 'rotate-key', [CompletionResultType]::ParameterValue, 'Review or rotate a policy''s session encryption key')
            break
        }
        'mm;access;help;session;list' {
            break
        }
        'mm;access;help;session;revoke' {
            break
        }
        'mm;access;help;session;rotate-key' {
            break
        }
        'mm;access;help;help' {
            break
        }
        'mm;build' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--publish', '--publish', [CompletionResultType]::ParameterName, 'Push a multi-platform image index and reviewed tags to the registry')
            [CompletionResult]::new('--all', '--all', [CompletionResultType]::ParameterName, 'Build every module that has a build, not only the ones the selected environment runs and the bases they build FROM')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the build confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Print the deterministic image build plan without running Docker')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Execute an exact plan hash printed by `mm build plan`')
            [CompletionResult]::new('receipt', 'receipt', [CompletionResultType]::ParameterValue, 'Write the latest published build of this checkout and environment to FILE, for `mm deploy plan --receipt` or `mm build import` on another machine')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Review or apply importing a build receipt from `mm build receipt`')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;build;plan' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--publish', '--publish', [CompletionResultType]::ParameterName, 'Push a multi-platform image index and reviewed tags to the registry')
            [CompletionResult]::new('--all', '--all', [CompletionResultType]::ParameterName, 'Build every module that has a build, not only the ones the selected environment runs and the bases they build FROM')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the build confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;build;apply' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash printed by `mm build plan`')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--publish', '--publish', [CompletionResultType]::ParameterName, 'Push a multi-platform image index and reviewed tags to the registry')
            [CompletionResult]::new('--all', '--all', [CompletionResultType]::ParameterName, 'Build every module that has a build, not only the ones the selected environment runs and the bases they build FROM')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the build confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;build;receipt' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--publish', '--publish', [CompletionResultType]::ParameterName, 'Push a multi-platform image index and reviewed tags to the registry')
            [CompletionResult]::new('--all', '--all', [CompletionResultType]::ParameterName, 'Build every module that has a build, not only the ones the selected environment runs and the bases they build FROM')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the build confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;build;import' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Apply the import with this hash, printed by the review')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--cache-only', '--cache-only', [CompletionResultType]::ParameterName, 'Only seed build reuse; never select deployment images')
            [CompletionResult]::new('--publish', '--publish', [CompletionResultType]::ParameterName, 'Push a multi-platform image index and reviewed tags to the registry')
            [CompletionResult]::new('--all', '--all', [CompletionResultType]::ParameterName, 'Build every module that has a build, not only the ones the selected environment runs and the bases they build FROM')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the build confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;build;help' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Print the deterministic image build plan without running Docker')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Execute an exact plan hash printed by `mm build plan`')
            [CompletionResult]::new('receipt', 'receipt', [CompletionResultType]::ParameterValue, 'Write the latest published build of this checkout and environment to FILE, for `mm deploy plan --receipt` or `mm build import` on another machine')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Review or apply importing a build receipt from `mm build receipt`')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;build;help;plan' {
            break
        }
        'mm;build;help;apply' {
            break
        }
        'mm;build;help;receipt' {
            break
        }
        'mm;build;help;import' {
            break
        }
        'mm;build;help;help' {
            break
        }
        'mm;deploy' {
            [CompletionResult]::new('--nodes', '--nodes', [CompletionResultType]::ParameterName, 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so')
            [CompletionResult]::new('--release', '--release', [CompletionResultType]::ParameterName, 'The release''s name, which templates read as MM.RELEASE. Defaults to sha- and the commit''s first 12 characters')
            [CompletionResult]::new('--release-input', '--release-input', [CompletionResultType]::ParameterName, 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the deployment confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Print the deterministic deployment plan without applying it')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Apply an exact plan hash printed by `mm deploy plan`')
            [CompletionResult]::new('rollback', 'rollback', [CompletionResultType]::ParameterValue, 'Plan a return to what a remote environment ran before, and apply it after review')
            [CompletionResult]::new('redeploy', 'redeploy', [CompletionResultType]::ParameterValue, 'Plan the commit a remote environment runs now again, and apply it after review, so that secrets and variables staged on its hosts go live')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show what each host of a remote environment last deployed, what it ran before, a rollout that is running or was left unfinished, and what runs on each host right now')
            [CompletionResult]::new('logs', 'logs', [CompletionResultType]::ParameterValue, 'Show the output of the jobs the last deployment ran, node by node')
            [CompletionResult]::new('cancel', 'cancel', [CompletionResultType]::ParameterValue, 'Stop a deployment or other operation that runs on a remote environment, end its processes on every host, and return the hosts it changed to what they ran before')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;deploy;plan' {
            [CompletionResult]::new('--commit', '--commit', [CompletionResultType]::ParameterName, 'Commit a remote environment deploys; the host fetches it from the project''s origin. Defaults to this checkout''s HEAD')
            [CompletionResult]::new('--receipt', '--receipt', [CompletionResultType]::ParameterName, 'Build receipt from `mm build receipt`: deploy its images, built from the deployed commit for this or another environment')
            [CompletionResult]::new('--nodes', '--nodes', [CompletionResultType]::ParameterName, 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so')
            [CompletionResult]::new('--release', '--release', [CompletionResultType]::ParameterName, 'The release''s name, which templates read as MM.RELEASE. Defaults to sha- and the commit''s first 12 characters')
            [CompletionResult]::new('--release-input', '--release-input', [CompletionResultType]::ParameterName, 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the deployment confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;deploy;apply' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash printed by `mm deploy plan`')
            [CompletionResult]::new('--receipt', '--receipt', [CompletionResultType]::ParameterName, 'The build receipt the local plan was made with. A remote plan already carries its receipt')
            [CompletionResult]::new('--nodes', '--nodes', [CompletionResultType]::ParameterName, 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so')
            [CompletionResult]::new('--release', '--release', [CompletionResultType]::ParameterName, 'The release''s name, which templates read as MM.RELEASE. Defaults to sha- and the commit''s first 12 characters')
            [CompletionResult]::new('--release-input', '--release-input', [CompletionResultType]::ParameterName, 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the deployment confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;deploy;rollback' {
            [CompletionResult]::new('--nodes', '--nodes', [CompletionResultType]::ParameterName, 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so')
            [CompletionResult]::new('--release', '--release', [CompletionResultType]::ParameterName, 'The release''s name, which templates read as MM.RELEASE. Defaults to sha- and the commit''s first 12 characters')
            [CompletionResult]::new('--release-input', '--release-input', [CompletionResultType]::ParameterName, 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the deployment confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;deploy;redeploy' {
            [CompletionResult]::new('--nodes', '--nodes', [CompletionResultType]::ParameterName, 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so')
            [CompletionResult]::new('--release', '--release', [CompletionResultType]::ParameterName, 'The release''s name, which templates read as MM.RELEASE. Defaults to sha- and the commit''s first 12 characters')
            [CompletionResult]::new('--release-input', '--release-input', [CompletionResultType]::ParameterName, 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the deployment confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;deploy;status' {
            [CompletionResult]::new('--nodes', '--nodes', [CompletionResultType]::ParameterName, 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so')
            [CompletionResult]::new('--release', '--release', [CompletionResultType]::ParameterName, 'The release''s name, which templates read as MM.RELEASE. Defaults to sha- and the commit''s first 12 characters')
            [CompletionResult]::new('--release-input', '--release-input', [CompletionResultType]::ParameterName, 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the deployment confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;deploy;logs' {
            [CompletionResult]::new('--nodes', '--nodes', [CompletionResultType]::ParameterName, 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so')
            [CompletionResult]::new('--release', '--release', [CompletionResultType]::ParameterName, 'The release''s name, which templates read as MM.RELEASE. Defaults to sha- and the commit''s first 12 characters')
            [CompletionResult]::new('--release-input', '--release-input', [CompletionResultType]::ParameterName, 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the deployment confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;deploy;cancel' {
            [CompletionResult]::new('--nodes', '--nodes', [CompletionResultType]::ParameterName, 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so')
            [CompletionResult]::new('--release', '--release', [CompletionResultType]::ParameterName, 'The release''s name, which templates read as MM.RELEASE. Defaults to sha- and the commit''s first 12 characters')
            [CompletionResult]::new('--release-input', '--release-input', [CompletionResultType]::ParameterName, 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the deployment confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;deploy;help' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Print the deterministic deployment plan without applying it')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Apply an exact plan hash printed by `mm deploy plan`')
            [CompletionResult]::new('rollback', 'rollback', [CompletionResultType]::ParameterValue, 'Plan a return to what a remote environment ran before, and apply it after review')
            [CompletionResult]::new('redeploy', 'redeploy', [CompletionResultType]::ParameterValue, 'Plan the commit a remote environment runs now again, and apply it after review, so that secrets and variables staged on its hosts go live')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show what each host of a remote environment last deployed, what it ran before, a rollout that is running or was left unfinished, and what runs on each host right now')
            [CompletionResult]::new('logs', 'logs', [CompletionResultType]::ParameterValue, 'Show the output of the jobs the last deployment ran, node by node')
            [CompletionResult]::new('cancel', 'cancel', [CompletionResultType]::ParameterValue, 'Stop a deployment or other operation that runs on a remote environment, end its processes on every host, and return the hosts it changed to what they ran before')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;deploy;help;plan' {
            break
        }
        'mm;deploy;help;apply' {
            break
        }
        'mm;deploy;help;rollback' {
            break
        }
        'mm;deploy;help;redeploy' {
            break
        }
        'mm;deploy;help;status' {
            break
        }
        'mm;deploy;help;logs' {
            break
        }
        'mm;deploy;help;cancel' {
            break
        }
        'mm;deploy;help;help' {
            break
        }
        'mm;team' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('join', 'join', [CompletionResultType]::ParameterValue, 'Add yourself to the users file with this device''s key (reviewed edit)')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add a user with a given public key (reviewed edit)')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a user from the users file (reviewed edit)')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show the users file: each user''s keys and access per environment')
            [CompletionResult]::new('whoami', 'whoami', [CompletionResultType]::ParameterValue, 'Show who the remote environment thinks you are and what you may do')
            [CompletionResult]::new('sync', 'sync', [CompletionResultType]::ParameterValue, 'Reconcile the selected remote environment''s access from a commit''s users file without deploying')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;team;join' {
            [CompletionResult]::new('--name', '--name', [CompletionResultType]::ParameterName, 'User name, [a-z][a-z0-9-]*. Defaults to $USER')
            [CompletionResult]::new('--request', '--request', [CompletionResultType]::ParameterName, 'Access to request, for example production=view. Repeatable')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review of the same change')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--agent', '--agent', [CompletionResultType]::ParameterName, 'Add this device''s agent key to your existing entry instead, marked `agent: true`. Create it first with `mm key create --agent`. After `mm key rotate --agent` it takes the place of the old agent key')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;team;add' {
            [CompletionResult]::new('--key', '--key', [CompletionResultType]::ParameterName, 'OpenSSH public key line, `ssh-ed25519 AAAA… comment`')
            [CompletionResult]::new('--access', '--access', [CompletionResultType]::ParameterName, 'Access per environment, for example staging=deploy or *=view. Repeatable')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review of the same change')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--machine', '--machine', [CompletionResultType]::ParameterName, 'A CI or automation identity rather than a person')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;team;remove' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review of the same change')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;team;show' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;team;whoami' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;team;sync' {
            [CompletionResult]::new('--commit', '--commit', [CompletionResultType]::ParameterName, 'Commit whose users file to apply; defaults to HEAD')
            [CompletionResult]::new('--nodes', '--nodes', [CompletionResultType]::ParameterName, 'On a target that lists nodes, plan only these nodes. The others keep the access they enforce now')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review of the same change')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;team;help' {
            [CompletionResult]::new('join', 'join', [CompletionResultType]::ParameterValue, 'Add yourself to the users file with this device''s key (reviewed edit)')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add a user with a given public key (reviewed edit)')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a user from the users file (reviewed edit)')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show the users file: each user''s keys and access per environment')
            [CompletionResult]::new('whoami', 'whoami', [CompletionResultType]::ParameterValue, 'Show who the remote environment thinks you are and what you may do')
            [CompletionResult]::new('sync', 'sync', [CompletionResultType]::ParameterValue, 'Reconcile the selected remote environment''s access from a commit''s users file without deploying')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;team;help;join' {
            break
        }
        'mm;team;help;add' {
            break
        }
        'mm;team;help;remove' {
            break
        }
        'mm;team;help;show' {
            break
        }
        'mm;team;help;whoami' {
            break
        }
        'mm;team;help;sync' {
            break
        }
        'mm;team;help;help' {
            break
        }
        'mm;forward' {
            [CompletionResult]::new('--local', '--local', [CompletionResultType]::ParameterName, 'Local port; defaults to the service port, or a free port if that is busy')
            [CompletionResult]::new('--bind', '--bind', [CompletionResultType]::ParameterName, 'Local address to listen on. Anything but loopback exposes the service to your network')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('save', 'save', [CompletionResultType]::ParameterValue, 'Save a forward for this project and environment, with a pinned local port')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List this project''s saved forwards')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a saved forward')
            [CompletionResult]::new('up', 'up', [CompletionResultType]::ParameterValue, 'Start every saved forward marked --auto')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;forward;save' {
            [CompletionResult]::new('--local', '--local', [CompletionResultType]::ParameterName, 'Local port to pin; defaults to the service port')
            [CompletionResult]::new('--bind', '--bind', [CompletionResultType]::ParameterName, 'Local address to listen on; defaults to 127.0.0.1')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--auto', '--auto', [CompletionResultType]::ParameterName, 'Start it with `mm forward up`')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;forward;list' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;forward;remove' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;forward;up' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;forward;help' {
            [CompletionResult]::new('save', 'save', [CompletionResultType]::ParameterValue, 'Save a forward for this project and environment, with a pinned local port')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List this project''s saved forwards')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a saved forward')
            [CompletionResult]::new('up', 'up', [CompletionResultType]::ParameterValue, 'Start every saved forward marked --auto')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;forward;help;save' {
            break
        }
        'mm;forward;help;list' {
            break
        }
        'mm;forward;help;remove' {
            break
        }
        'mm;forward;help;up' {
            break
        }
        'mm;forward;help;help' {
            break
        }
        'mm;key' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('create', 'create', [CompletionResultType]::ParameterValue, 'Create this device''s SSH key under MM_HOME, encrypted under a passphrase mm keeps in the system keychain. mm never uses ~/.ssh')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Print this device''s public key and fingerprint for the users file, and how the key is protected')
            [CompletionResult]::new('protect', 'protect', [CompletionResultType]::ParameterValue, 'Encrypt this device''s plain keys in place (reviewed). Public keys stay the same, so no host access changes')
            [CompletionResult]::new('rotate', 'rotate', [CompletionResultType]::ParameterValue, 'Replace a key with a new one (reviewed), without losing access: the new key is made beside the current one and mm offers both until you finish the rotation')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Delete a key and its passphrase from this device (reviewed)')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;key;create' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--security-key', '--security-key', [CompletionResultType]::ParameterName, 'Use a FIDO2 security key (sk-ssh-ed25519); touch it when asked')
            [CompletionResult]::new('--agent', '--agent', [CompletionResultType]::ParameterName, 'Create the key `mm mcp serve` uses instead: a separate key for your AI agent, limited on the host by the `agents:` block')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;key;show' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--agent', '--agent', [CompletionResultType]::ParameterName, 'Print the agent key instead')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;key;protect' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review of the same change')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;key;rotate' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review of the same change')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--agent', '--agent', [CompletionResultType]::ParameterName, 'Rotate the agent key instead')
            [CompletionResult]::new('--finish', '--finish', [CompletionResultType]::ParameterName, 'Put the new key in place of the current one and delete the old key. Hosts whose users file does not list the new key refuse this device from then on')
            [CompletionResult]::new('--unlisted', '--unlisted', [CompletionResultType]::ParameterName, 'Finish even though the users file here does not list the new key')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;key;remove' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review of the same change')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--agent', '--agent', [CompletionResultType]::ParameterName, 'Delete the agent key instead')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;key;help' {
            [CompletionResult]::new('create', 'create', [CompletionResultType]::ParameterValue, 'Create this device''s SSH key under MM_HOME, encrypted under a passphrase mm keeps in the system keychain. mm never uses ~/.ssh')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Print this device''s public key and fingerprint for the users file, and how the key is protected')
            [CompletionResult]::new('protect', 'protect', [CompletionResultType]::ParameterValue, 'Encrypt this device''s plain keys in place (reviewed). Public keys stay the same, so no host access changes')
            [CompletionResult]::new('rotate', 'rotate', [CompletionResultType]::ParameterValue, 'Replace a key with a new one (reviewed), without losing access: the new key is made beside the current one and mm offers both until you finish the rotation')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Delete a key and its passphrase from this device (reviewed)')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;key;help;create' {
            break
        }
        'mm;key;help;show' {
            break
        }
        'mm;key;help;protect' {
            break
        }
        'mm;key;help;rotate' {
            break
        }
        'mm;key;help;remove' {
            break
        }
        'mm;key;help;help' {
            break
        }
        'mm;target' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('local', 'local', [CompletionResultType]::ParameterValue, 'Register this machine as the local target, preserving existing targets')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List targets configured beneath MM_HOME')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show one target from the shared catalog')
            [CompletionResult]::new('nodes', 'nodes', [CompletionResultType]::ParameterValue, 'List every node of the selected environment''s target: whether it answers (and why not), its mmd version, its mesh address, and the commit and time of its last applied deployment')
            [CompletionResult]::new('mesh', 'mesh', [CompletionResultType]::ParameterValue, 'The WireGuard mesh between the nodes of the selected environment''s target: look at it, or replace one node''s key')
            [CompletionResult]::new('storage', 'storage', [CompletionResultType]::ParameterValue, 'The shared volumes of the selected environment''s target: the node that keeps them, their sizes, and which nodes mount them; or delete one volume''s files')
            [CompletionResult]::new('connections', 'connections', [CompletionResultType]::ParameterValue, 'List the SSH connections this device keeps open to remote hosts between commands, or close them')
            [CompletionResult]::new('trust', 'trust', [CompletionResultType]::ParameterValue, 'Review a repo-declared target''s host key and daemon identity, and accept a change with --yes')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add an SSH target to the shared catalog')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'Update an SSH target without changing its stable identity')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a target only if its reviewed fingerprint still matches')
            [CompletionResult]::new('select', 'select', [CompletionResultType]::ParameterValue, 'Select a private target preference for this project')
            [CompletionResult]::new('preference', 'preference', [CompletionResultType]::ParameterValue, 'Show or change private runtime and target preferences')
            [CompletionResult]::new('setup', 'setup', [CompletionResultType]::ParameterValue, 'Plan or apply target provisioning and project enrollment')
            [CompletionResult]::new('ingress', 'ingress', [CompletionResultType]::ParameterValue, 'Configure and plan the target-owned ingress stack')
            [CompletionResult]::new('access', 'access', [CompletionResultType]::ParameterValue, 'Administer remote developer access on a target')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;target;local' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;list' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;show' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;nodes' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;mesh' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show every node''s mesh interface and, per peer, its endpoint, last handshake, and traffic. Lists what is wrong with what to do about it, such as the UDP port to open between two hosts. Exits 3 when a node does not answer or the mesh has a problem')
            [CompletionResult]::new('rotate', 'rotate', [CompletionResultType]::ParameterValue, 'Replace one node''s WireGuard key pair. Without --plan this prints the plan: the node makes a new key and every other node replaces its one entry for it. Nothing else changes. The private key never leaves the node')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;target;mesh;status' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;mesh;rotate' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Apply the plan with this hash, as printed without --plan')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;mesh;help' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show every node''s mesh interface and, per peer, its endpoint, last handshake, and traffic. Lists what is wrong with what to do about it, such as the UDP port to open between two hosts. Exits 3 when a node does not answer or the mesh has a problem')
            [CompletionResult]::new('rotate', 'rotate', [CompletionResultType]::ParameterValue, 'Replace one node''s WireGuard key pair. Without --plan this prints the plan: the node makes a new key and every other node replaces its one entry for it. Nothing else changes. The private key never leaves the node')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;target;mesh;help;status' {
            break
        }
        'mm;target;mesh;help;rotate' {
            break
        }
        'mm;target;mesh;help;help' {
            break
        }
        'mm;target;storage' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show the node that keeps shared volumes, each volume with its size and the nodes that have it mounted, and whether every node reaches the storage node. Also lists volumes that are no longer in the configuration and still have files. Exits 3 when a node does not answer or cannot reach the storage node')
            [CompletionResult]::new('delete', 'delete', [CompletionResultType]::ParameterValue, 'Delete a shared volume''s files from the storage node. Without --plan this prints what would be removed. Refused while a container on any node uses the volume. This is the only command that removes shared data: taking a volume out of `shared-volumes` keeps it')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;target;storage;status' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;storage;delete' {
            [CompletionResult]::new('--stack', '--stack', [CompletionResultType]::ParameterName, 'The stack the volume belongs to, when more than one stack keeps a volume of this name')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Delete what the plan with this hash showed, as printed without --plan')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the confirmation prompt')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;storage;help' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show the node that keeps shared volumes, each volume with its size and the nodes that have it mounted, and whether every node reaches the storage node. Also lists volumes that are no longer in the configuration and still have files. Exits 3 when a node does not answer or cannot reach the storage node')
            [CompletionResult]::new('delete', 'delete', [CompletionResultType]::ParameterValue, 'Delete a shared volume''s files from the storage node. Without --plan this prints what would be removed. Refused while a container on any node uses the volume. This is the only command that removes shared data: taking a volume out of `shared-volumes` keeps it')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;target;storage;help;status' {
            break
        }
        'mm;target;storage;help;delete' {
            break
        }
        'mm;target;storage;help;help' {
            break
        }
        'mm;target;connections' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--close', '--close', [CompletionResultType]::ParameterName, 'Close every kept connection')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;trust' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Accept the repo''s current keys after reviewing them')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;add' {
            [CompletionResult]::new('--host', '--host', [CompletionResultType]::ParameterName, 'SSH hostname or address')
            [CompletionResult]::new('--user', '--user', [CompletionResultType]::ParameterName, 'SSH user')
            [CompletionResult]::new('--port', '--port', [CompletionResultType]::ParameterName, 'SSH port')
            [CompletionResult]::new('--host-key-fingerprint', '--host-key-fingerprint', [CompletionResultType]::ParameterName, 'OpenSSH SHA256 fingerprint accepted for this host')
            [CompletionResult]::new('--host-key', '--host-key', [CompletionResultType]::ParameterName, 'Full reviewed OpenSSH host public key used for strict known-hosts verification')
            [CompletionResult]::new('--identity-file', '--identity-file', [CompletionResultType]::ParameterName, 'Absolute private-key path used for public-key client authentication')
            [CompletionResult]::new('--remote-root', '--remote-root', [CompletionResultType]::ParameterName, 'Absolute project checkout path on the remote host')
            [CompletionResult]::new('--daemon-fingerprint', '--daemon-fingerprint', [CompletionResultType]::ParameterName, 'BLAKE3 daemon identity printed by `mmd --print-identity`')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;update' {
            [CompletionResult]::new('--host', '--host', [CompletionResultType]::ParameterName, 'SSH hostname or address')
            [CompletionResult]::new('--user', '--user', [CompletionResultType]::ParameterName, 'SSH user')
            [CompletionResult]::new('--port', '--port', [CompletionResultType]::ParameterName, 'SSH port')
            [CompletionResult]::new('--host-key-fingerprint', '--host-key-fingerprint', [CompletionResultType]::ParameterName, 'OpenSSH SHA256 fingerprint accepted for this host')
            [CompletionResult]::new('--host-key', '--host-key', [CompletionResultType]::ParameterName, 'Full reviewed OpenSSH host public key used for strict known-hosts verification')
            [CompletionResult]::new('--identity-file', '--identity-file', [CompletionResultType]::ParameterName, 'Absolute private-key path used for public-key client authentication')
            [CompletionResult]::new('--remote-root', '--remote-root', [CompletionResultType]::ParameterName, 'Absolute project checkout path on the remote host')
            [CompletionResult]::new('--daemon-fingerprint', '--daemon-fingerprint', [CompletionResultType]::ParameterName, 'BLAKE3 daemon identity printed by `mmd --print-identity`')
            [CompletionResult]::new('--expected-fingerprint', '--expected-fingerprint', [CompletionResultType]::ParameterName, 'Fingerprint from the reviewed target list')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;remove' {
            [CompletionResult]::new('--expected-fingerprint', '--expected-fingerprint', [CompletionResultType]::ParameterName, 'Fingerprint from `mm target show`')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Confirm catalog mutation')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;select' {
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Limit the preference to one environment')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;preference' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show resolved private runtime and target preferences')
            [CompletionResult]::new('set-runtime', 'set-runtime', [CompletionResultType]::ParameterValue, 'Persist a private runtime preference')
            [CompletionResult]::new('unset-runtime', 'unset-runtime', [CompletionResultType]::ParameterValue, 'Remove a private runtime preference')
            [CompletionResult]::new('set-target', 'set-target', [CompletionResultType]::ParameterValue, 'Persist a private target preference')
            [CompletionResult]::new('unset-target', 'unset-target', [CompletionResultType]::ParameterValue, 'Remove a private target preference')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;target;preference;show' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;preference;set-runtime' {
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Limit the preference to one environment')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;preference;unset-runtime' {
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Limit the unset to one environment')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;preference;set-target' {
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Limit the preference to one environment')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;preference;unset-target' {
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Limit the unset to one environment')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;preference;help' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show resolved private runtime and target preferences')
            [CompletionResult]::new('set-runtime', 'set-runtime', [CompletionResultType]::ParameterValue, 'Persist a private runtime preference')
            [CompletionResult]::new('unset-runtime', 'unset-runtime', [CompletionResultType]::ParameterValue, 'Remove a private runtime preference')
            [CompletionResult]::new('set-target', 'set-target', [CompletionResultType]::ParameterValue, 'Persist a private target preference')
            [CompletionResult]::new('unset-target', 'unset-target', [CompletionResultType]::ParameterValue, 'Remove a private target preference')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;target;preference;help;show' {
            break
        }
        'mm;target;preference;help;set-runtime' {
            break
        }
        'mm;target;preference;help;unset-runtime' {
            break
        }
        'mm;target;preference;help;set-target' {
            break
        }
        'mm;target;preference;help;unset-target' {
            break
        }
        'mm;target;preference;help;help' {
            break
        }
        'mm;target;setup' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Produce a deterministic target setup plan. For a cluster target (`nodes:`), the plan also scans each node''s SSH host key')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Execute an exact plan hash printed by `mm target setup plan`')
            [CompletionResult]::new('deploy-key', 'deploy-key', [CompletionResultType]::ParameterValue, 'Give the environment its cluster target''s read-only git deploy key, stored in the secret `setup.deploy-key.secret` names, and print its public half. A target has one key: it is copied from another environment of the target that holds it, and created only when none does. Prints the existing key when there is one')
            [CompletionResult]::new('record', 'record', [CompletionResultType]::ParameterValue, 'Write each node''s SSH host key, daemon identity, and private address from the last cluster setup into `.config/mm/targets.yaml`')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;target;setup;plan' {
            [CompletionResult]::new('--origin', '--origin', [CompletionResultType]::ParameterName, 'Git URL the host fetches the project from with its own read-only deploy key. Turns on the remote access layer')
            [CompletionResult]::new('--delivery', '--delivery', [CompletionResultType]::ParameterName, '`push`: the host fetches nothing and mm sends it each commit''s files, so it needs no origin and no deploy key. Turns on the remote access layer, like --origin, and takes its place')
            [CompletionResult]::new('--origin-host-key', '--origin-host-key', [CompletionResultType]::ParameterName, 'The git server''s SSH host key, `<type> <base64>`, for SSH origins')
            [CompletionResult]::new('--break-glass', '--break-glass', [CompletionResultType]::ParameterName, 'A recovery key for the host policy, NAME=''ssh-ed25519 AAAA… comment''. Repeat for more. Keep the private half offline')
            [CompletionResult]::new('--host-binaries', '--host-binaries', [CompletionResultType]::ParameterName, 'Directory with `mm` and `mmd` built for the host, such as an extracted Linux release. Default: for each host''s CPU, the directory of this `mm` when it holds them, otherwise this mm''s own release, downloaded, checked, and kept under the mm home')
            [CompletionResult]::new('--commit', '--commit', [CompletionResultType]::ParameterName, 'Commit every node syncs the users file from (default: HEAD)')
            [CompletionResult]::new('--nodes', '--nodes', [CompletionResultType]::ParameterName, 'Set up only these nodes, for example while another one cannot be reached. The others are not contacted and keep the version they run')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--trust-new-host-keys', '--trust-new-host-keys', [CompletionResultType]::ParameterName, 'Accept SSH host keys that differ from the ones the repo records, such as after rebuilding a node. The plan shows each key''s fingerprint')
            [CompletionResult]::new('--downgrade', '--downgrade', [CompletionResultType]::ParameterName, 'Install this release on hosts that run a newer one')
            [CompletionResult]::new('--upgrade', '--upgrade', [CompletionResultType]::ParameterName, 'Install this mm and mmd on hosts that run another build of it or an older release. Without it, setup refuses to replace what the hosts run, unless deploy.mm-version pins this release')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;setup;apply' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Hash printed by `mm target setup plan`')
            [CompletionResult]::new('--origin', '--origin', [CompletionResultType]::ParameterName, 'Git URL the host fetches the project from with its own read-only deploy key. Turns on the remote access layer')
            [CompletionResult]::new('--delivery', '--delivery', [CompletionResultType]::ParameterName, '`push`: the host fetches nothing and mm sends it each commit''s files, so it needs no origin and no deploy key. Turns on the remote access layer, like --origin, and takes its place')
            [CompletionResult]::new('--origin-host-key', '--origin-host-key', [CompletionResultType]::ParameterName, 'The git server''s SSH host key, `<type> <base64>`, for SSH origins')
            [CompletionResult]::new('--break-glass', '--break-glass', [CompletionResultType]::ParameterName, 'A recovery key for the host policy, NAME=''ssh-ed25519 AAAA… comment''. Repeat for more. Keep the private half offline')
            [CompletionResult]::new('--host-binaries', '--host-binaries', [CompletionResultType]::ParameterName, 'Directory with `mm` and `mmd` built for the host, such as an extracted Linux release. Default: for each host''s CPU, the directory of this `mm` when it holds them, otherwise this mm''s own release, downloaded, checked, and kept under the mm home')
            [CompletionResult]::new('--commit', '--commit', [CompletionResultType]::ParameterName, 'Commit every node syncs the users file from (default: HEAD)')
            [CompletionResult]::new('--nodes', '--nodes', [CompletionResultType]::ParameterName, 'Set up only these nodes, for example while another one cannot be reached. The others are not contacted and keep the version they run')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the bootstrap confirmation prompt')
            [CompletionResult]::new('--trust-new-host-keys', '--trust-new-host-keys', [CompletionResultType]::ParameterName, 'Accept SSH host keys that differ from the ones the repo records, such as after rebuilding a node. The plan shows each key''s fingerprint')
            [CompletionResult]::new('--downgrade', '--downgrade', [CompletionResultType]::ParameterName, 'Install this release on hosts that run a newer one')
            [CompletionResult]::new('--upgrade', '--upgrade', [CompletionResultType]::ParameterName, 'Install this mm and mmd on hosts that run another build of it or an older release. Without it, setup refuses to replace what the hosts run, unless deploy.mm-version pins this release')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;setup;deploy-key' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;setup;record' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;setup;help' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Produce a deterministic target setup plan. For a cluster target (`nodes:`), the plan also scans each node''s SSH host key')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Execute an exact plan hash printed by `mm target setup plan`')
            [CompletionResult]::new('deploy-key', 'deploy-key', [CompletionResultType]::ParameterValue, 'Give the environment its cluster target''s read-only git deploy key, stored in the secret `setup.deploy-key.secret` names, and print its public half. A target has one key: it is copied from another environment of the target that holds it, and created only when none does. Prints the existing key when there is one')
            [CompletionResult]::new('record', 'record', [CompletionResultType]::ParameterValue, 'Write each node''s SSH host key, daemon identity, and private address from the last cluster setup into `.config/mm/targets.yaml`')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;target;setup;help;plan' {
            break
        }
        'mm;target;setup;help;apply' {
            break
        }
        'mm;target;setup;help;deploy-key' {
            break
        }
        'mm;target;setup;help;record' {
            break
        }
        'mm;target;setup;help;help' {
            break
        }
        'mm;target;ingress' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('configure', 'configure', [CompletionResultType]::ParameterValue, 'Configure digest-pinned Traefik and optional ACME metadata')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Publish a deterministic target-owned ingress artifact and plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;target;ingress;configure' {
            [CompletionResult]::new('--image', '--image', [CompletionResultType]::ParameterName, 'Digest-pinned Traefik image reference')
            [CompletionResult]::new('--acme-email', '--acme-email', [CompletionResultType]::ParameterName, 'Optional ACME account email')
            [CompletionResult]::new('--acme-ca-server', '--acme-ca-server', [CompletionResultType]::ParameterName, 'ACME directory URL, such as Let''s Encrypt staging or an internal ACME server')
            [CompletionResult]::new('--acme-ca-certificate', '--acme-ca-certificate', [CompletionResultType]::ParameterName, 'PEM file of the CA that signs the ACME server''s HTTPS certificate')
            [CompletionResult]::new('--expected-fingerprint', '--expected-fingerprint', [CompletionResultType]::ParameterName, 'Fingerprint from the reviewed target list')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;ingress;plan' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;ingress;help' {
            [CompletionResult]::new('configure', 'configure', [CompletionResultType]::ParameterValue, 'Configure digest-pinned Traefik and optional ACME metadata')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Publish a deterministic target-owned ingress artifact and plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;target;ingress;help;configure' {
            break
        }
        'mm;target;ingress;help;plan' {
            break
        }
        'mm;target;ingress;help;help' {
            break
        }
        'mm;target;access' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Explain the privileged remote-access administration boundary')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;target;access;status' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;target;access;help' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Explain the privileged remote-access administration boundary')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;target;access;help;status' {
            break
        }
        'mm;target;access;help;help' {
            break
        }
        'mm;target;help' {
            [CompletionResult]::new('local', 'local', [CompletionResultType]::ParameterValue, 'Register this machine as the local target, preserving existing targets')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List targets configured beneath MM_HOME')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show one target from the shared catalog')
            [CompletionResult]::new('nodes', 'nodes', [CompletionResultType]::ParameterValue, 'List every node of the selected environment''s target: whether it answers (and why not), its mmd version, its mesh address, and the commit and time of its last applied deployment')
            [CompletionResult]::new('mesh', 'mesh', [CompletionResultType]::ParameterValue, 'The WireGuard mesh between the nodes of the selected environment''s target: look at it, or replace one node''s key')
            [CompletionResult]::new('storage', 'storage', [CompletionResultType]::ParameterValue, 'The shared volumes of the selected environment''s target: the node that keeps them, their sizes, and which nodes mount them; or delete one volume''s files')
            [CompletionResult]::new('connections', 'connections', [CompletionResultType]::ParameterValue, 'List the SSH connections this device keeps open to remote hosts between commands, or close them')
            [CompletionResult]::new('trust', 'trust', [CompletionResultType]::ParameterValue, 'Review a repo-declared target''s host key and daemon identity, and accept a change with --yes')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add an SSH target to the shared catalog')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'Update an SSH target without changing its stable identity')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a target only if its reviewed fingerprint still matches')
            [CompletionResult]::new('select', 'select', [CompletionResultType]::ParameterValue, 'Select a private target preference for this project')
            [CompletionResult]::new('preference', 'preference', [CompletionResultType]::ParameterValue, 'Show or change private runtime and target preferences')
            [CompletionResult]::new('setup', 'setup', [CompletionResultType]::ParameterValue, 'Plan or apply target provisioning and project enrollment')
            [CompletionResult]::new('ingress', 'ingress', [CompletionResultType]::ParameterValue, 'Configure and plan the target-owned ingress stack')
            [CompletionResult]::new('access', 'access', [CompletionResultType]::ParameterValue, 'Administer remote developer access on a target')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;target;help;local' {
            break
        }
        'mm;target;help;list' {
            break
        }
        'mm;target;help;show' {
            break
        }
        'mm;target;help;nodes' {
            break
        }
        'mm;target;help;mesh' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show every node''s mesh interface and, per peer, its endpoint, last handshake, and traffic. Lists what is wrong with what to do about it, such as the UDP port to open between two hosts. Exits 3 when a node does not answer or the mesh has a problem')
            [CompletionResult]::new('rotate', 'rotate', [CompletionResultType]::ParameterValue, 'Replace one node''s WireGuard key pair. Without --plan this prints the plan: the node makes a new key and every other node replaces its one entry for it. Nothing else changes. The private key never leaves the node')
            break
        }
        'mm;target;help;mesh;status' {
            break
        }
        'mm;target;help;mesh;rotate' {
            break
        }
        'mm;target;help;storage' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show the node that keeps shared volumes, each volume with its size and the nodes that have it mounted, and whether every node reaches the storage node. Also lists volumes that are no longer in the configuration and still have files. Exits 3 when a node does not answer or cannot reach the storage node')
            [CompletionResult]::new('delete', 'delete', [CompletionResultType]::ParameterValue, 'Delete a shared volume''s files from the storage node. Without --plan this prints what would be removed. Refused while a container on any node uses the volume. This is the only command that removes shared data: taking a volume out of `shared-volumes` keeps it')
            break
        }
        'mm;target;help;storage;status' {
            break
        }
        'mm;target;help;storage;delete' {
            break
        }
        'mm;target;help;connections' {
            break
        }
        'mm;target;help;trust' {
            break
        }
        'mm;target;help;add' {
            break
        }
        'mm;target;help;update' {
            break
        }
        'mm;target;help;remove' {
            break
        }
        'mm;target;help;select' {
            break
        }
        'mm;target;help;preference' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show resolved private runtime and target preferences')
            [CompletionResult]::new('set-runtime', 'set-runtime', [CompletionResultType]::ParameterValue, 'Persist a private runtime preference')
            [CompletionResult]::new('unset-runtime', 'unset-runtime', [CompletionResultType]::ParameterValue, 'Remove a private runtime preference')
            [CompletionResult]::new('set-target', 'set-target', [CompletionResultType]::ParameterValue, 'Persist a private target preference')
            [CompletionResult]::new('unset-target', 'unset-target', [CompletionResultType]::ParameterValue, 'Remove a private target preference')
            break
        }
        'mm;target;help;preference;show' {
            break
        }
        'mm;target;help;preference;set-runtime' {
            break
        }
        'mm;target;help;preference;unset-runtime' {
            break
        }
        'mm;target;help;preference;set-target' {
            break
        }
        'mm;target;help;preference;unset-target' {
            break
        }
        'mm;target;help;setup' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Produce a deterministic target setup plan. For a cluster target (`nodes:`), the plan also scans each node''s SSH host key')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Execute an exact plan hash printed by `mm target setup plan`')
            [CompletionResult]::new('deploy-key', 'deploy-key', [CompletionResultType]::ParameterValue, 'Give the environment its cluster target''s read-only git deploy key, stored in the secret `setup.deploy-key.secret` names, and print its public half. A target has one key: it is copied from another environment of the target that holds it, and created only when none does. Prints the existing key when there is one')
            [CompletionResult]::new('record', 'record', [CompletionResultType]::ParameterValue, 'Write each node''s SSH host key, daemon identity, and private address from the last cluster setup into `.config/mm/targets.yaml`')
            break
        }
        'mm;target;help;setup;plan' {
            break
        }
        'mm;target;help;setup;apply' {
            break
        }
        'mm;target;help;setup;deploy-key' {
            break
        }
        'mm;target;help;setup;record' {
            break
        }
        'mm;target;help;ingress' {
            [CompletionResult]::new('configure', 'configure', [CompletionResultType]::ParameterValue, 'Configure digest-pinned Traefik and optional ACME metadata')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Publish a deterministic target-owned ingress artifact and plan')
            break
        }
        'mm;target;help;ingress;configure' {
            break
        }
        'mm;target;help;ingress;plan' {
            break
        }
        'mm;target;help;access' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Explain the privileged remote-access administration boundary')
            break
        }
        'mm;target;help;access;status' {
            break
        }
        'mm;target;help;help' {
            break
        }
        'mm;agent' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show what agents may do in each environment and whether this device trusts the project''s `agents:` block')
            [CompletionResult]::new('trust', 'trust', [CompletionResultType]::ParameterValue, 'Review the project''s `agents:` block and trust it on this device. Until then, and after any edit to the block, agents can only view')
            [CompletionResult]::new('setup', 'setup', [CompletionResultType]::ParameterValue, 'Connect coding tools to `mm mcp serve`')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;agent;status' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;agent;trust' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by the review of the block')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Trust the reviewed block')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;agent;setup' {
            [CompletionResult]::new('--client', '--client', [CompletionResultType]::ParameterName, 'Tool to connect. Repeatable. Defaults to the tools found on PATH')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--user', '--user', [CompletionResultType]::ParameterName, 'Register the server for this user instead of this project')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Make the changes shown')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;agent;help' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show what agents may do in each environment and whether this device trusts the project''s `agents:` block')
            [CompletionResult]::new('trust', 'trust', [CompletionResultType]::ParameterValue, 'Review the project''s `agents:` block and trust it on this device. Until then, and after any edit to the block, agents can only view')
            [CompletionResult]::new('setup', 'setup', [CompletionResultType]::ParameterValue, 'Connect coding tools to `mm mcp serve`')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;agent;help;status' {
            break
        }
        'mm;agent;help;trust' {
            break
        }
        'mm;agent;help;setup' {
            break
        }
        'mm;agent;help;help' {
            break
        }
        'mm;history' {
            [CompletionResult]::new('--outcome', '--outcome', [CompletionResultType]::ParameterName, 'Only commands that ended this way: succeeded, failed or interrupted')
            [CompletionResult]::new('--since', '--since', [CompletionResultType]::ParameterName, 'Only commands that started within this long, such as 30m, 2h or 3d')
            [CompletionResult]::new('--limit', '--limit', [CompletionResultType]::ParameterName, 'How many commands to list')
            [CompletionResult]::new('--before', '--before', [CompletionResultType]::ParameterName, 'List the commands that started before this one')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show one command: what it printed, its steps and its result')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;history;show' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;history;help' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show one command: what it printed, its steps and its result')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;history;help;show' {
            break
        }
        'mm;history;help;help' {
            break
        }
        'mm;workspace' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List the project folders mm has run in on this machine, when each was last used, and which are gone')
            [CompletionResult]::new('forget', 'forget', [CompletionResultType]::ParameterValue, 'Forget registered project folders through a reviewed plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;workspace;list' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;workspace;forget' {
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Exact hash printed by an earlier review of the same change')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--stale', '--stale', [CompletionResultType]::ParameterName, 'Forget every registered workspace whose folder or mm.yaml is gone')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Apply the exact reviewed plan')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;workspace;help' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List the project folders mm has run in on this machine, when each was last used, and which are gone')
            [CompletionResult]::new('forget', 'forget', [CompletionResultType]::ParameterValue, 'Forget registered project folders through a reviewed plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;workspace;help;list' {
            break
        }
        'mm;workspace;help;forget' {
            break
        }
        'mm;workspace;help;help' {
            break
        }
        'mm;update' {
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Install this release instead of the latest stable one, such as v0.1.2, or a release candidate such as v0.1.3-rc.1')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('--check', '--check', [CompletionResultType]::ParameterName, 'Only say whether a newer stable release exists; change nothing')
            [CompletionResult]::new('--force', '--force', [CompletionResultType]::ParameterName, 'Install an older release named with --version, install the same release again, or replace an mm built from a checkout')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Replace without asking. Needed when standard input is not a terminal')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'mm;mcp' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('serve', 'serve', [CompletionResultType]::ParameterValue, 'Serve MCP on standard input/output for the project in the current directory (or --root). No network listener is opened')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;mcp;serve' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Private limits for this device. Defaults to $MM_HOME/mcp.json, which is optional')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Project root or a path inside the project')
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--environment', '--environment', [CompletionResultType]::ParameterName, 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)')
            [CompletionResult]::new('--runtime', '--runtime', [CompletionResultType]::ParameterName, 'Ephemeral runtime adapter for this invocation only')
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Ephemeral target name for this invocation only')
            [CompletionResult]::new('--mm-home', '--mm-home', [CompletionResultType]::ParameterName, 'Directory containing mm''s local state and target catalog')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Human text or machine-readable JSON. JSON errors stay JSON')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'mm;mcp;help' {
            [CompletionResult]::new('serve', 'serve', [CompletionResultType]::ParameterValue, 'Serve MCP on standard input/output for the project in the current directory (or --root). No network listener is opened')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;mcp;help;serve' {
            break
        }
        'mm;mcp;help;help' {
            break
        }
        'mm;help' {
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Initialize a project through a reviewed plan or interactive onboarding')
            [CompletionResult]::new('dev', 'dev', [CompletionResultType]::ParameterValue, 'Set up and operate the local development environment')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Validate and edit desired project configuration through typed reviews')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'Check configuration health and resolved runtime capabilities')
            [CompletionResult]::new('ingress', 'ingress', [CompletionResultType]::ParameterValue, 'Start or stop this workstation''s global public HTTP/HTTPS forwarding')
            [CompletionResult]::new('service', 'service', [CompletionResultType]::ParameterValue, 'Inspect and operate deployed services')
            [CompletionResult]::new('job', 'job', [CompletionResultType]::ParameterValue, 'Run finite jobs and operate their schedules')
            [CompletionResult]::new('env', 'env', [CompletionResultType]::ParameterValue, 'Inspect and deliver non-secret environment values')
            [CompletionResult]::new('secret', 'secret', [CompletionResultType]::ParameterValue, 'Manage secret values, rotation, and provider instances')
            [CompletionResult]::new('access', 'access', [CompletionResultType]::ParameterValue, 'Manage application access policies, users, routes, and sessions')
            [CompletionResult]::new('build', 'build', [CompletionResultType]::ParameterValue, 'Plan and execute deterministic OCI image builds')
            [CompletionResult]::new('deploy', 'deploy', [CompletionResultType]::ParameterValue, 'Plan and apply the selected environment to its target')
            [CompletionResult]::new('team', 'team', [CompletionResultType]::ParameterValue, 'Show who may reach remote environments, join, and sync access')
            [CompletionResult]::new('forward', 'forward', [CompletionResultType]::ParameterValue, 'Reach a remote service''s port on localhost; save forwards for reuse')
            [CompletionResult]::new('key', 'key', [CompletionResultType]::ParameterValue, 'Create, show, protect, replace, or remove this device''s SSH keys for remote environments')
            [CompletionResult]::new('target', 'target', [CompletionResultType]::ParameterValue, 'Manage targets, private selection, setup, ingress, and remote access')
            [CompletionResult]::new('agent', 'agent', [CompletionResultType]::ParameterValue, 'Show and trust what AI agents may do, and connect coding tools')
            [CompletionResult]::new('history', 'history', [CompletionResultType]::ParameterValue, 'List the commands run in this project on this machine, and show what one printed')
            [CompletionResult]::new('workspace', 'workspace', [CompletionResultType]::ParameterValue, 'List the project folders mm knows on this machine, and forget the ones that are gone')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'Update this machine''s mm, and the mmd and mm-ingress-helper beside it, from a verified release')
            [CompletionResult]::new('mcp', 'mcp', [CompletionResultType]::ParameterValue, 'Serve the Model Context Protocol server that AI agents use')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'mm;help;init' {
            break
        }
        'mm;help;dev' {
            [CompletionResult]::new('setup', 'setup', [CompletionResultType]::ParameterValue, 'Set up local trust, ingress, Mailpit, Cap, dashboard, and project routes')
            [CompletionResult]::new('up', 'up', [CompletionResultType]::ParameterValue, 'Review or apply the local development deployment')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Turn optional modules, services or jobs on for this machine; `mm dev up` starts them')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Turn optional units off for this machine, stop them and remove their routes; their volumes stay')
            [CompletionResult]::new('down', 'down', [CompletionResultType]::ParameterValue, 'Stop the project''s containers; volumes, routes, and overrides are kept')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show local project and runtime status')
            [CompletionResult]::new('tls', 'tls', [CompletionResultType]::ParameterValue, 'Inspect and maintain local HTTPS certificates and trust')
            [CompletionResult]::new('services', 'services', [CompletionResultType]::ParameterValue, 'Manage the installation-wide local service stack')
            [CompletionResult]::new('dashboard', 'dashboard', [CompletionResultType]::ParameterValue, 'Serve the local developer dashboard')
            [CompletionResult]::new('mounts', 'mounts', [CompletionResultType]::ParameterValue, 'Discover, review, and disable source-code mounts')
            [CompletionResult]::new('open', 'open', [CompletionResultType]::ParameterValue, 'Print a configured local service URL')
            break
        }
        'mm;help;dev;setup' {
            [CompletionResult]::new('project', 'project', [CompletionResultType]::ParameterValue, 'Inspect and deploy an existing clone using its configured development environment')
            break
        }
        'mm;help;dev;setup;project' {
            break
        }
        'mm;help;dev;up' {
            break
        }
        'mm;help;dev;enable' {
            break
        }
        'mm;help;dev;disable' {
            break
        }
        'mm;help;dev;down' {
            break
        }
        'mm;help;dev;status' {
            break
        }
        'mm;help;dev;tls' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Query CA files, leaf renewal, and the actual platform trust store')
            [CompletionResult]::new('repair', 'repair', [CompletionResultType]::ParameterValue, 'Restore trust, renew stale leaves, and finish an interrupted rotation')
            [CompletionResult]::new('rotate', 'rotate', [CompletionResultType]::ParameterValue, 'Rotate the root with staged trust overlap and reissued leaves')
            [CompletionResult]::new('uninstall', 'uninstall', [CompletionResultType]::ParameterValue, 'Remove only exact mm-owned trust entries and local private material')
            break
        }
        'mm;help;dev;tls;status' {
            break
        }
        'mm;help;dev;tls;repair' {
            break
        }
        'mm;help;dev;tls;rotate' {
            break
        }
        'mm;help;dev;tls;uninstall' {
            break
        }
        'mm;help;dev;services' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Publish a deterministic reviewed lifecycle plan')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Apply an exact install/convergence plan hash')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show component health, ownership, image, route, certificate, and drift status')
            [CompletionResult]::new('upgrade', 'upgrade', [CompletionResultType]::ParameterValue, 'Apply an exact version upgrade plan hash')
            [CompletionResult]::new('repair', 'repair', [CompletionResultType]::ParameterValue, 'Recover an interrupted operation or reconcile drift from an exact plan')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove only mm-owned runtime resources using an exact reviewed plan')
            break
        }
        'mm;help;dev;services;plan' {
            break
        }
        'mm;help;dev;services;apply' {
            break
        }
        'mm;help;dev;services;status' {
            break
        }
        'mm;help;dev;services;upgrade' {
            break
        }
        'mm;help;dev;services;repair' {
            break
        }
        'mm;help;dev;services;remove' {
            break
        }
        'mm;help;dev;dashboard' {
            [CompletionResult]::new('serve', 'serve', [CompletionResultType]::ParameterValue, 'Run the ingress-authenticated dashboard backend')
            break
        }
        'mm;help;dev;dashboard;serve' {
            break
        }
        'mm;help;dev;mounts' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Produce a deterministic mount proposal with discovery evidence')
            [CompletionResult]::new('configure', 'configure', [CompletionResultType]::ParameterValue, 'Persist the exact reviewed proposal in the project config')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Remove a configured mount after reviewing its current plan')
            break
        }
        'mm;help;dev;mounts;plan' {
            break
        }
        'mm;help;dev;mounts;configure' {
            break
        }
        'mm;help;dev;mounts;disable' {
            break
        }
        'mm;help;dev;open' {
            break
        }
        'mm;help;config' {
            [CompletionResult]::new('validate', 'validate', [CompletionResultType]::ParameterValue, 'Validate .config/mm.yaml, its split files, and local overrides, without probing a runtime')
            [CompletionResult]::new('module', 'module', [CompletionResultType]::ParameterValue, 'Review typed module declarations')
            [CompletionResult]::new('service', 'service', [CompletionResultType]::ParameterValue, 'Review typed service declarations')
            [CompletionResult]::new('stack', 'stack', [CompletionResultType]::ParameterValue, 'Review stack declarations or compile generated artifacts')
            [CompletionResult]::new('build', 'build', [CompletionResultType]::ParameterValue, 'Review typed image-build declarations')
            [CompletionResult]::new('environment', 'environment', [CompletionResultType]::ParameterValue, 'Review typed environment declarations')
            [CompletionResult]::new('binding', 'binding', [CompletionResultType]::ParameterValue, 'Review typed environment bindings')
            [CompletionResult]::new('secret', 'secret', [CompletionResultType]::ParameterValue, 'Review secret declarations and references, never material')
            [CompletionResult]::new('ingress', 'ingress', [CompletionResultType]::ParameterValue, 'Review ingress declarations or inspect compiled routes')
            [CompletionResult]::new('mount', 'mount', [CompletionResultType]::ParameterValue, 'Review typed development mount declarations')
            [CompletionResult]::new('job', 'job', [CompletionResultType]::ParameterValue, 'Review typed finite-job declarations')
            [CompletionResult]::new('schedule', 'schedule', [CompletionResultType]::ParameterValue, 'Review typed schedule declarations')
            [CompletionResult]::new('access', 'access', [CompletionResultType]::ParameterValue, 'Review typed application-access declarations')
            [CompletionResult]::new('user', 'user', [CompletionResultType]::ParameterValue, 'Review who may reach remote environments (the users: key)')
            [CompletionResult]::new('schema', 'schema', [CompletionResultType]::ParameterValue, 'Print the generated JSON Schema for .config/mm.yaml, or for one split file''s value')
            [CompletionResult]::new('split', 'split', [CompletionResultType]::ParameterValue, 'Plan moving a top-level key into its own .config/mm/KEY.yaml; apply only with --plan and --yes')
            [CompletionResult]::new('join', 'join', [CompletionResultType]::ParameterValue, 'Plan moving a split key back into .config/mm.yaml; apply only with --plan and --yes')
            [CompletionResult]::new('sources', 'sources', [CompletionResultType]::ParameterValue, 'Show which file declares each top-level key')
            break
        }
        'mm;help;config;validate' {
            break
        }
        'mm;help;config;module' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;help;config;module;add' {
            break
        }
        'mm;help;config;module;edit' {
            break
        }
        'mm;help;config;module;remove' {
            break
        }
        'mm;help;config;service' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;help;config;service;add' {
            break
        }
        'mm;help;config;service;edit' {
            break
        }
        'mm;help;config;service;remove' {
            break
        }
        'mm;help;config;stack' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a stack declaration')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing a stack declaration')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing a stack declaration')
            [CompletionResult]::new('compile', 'compile', [CompletionResultType]::ParameterValue, 'Publish deterministic Compose manifests and generated overlays')
            break
        }
        'mm;help;config;stack;add' {
            break
        }
        'mm;help;config;stack;edit' {
            break
        }
        'mm;help;config;stack;remove' {
            break
        }
        'mm;help;config;stack;compile' {
            break
        }
        'mm;help;config;build' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;help;config;build;add' {
            break
        }
        'mm;help;config;build;edit' {
            break
        }
        'mm;help;config;build;remove' {
            break
        }
        'mm;help;config;environment' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;help;config;environment;add' {
            break
        }
        'mm;help;config;environment;edit' {
            break
        }
        'mm;help;config;environment;remove' {
            break
        }
        'mm;help;config;binding' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;help;config;binding;add' {
            break
        }
        'mm;help;config;binding;edit' {
            break
        }
        'mm;help;config;binding;remove' {
            break
        }
        'mm;help;config;secret' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;help;config;secret;add' {
            break
        }
        'mm;help;config;secret;edit' {
            break
        }
        'mm;help;config;secret;remove' {
            break
        }
        'mm;help;config;ingress' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding an ingress declaration')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing an ingress declaration')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing an ingress declaration')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List every service URL or the reason it was not routed')
            [CompletionResult]::new('explain', 'explain', [CompletionResultType]::ParameterValue, 'Explain ingress selection for one service name')
            break
        }
        'mm;help;config;ingress;add' {
            break
        }
        'mm;help;config;ingress;edit' {
            break
        }
        'mm;help;config;ingress;remove' {
            break
        }
        'mm;help;config;ingress;list' {
            break
        }
        'mm;help;config;ingress;explain' {
            break
        }
        'mm;help;config;mount' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;help;config;mount;add' {
            break
        }
        'mm;help;config;mount;edit' {
            break
        }
        'mm;help;config;mount;remove' {
            break
        }
        'mm;help;config;job' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;help;config;job;add' {
            break
        }
        'mm;help;config;job;edit' {
            break
        }
        'mm;help;config;job;remove' {
            break
        }
        'mm;help;config;schedule' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;help;config;schedule;add' {
            break
        }
        'mm;help;config;schedule;edit' {
            break
        }
        'mm;help;config;schedule;remove' {
            break
        }
        'mm;help;config;access' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;help;config;access;add' {
            break
        }
        'mm;help;config;access;edit' {
            break
        }
        'mm;help;config;access;remove' {
            break
        }
        'mm;help;config;user' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Plan adding a new named resource; apply only with --plan and --yes')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Plan replacing one named resource definition; apply only with --plan and --yes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Plan removing one named resource; apply only with --plan and --yes')
            break
        }
        'mm;help;config;user;add' {
            break
        }
        'mm;help;config;user;edit' {
            break
        }
        'mm;help;config;user;remove' {
            break
        }
        'mm;help;config;schema' {
            break
        }
        'mm;help;config;split' {
            break
        }
        'mm;help;config;join' {
            break
        }
        'mm;help;config;sources' {
            break
        }
        'mm;help;doctor' {
            [CompletionResult]::new('recover-state', 'recover-state', [CompletionResultType]::ParameterValue, 'Review recovery of pre-baseline state; apply requires the reviewed hash and --yes')
            break
        }
        'mm;help;doctor;recover-state' {
            break
        }
        'mm;help;ingress' {
            [CompletionResult]::new('start', 'start', [CompletionResultType]::ParameterValue, 'Ensure ingress is installed and healthy, then start public forwarding on ports 80 and 443')
            [CompletionResult]::new('stop', 'stop', [CompletionResultType]::ParameterValue, 'Stop global public forwarding to free ports 80 and 443; keep project containers running')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show public forwarding state and the health of the services it depends on (read-only)')
            [CompletionResult]::new('restart', 'restart', [CompletionResultType]::ParameterValue, 'Reconcile ingress prerequisites, then restart public forwarding on ports 80 and 443')
            break
        }
        'mm;help;ingress;start' {
            break
        }
        'mm;help;ingress;stop' {
            break
        }
        'mm;help;ingress;status' {
            break
        }
        'mm;help;ingress;restart' {
            break
        }
        'mm;help;service' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show desired and observed state, optionally for one service')
            [CompletionResult]::new('logs', 'logs', [CompletionResultType]::ParameterValue, 'Read a service''s log, or every service''s, from all its containers in the order the lines were written')
            [CompletionResult]::new('exec', 'exec', [CompletionResultType]::ParameterValue, 'Execute an exact argv vector; no shell is implied')
            [CompletionResult]::new('shell', 'shell', [CompletionResultType]::ParameterValue, 'Open an explicitly approved high-risk shell')
            [CompletionResult]::new('restart', 'restart', [CompletionResultType]::ParameterValue, 'Restart one service through runtime policy')
            break
        }
        'mm;help;service;status' {
            break
        }
        'mm;help;service;logs' {
            break
        }
        'mm;help;service;exec' {
            break
        }
        'mm;help;service;shell' {
            break
        }
        'mm;help;service;restart' {
            break
        }
        'mm;help;job' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List the declared jobs of the environment: when a deployment runs each, on which node, and how its last run ended')
            [CompletionResult]::new('run', 'run', [CompletionResultType]::ParameterValue, 'Enqueue a named job and wait for its durable terminal status')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show one durable run by UUID')
            [CompletionResult]::new('history', 'history', [CompletionResultType]::ParameterValue, 'List bounded run history, optionally for one job')
            [CompletionResult]::new('cancel', 'cancel', [CompletionResultType]::ParameterValue, 'Request cancellation of a queued or active run')
            [CompletionResult]::new('retry', 'retry', [CompletionResultType]::ParameterValue, 'Enqueue a new run from the current definition of a terminal run')
            [CompletionResult]::new('schedule', 'schedule', [CompletionResultType]::ParameterValue, 'Operate schedules for configured jobs')
            break
        }
        'mm;help;job;list' {
            break
        }
        'mm;help;job;run' {
            break
        }
        'mm;help;job;status' {
            break
        }
        'mm;help;job;history' {
            break
        }
        'mm;help;job;cancel' {
            break
        }
        'mm;help;job;retry' {
            break
        }
        'mm;help;job;schedule' {
            [CompletionResult]::new('reconcile', 'reconcile', [CompletionResultType]::ParameterValue, 'Plan or apply transactional definition reconciliation')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List schedules and their durable next due time')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show one schedule')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Enable one reconciled schedule')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Disable one reconciled schedule without deleting history')
            [CompletionResult]::new('run-now', 'run-now', [CompletionResultType]::ParameterValue, 'Enqueue the schedule''s job immediately, independent of enabled state')
            [CompletionResult]::new('next', 'next', [CompletionResultType]::ParameterValue, 'Show the durable next due time for one schedule')
            [CompletionResult]::new('history', 'history', [CompletionResultType]::ParameterValue, 'List bounded history for one schedule')
            [CompletionResult]::new('tick', 'tick', [CompletionResultType]::ParameterValue, 'Execute one fake-clock-friendly controller cycle')
            [CompletionResult]::new('daemon', 'daemon', [CompletionResultType]::ParameterValue, 'Run the scheduler controller until interrupted')
            break
        }
        'mm;help;job;schedule;reconcile' {
            break
        }
        'mm;help;job;schedule;list' {
            break
        }
        'mm;help;job;schedule;show' {
            break
        }
        'mm;help;job;schedule;enable' {
            break
        }
        'mm;help;job;schedule;disable' {
            break
        }
        'mm;help;job;schedule;run-now' {
            break
        }
        'mm;help;job;schedule;next' {
            break
        }
        'mm;help;job;schedule;history' {
            break
        }
        'mm;help;job;schedule;tick' {
            break
        }
        'mm;help;job;schedule;daemon' {
            break
        }
        'mm;help;env' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show effective values, provenance, and secret status')
            [CompletionResult]::new('explain', 'explain', [CompletionResultType]::ParameterValue, 'Explain every use of one global key or local variable')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set a workspace-local non-secret override. On a remote environment the value is staged on every host for the next deployment')
            [CompletionResult]::new('unset', 'unset', [CompletionResultType]::ParameterValue, 'Remove a workspace-local override. On a remote environment the removal is staged for the next deployment')
            [CompletionResult]::new('discard', 'discard', [CompletionResultType]::ParameterValue, 'Drop a variable change staged on a remote environment''s hosts before it is deployed')
            [CompletionResult]::new('default', 'default', [CompletionResultType]::ParameterValue, 'Get or change the project-wide default environment')
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'Resolve and compile every service environment in memory')
            [CompletionResult]::new('materialize', 'materialize', [CompletionResultType]::ParameterValue, 'Explicitly write a module''s conventional .env file')
            [CompletionResult]::new('diff', 'diff', [CompletionResultType]::ParameterValue, 'Compare a module''s .env with the effective compiled environment')
            [CompletionResult]::new('review', 'review', [CompletionResultType]::ParameterValue, 'Explain source and delivery without revealing secret values')
            [CompletionResult]::new('setup', 'setup', [CompletionResultType]::ParameterValue, 'Show missing bindings and the resolved delivery mode')
            [CompletionResult]::new('delivery', 'delivery', [CompletionResultType]::ParameterValue, 'Show or persist environment delivery mode')
            break
        }
        'mm;help;env;status' {
            break
        }
        'mm;help;env;explain' {
            break
        }
        'mm;help;env;set' {
            break
        }
        'mm;help;env;unset' {
            break
        }
        'mm;help;env;discard' {
            break
        }
        'mm;help;env;default' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Print the configured project default environment')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Persist a new project default environment')
            [CompletionResult]::new('unset', 'unset', [CompletionResultType]::ParameterValue, 'Remove the project default environment')
            break
        }
        'mm;help;env;default;show' {
            break
        }
        'mm;help;env;default;set' {
            break
        }
        'mm;help;env;default;unset' {
            break
        }
        'mm;help;env;check' {
            break
        }
        'mm;help;env;materialize' {
            break
        }
        'mm;help;env;diff' {
            break
        }
        'mm;help;env;review' {
            break
        }
        'mm;help;env;setup' {
            break
        }
        'mm;help;env;delivery' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show the resolved delivery mode and provenance')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Persist an explicit delivery mode for one environment')
            break
        }
        'mm;help;env;delivery;show' {
            break
        }
        'mm;help;env;delivery;set' {
            break
        }
        'mm;help;secret' {
            [CompletionResult]::new('providers', 'providers', [CompletionResultType]::ParameterValue, 'List available provider capabilities')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Initialize a secret provider for this project')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show secret set/unset state without revealing values. On a remote environment: what each host holds and has staged')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set a configured secret using a hidden prompt or stdin. On a remote environment the value is staged on every host for the next deployment')
            [CompletionResult]::new('unset', 'unset', [CompletionResultType]::ParameterValue, 'Remove a configured secret. On a remote environment the removal is staged for the next deployment')
            [CompletionResult]::new('discard', 'discard', [CompletionResultType]::ParameterValue, 'Drop a change staged on a remote environment''s hosts before it is deployed')
            [CompletionResult]::new('generate', 'generate', [CompletionResultType]::ParameterValue, 'Create configured generated secrets without overwriting existing values')
            [CompletionResult]::new('rotate', 'rotate', [CompletionResultType]::ParameterValue, 'Explicitly replace one secret using its configured source')
            [CompletionResult]::new('migrate', 'migrate', [CompletionResultType]::ParameterValue, 'Move a cluster environment''s secrets from the age store into the store mm runs on its nodes. Each becomes a version there with the revision it has now, so pointing the secrets at the store afterwards replaces no container. Safe to run again')
            [CompletionResult]::new('rotation', 'rotation', [CompletionResultType]::ParameterValue, 'Plan or apply scheduled rotations')
            [CompletionResult]::new('provider', 'provider', [CompletionResultType]::ParameterValue, 'Manage OpenBao and Vault provider instances')
            break
        }
        'mm;help;secret;providers' {
            break
        }
        'mm;help;secret;init' {
            break
        }
        'mm;help;secret;status' {
            break
        }
        'mm;help;secret;set' {
            break
        }
        'mm;help;secret;unset' {
            break
        }
        'mm;help;secret;discard' {
            break
        }
        'mm;help;secret;generate' {
            break
        }
        'mm;help;secret;rotate' {
            break
        }
        'mm;help;secret;migrate' {
            break
        }
        'mm;help;secret;rotation' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Inspect metadata and produce an approval hash without changing secrets')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Apply the exact reviewed plan using revision-guarded writes')
            break
        }
        'mm;help;secret;rotation;plan' {
            break
        }
        'mm;help;secret;rotation;apply' {
            break
        }
        'mm;help;secret;provider' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List configured provider instances and registered drivers')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show one provider instance with its typed descriptor and capabilities')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Configure a declared provider instance')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Reconfigure a declared provider instance without changing its driver or mode')
            [CompletionResult]::new('test', 'test', [CompletionResultType]::ParameterValue, 'Test one external provider instance')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show health and capabilities for one provider instance')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Produce a deterministic provider lifecycle plan')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Apply an exact provider lifecycle plan')
            [CompletionResult]::new('backup', 'backup', [CompletionResultType]::ParameterValue, 'Write an encrypted provider snapshot after exact review')
            [CompletionResult]::new('restore', 'restore', [CompletionResultType]::ParameterValue, 'Restore an encrypted provider snapshot after exact review. For the store mm runs on a cluster''s nodes: put a snapshot from `snapshot` back on every node; without --plan it shows the plan')
            [CompletionResult]::new('snapshot', 'snapshot', [CompletionResultType]::ParameterValue, 'Take a snapshot of the store mm runs on a cluster environment''s nodes: its data and the keys it opens with, sealed to you')
            [CompletionResult]::new('recovery', 'recovery', [CompletionResultType]::ParameterValue, 'The recovery copy of the store mm runs on a cluster environment''s nodes')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a provider through its exact reviewed lifecycle plan')
            break
        }
        'mm;help;secret;provider;list' {
            break
        }
        'mm;help;secret;provider;show' {
            break
        }
        'mm;help;secret;provider;add' {
            break
        }
        'mm;help;secret;provider;edit' {
            break
        }
        'mm;help;secret;provider;test' {
            break
        }
        'mm;help;secret;provider;status' {
            break
        }
        'mm;help;secret;provider;plan' {
            break
        }
        'mm;help;secret;provider;apply' {
            break
        }
        'mm;help;secret;provider;backup' {
            break
        }
        'mm;help;secret;provider;restore' {
            break
        }
        'mm;help;secret;provider;snapshot' {
            break
        }
        'mm;help;secret;provider;recovery' {
            [CompletionResult]::new('export', 'export', [CompletionResultType]::ParameterValue, 'Write the recovery copy of an environment''s managed store (its unseal key, mmd''s logins and its certificate authority), sealed with age to you or to another operator, as ASCII armour')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Keep a recovery copy another machine exported to you, so setup from this machine can give a reinstalled node its keys')
            break
        }
        'mm;help;secret;provider;recovery;export' {
            break
        }
        'mm;help;secret;provider;recovery;import' {
            break
        }
        'mm;help;secret;provider;remove' {
            break
        }
        'mm;help;access' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Review or create a password-cap policy using the managed local Cap service')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Review or remove an unattached policy and its owned secret material')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show redacted policy metadata')
            [CompletionResult]::new('attach', 'attach', [CompletionResultType]::ParameterValue, 'Review or attach one policy middleware to an owned router')
            [CompletionResult]::new('detach', 'detach', [CompletionResultType]::ParameterValue, 'Review or detach one policy middleware from an owned router')
            [CompletionResult]::new('user', 'user', [CompletionResultType]::ParameterValue, 'Manage application-access principals (separate from deployment RBAC users)')
            [CompletionResult]::new('session', 'session', [CompletionResultType]::ParameterValue, 'List, revoke, or rotate opaque sessions')
            break
        }
        'mm;help;access;add' {
            break
        }
        'mm;help;access;remove' {
            break
        }
        'mm;help;access;show' {
            break
        }
        'mm;help;access;attach' {
            break
        }
        'mm;help;access;detach' {
            break
        }
        'mm;help;access;user' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Review or create an application-access user with a hidden password')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Review or disable an application-access user and revoke active sessions')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Review or re-enable an application-access user')
            [CompletionResult]::new('password-reset', 'password-reset', [CompletionResultType]::ParameterValue, 'Review or replace an application-access user''s hidden password')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Review or remove an application-access user and revoke active sessions')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'Show redacted application-user metadata for one policy')
            break
        }
        'mm;help;access;user;add' {
            break
        }
        'mm;help;access;user;disable' {
            break
        }
        'mm;help;access;user;enable' {
            break
        }
        'mm;help;access;user;password-reset' {
            break
        }
        'mm;help;access;user;remove' {
            break
        }
        'mm;help;access;user;list' {
            break
        }
        'mm;help;access;session' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'Show redacted active-session metadata for one policy')
            [CompletionResult]::new('revoke', 'revoke', [CompletionResultType]::ParameterValue, 'Review or revoke one active session')
            [CompletionResult]::new('rotate-key', 'rotate-key', [CompletionResultType]::ParameterValue, 'Review or rotate a policy''s session encryption key')
            break
        }
        'mm;help;access;session;list' {
            break
        }
        'mm;help;access;session;revoke' {
            break
        }
        'mm;help;access;session;rotate-key' {
            break
        }
        'mm;help;build' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Print the deterministic image build plan without running Docker')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Execute an exact plan hash printed by `mm build plan`')
            [CompletionResult]::new('receipt', 'receipt', [CompletionResultType]::ParameterValue, 'Write the latest published build of this checkout and environment to FILE, for `mm deploy plan --receipt` or `mm build import` on another machine')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Review or apply importing a build receipt from `mm build receipt`')
            break
        }
        'mm;help;build;plan' {
            break
        }
        'mm;help;build;apply' {
            break
        }
        'mm;help;build;receipt' {
            break
        }
        'mm;help;build;import' {
            break
        }
        'mm;help;deploy' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Print the deterministic deployment plan without applying it')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Apply an exact plan hash printed by `mm deploy plan`')
            [CompletionResult]::new('rollback', 'rollback', [CompletionResultType]::ParameterValue, 'Plan a return to what a remote environment ran before, and apply it after review')
            [CompletionResult]::new('redeploy', 'redeploy', [CompletionResultType]::ParameterValue, 'Plan the commit a remote environment runs now again, and apply it after review, so that secrets and variables staged on its hosts go live')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show what each host of a remote environment last deployed, what it ran before, a rollout that is running or was left unfinished, and what runs on each host right now')
            [CompletionResult]::new('logs', 'logs', [CompletionResultType]::ParameterValue, 'Show the output of the jobs the last deployment ran, node by node')
            [CompletionResult]::new('cancel', 'cancel', [CompletionResultType]::ParameterValue, 'Stop a deployment or other operation that runs on a remote environment, end its processes on every host, and return the hosts it changed to what they ran before')
            break
        }
        'mm;help;deploy;plan' {
            break
        }
        'mm;help;deploy;apply' {
            break
        }
        'mm;help;deploy;rollback' {
            break
        }
        'mm;help;deploy;redeploy' {
            break
        }
        'mm;help;deploy;status' {
            break
        }
        'mm;help;deploy;logs' {
            break
        }
        'mm;help;deploy;cancel' {
            break
        }
        'mm;help;team' {
            [CompletionResult]::new('join', 'join', [CompletionResultType]::ParameterValue, 'Add yourself to the users file with this device''s key (reviewed edit)')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add a user with a given public key (reviewed edit)')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a user from the users file (reviewed edit)')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show the users file: each user''s keys and access per environment')
            [CompletionResult]::new('whoami', 'whoami', [CompletionResultType]::ParameterValue, 'Show who the remote environment thinks you are and what you may do')
            [CompletionResult]::new('sync', 'sync', [CompletionResultType]::ParameterValue, 'Reconcile the selected remote environment''s access from a commit''s users file without deploying')
            break
        }
        'mm;help;team;join' {
            break
        }
        'mm;help;team;add' {
            break
        }
        'mm;help;team;remove' {
            break
        }
        'mm;help;team;show' {
            break
        }
        'mm;help;team;whoami' {
            break
        }
        'mm;help;team;sync' {
            break
        }
        'mm;help;forward' {
            [CompletionResult]::new('save', 'save', [CompletionResultType]::ParameterValue, 'Save a forward for this project and environment, with a pinned local port')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List this project''s saved forwards')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a saved forward')
            [CompletionResult]::new('up', 'up', [CompletionResultType]::ParameterValue, 'Start every saved forward marked --auto')
            break
        }
        'mm;help;forward;save' {
            break
        }
        'mm;help;forward;list' {
            break
        }
        'mm;help;forward;remove' {
            break
        }
        'mm;help;forward;up' {
            break
        }
        'mm;help;key' {
            [CompletionResult]::new('create', 'create', [CompletionResultType]::ParameterValue, 'Create this device''s SSH key under MM_HOME, encrypted under a passphrase mm keeps in the system keychain. mm never uses ~/.ssh')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Print this device''s public key and fingerprint for the users file, and how the key is protected')
            [CompletionResult]::new('protect', 'protect', [CompletionResultType]::ParameterValue, 'Encrypt this device''s plain keys in place (reviewed). Public keys stay the same, so no host access changes')
            [CompletionResult]::new('rotate', 'rotate', [CompletionResultType]::ParameterValue, 'Replace a key with a new one (reviewed), without losing access: the new key is made beside the current one and mm offers both until you finish the rotation')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Delete a key and its passphrase from this device (reviewed)')
            break
        }
        'mm;help;key;create' {
            break
        }
        'mm;help;key;show' {
            break
        }
        'mm;help;key;protect' {
            break
        }
        'mm;help;key;rotate' {
            break
        }
        'mm;help;key;remove' {
            break
        }
        'mm;help;target' {
            [CompletionResult]::new('local', 'local', [CompletionResultType]::ParameterValue, 'Register this machine as the local target, preserving existing targets')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List targets configured beneath MM_HOME')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show one target from the shared catalog')
            [CompletionResult]::new('nodes', 'nodes', [CompletionResultType]::ParameterValue, 'List every node of the selected environment''s target: whether it answers (and why not), its mmd version, its mesh address, and the commit and time of its last applied deployment')
            [CompletionResult]::new('mesh', 'mesh', [CompletionResultType]::ParameterValue, 'The WireGuard mesh between the nodes of the selected environment''s target: look at it, or replace one node''s key')
            [CompletionResult]::new('storage', 'storage', [CompletionResultType]::ParameterValue, 'The shared volumes of the selected environment''s target: the node that keeps them, their sizes, and which nodes mount them; or delete one volume''s files')
            [CompletionResult]::new('connections', 'connections', [CompletionResultType]::ParameterValue, 'List the SSH connections this device keeps open to remote hosts between commands, or close them')
            [CompletionResult]::new('trust', 'trust', [CompletionResultType]::ParameterValue, 'Review a repo-declared target''s host key and daemon identity, and accept a change with --yes')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add an SSH target to the shared catalog')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'Update an SSH target without changing its stable identity')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a target only if its reviewed fingerprint still matches')
            [CompletionResult]::new('select', 'select', [CompletionResultType]::ParameterValue, 'Select a private target preference for this project')
            [CompletionResult]::new('preference', 'preference', [CompletionResultType]::ParameterValue, 'Show or change private runtime and target preferences')
            [CompletionResult]::new('setup', 'setup', [CompletionResultType]::ParameterValue, 'Plan or apply target provisioning and project enrollment')
            [CompletionResult]::new('ingress', 'ingress', [CompletionResultType]::ParameterValue, 'Configure and plan the target-owned ingress stack')
            [CompletionResult]::new('access', 'access', [CompletionResultType]::ParameterValue, 'Administer remote developer access on a target')
            break
        }
        'mm;help;target;local' {
            break
        }
        'mm;help;target;list' {
            break
        }
        'mm;help;target;show' {
            break
        }
        'mm;help;target;nodes' {
            break
        }
        'mm;help;target;mesh' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show every node''s mesh interface and, per peer, its endpoint, last handshake, and traffic. Lists what is wrong with what to do about it, such as the UDP port to open between two hosts. Exits 3 when a node does not answer or the mesh has a problem')
            [CompletionResult]::new('rotate', 'rotate', [CompletionResultType]::ParameterValue, 'Replace one node''s WireGuard key pair. Without --plan this prints the plan: the node makes a new key and every other node replaces its one entry for it. Nothing else changes. The private key never leaves the node')
            break
        }
        'mm;help;target;mesh;status' {
            break
        }
        'mm;help;target;mesh;rotate' {
            break
        }
        'mm;help;target;storage' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show the node that keeps shared volumes, each volume with its size and the nodes that have it mounted, and whether every node reaches the storage node. Also lists volumes that are no longer in the configuration and still have files. Exits 3 when a node does not answer or cannot reach the storage node')
            [CompletionResult]::new('delete', 'delete', [CompletionResultType]::ParameterValue, 'Delete a shared volume''s files from the storage node. Without --plan this prints what would be removed. Refused while a container on any node uses the volume. This is the only command that removes shared data: taking a volume out of `shared-volumes` keeps it')
            break
        }
        'mm;help;target;storage;status' {
            break
        }
        'mm;help;target;storage;delete' {
            break
        }
        'mm;help;target;connections' {
            break
        }
        'mm;help;target;trust' {
            break
        }
        'mm;help;target;add' {
            break
        }
        'mm;help;target;update' {
            break
        }
        'mm;help;target;remove' {
            break
        }
        'mm;help;target;select' {
            break
        }
        'mm;help;target;preference' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show resolved private runtime and target preferences')
            [CompletionResult]::new('set-runtime', 'set-runtime', [CompletionResultType]::ParameterValue, 'Persist a private runtime preference')
            [CompletionResult]::new('unset-runtime', 'unset-runtime', [CompletionResultType]::ParameterValue, 'Remove a private runtime preference')
            [CompletionResult]::new('set-target', 'set-target', [CompletionResultType]::ParameterValue, 'Persist a private target preference')
            [CompletionResult]::new('unset-target', 'unset-target', [CompletionResultType]::ParameterValue, 'Remove a private target preference')
            break
        }
        'mm;help;target;preference;show' {
            break
        }
        'mm;help;target;preference;set-runtime' {
            break
        }
        'mm;help;target;preference;unset-runtime' {
            break
        }
        'mm;help;target;preference;set-target' {
            break
        }
        'mm;help;target;preference;unset-target' {
            break
        }
        'mm;help;target;setup' {
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Produce a deterministic target setup plan. For a cluster target (`nodes:`), the plan also scans each node''s SSH host key')
            [CompletionResult]::new('apply', 'apply', [CompletionResultType]::ParameterValue, 'Execute an exact plan hash printed by `mm target setup plan`')
            [CompletionResult]::new('deploy-key', 'deploy-key', [CompletionResultType]::ParameterValue, 'Give the environment its cluster target''s read-only git deploy key, stored in the secret `setup.deploy-key.secret` names, and print its public half. A target has one key: it is copied from another environment of the target that holds it, and created only when none does. Prints the existing key when there is one')
            [CompletionResult]::new('record', 'record', [CompletionResultType]::ParameterValue, 'Write each node''s SSH host key, daemon identity, and private address from the last cluster setup into `.config/mm/targets.yaml`')
            break
        }
        'mm;help;target;setup;plan' {
            break
        }
        'mm;help;target;setup;apply' {
            break
        }
        'mm;help;target;setup;deploy-key' {
            break
        }
        'mm;help;target;setup;record' {
            break
        }
        'mm;help;target;ingress' {
            [CompletionResult]::new('configure', 'configure', [CompletionResultType]::ParameterValue, 'Configure digest-pinned Traefik and optional ACME metadata')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Publish a deterministic target-owned ingress artifact and plan')
            break
        }
        'mm;help;target;ingress;configure' {
            break
        }
        'mm;help;target;ingress;plan' {
            break
        }
        'mm;help;target;access' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Explain the privileged remote-access administration boundary')
            break
        }
        'mm;help;target;access;status' {
            break
        }
        'mm;help;agent' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show what agents may do in each environment and whether this device trusts the project''s `agents:` block')
            [CompletionResult]::new('trust', 'trust', [CompletionResultType]::ParameterValue, 'Review the project''s `agents:` block and trust it on this device. Until then, and after any edit to the block, agents can only view')
            [CompletionResult]::new('setup', 'setup', [CompletionResultType]::ParameterValue, 'Connect coding tools to `mm mcp serve`')
            break
        }
        'mm;help;agent;status' {
            break
        }
        'mm;help;agent;trust' {
            break
        }
        'mm;help;agent;setup' {
            break
        }
        'mm;help;history' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show one command: what it printed, its steps and its result')
            break
        }
        'mm;help;history;show' {
            break
        }
        'mm;help;workspace' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List the project folders mm has run in on this machine, when each was last used, and which are gone')
            [CompletionResult]::new('forget', 'forget', [CompletionResultType]::ParameterValue, 'Forget registered project folders through a reviewed plan')
            break
        }
        'mm;help;workspace;list' {
            break
        }
        'mm;help;workspace;forget' {
            break
        }
        'mm;help;update' {
            break
        }
        'mm;help;mcp' {
            [CompletionResult]::new('serve', 'serve', [CompletionResultType]::ParameterValue, 'Serve MCP on standard input/output for the project in the current directory (or --root). No network listener is opened')
            break
        }
        'mm;help;mcp;serve' {
            break
        }
        'mm;help;help' {
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
