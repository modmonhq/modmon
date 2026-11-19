# Generated from the canonical Clap tree by ./scripts/generate-artifacts.sh. Do not edit.
# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_mm_global_optspecs
    string join \n root= e/environment= runtime= target= mm-home= output= h/help V/version
end

function __fish_mm_needs_command
    # Figure out if the current invocation already has a command.
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_mm_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        # Also print the command, so this can be used to figure out what it is.
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_mm_using_subcommand
    set -l cmd (__fish_mm_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

complete -c mm -n "__fish_mm_needs_command" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_needs_command" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_needs_command" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_needs_command" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_needs_command" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_needs_command" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_needs_command" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c mm -n "__fish_mm_needs_command" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_needs_command" -f -a "init" -d 'Initialize a project through a reviewed plan or interactive onboarding'
complete -c mm -n "__fish_mm_needs_command" -f -a "dev" -d 'Set up and operate the local development environment'
complete -c mm -n "__fish_mm_needs_command" -f -a "config" -d 'Validate and edit desired project configuration through typed reviews'
complete -c mm -n "__fish_mm_needs_command" -f -a "doctor" -d 'Check configuration health and resolved runtime capabilities'
complete -c mm -n "__fish_mm_needs_command" -f -a "ingress" -d 'Start or stop this workstation\'s global public HTTP/HTTPS forwarding'
complete -c mm -n "__fish_mm_needs_command" -f -a "service" -d 'Inspect and operate deployed services'
complete -c mm -n "__fish_mm_needs_command" -f -a "job" -d 'Run finite jobs and operate their schedules'
complete -c mm -n "__fish_mm_needs_command" -f -a "env" -d 'Inspect and deliver non-secret environment values'
complete -c mm -n "__fish_mm_needs_command" -f -a "secret" -d 'Manage secret values, rotation, and provider instances'
complete -c mm -n "__fish_mm_needs_command" -f -a "access" -d 'Manage application access policies, users, routes, and sessions'
complete -c mm -n "__fish_mm_needs_command" -f -a "build" -d 'Plan and execute deterministic OCI image builds'
complete -c mm -n "__fish_mm_needs_command" -f -a "deploy" -d 'Plan and apply the selected environment to its target'
complete -c mm -n "__fish_mm_needs_command" -f -a "team" -d 'Show who may reach remote environments, join, and sync access'
complete -c mm -n "__fish_mm_needs_command" -f -a "forward" -d 'Reach a remote service\'s port on localhost; save forwards for reuse'
complete -c mm -n "__fish_mm_needs_command" -f -a "key" -d 'Create, show, protect, replace, or remove this device\'s SSH keys for remote environments'
complete -c mm -n "__fish_mm_needs_command" -f -a "target" -d 'Manage targets, private selection, setup, ingress, and remote access'
complete -c mm -n "__fish_mm_needs_command" -f -a "agent" -d 'Show and trust what AI agents may do, and connect coding tools'
complete -c mm -n "__fish_mm_needs_command" -f -a "history" -d 'List the commands run in this project on this machine, and show what one printed'
complete -c mm -n "__fish_mm_needs_command" -f -a "workspace" -d 'List the project folders mm knows on this machine, and forget the ones that are gone'
complete -c mm -n "__fish_mm_needs_command" -f -a "mcp" -d 'Serve the Model Context Protocol server that AI agents use'
complete -c mm -n "__fish_mm_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand init" -l name -d 'Override the project name inferred from the directory' -r
complete -c mm -n "__fish_mm_using_subcommand init" -l plan -d 'Exact hash printed by the read-only initialization review' -r
complete -c mm -n "__fish_mm_using_subcommand init" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand init" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand init" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand init" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand init" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand init" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand init" -l yes -d 'Apply the reviewed configuration, Ansible starter, and local-override ignore rule'
complete -c mm -n "__fish_mm_using_subcommand init" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand init" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -f -a "setup" -d 'Set up local trust, ingress, Mailpit, Cap, dashboard, and project routes'
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -f -a "up" -d 'Review or apply the local development deployment'
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -f -a "enable" -d 'Turn optional modules, services or jobs on for this machine; `mm dev up` starts them'
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -f -a "disable" -d 'Turn optional units off for this machine, stop them and remove their routes; their volumes stay'
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -f -a "down" -d 'Stop the project\'s containers; volumes, routes, and overrides are kept'
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -f -a "status" -d 'Show local project and runtime status'
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -f -a "tls" -d 'Inspect and maintain local HTTPS certificates and trust'
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -f -a "services" -d 'Manage the installation-wide local service stack'
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -f -a "dashboard" -d 'Serve the local developer dashboard'
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -f -a "mounts" -d 'Discover, review, and disable source-code mounts'
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -f -a "open" -d 'Print a configured local service URL'
complete -c mm -n "__fish_mm_using_subcommand dev; and not __fish_seen_subcommand_from setup up enable disable down status tls services dashboard mounts open help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from setup" -l smtp-port -d 'Publish SMTP on this explicit loopback port' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from setup" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from setup" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from setup" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from setup" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from setup" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from setup" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from setup" -l no-trust -d 'Generate certificates without trusting the root'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from setup" -l system -d 'Use the privileged system trust store on macOS or Windows'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from setup" -l services -d 'Review and install the shared local service stack'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from setup" -l yes -d 'Apply the displayed service plan without prompting'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from setup" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from setup" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from setup" -f -a "project" -d 'Inspect and deploy an existing clone using its configured development environment'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from setup" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from up" -l with -d 'Turn an optional module, service or job on for this run only; repeat for more' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from up" -l plan -d 'Hash printed by the deployment review' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from up" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from up" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from up" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from up" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from up" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from up" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from up" -l yes -d 'Apply the exact reviewed plan without prompting'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from up" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from up" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from enable" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from enable" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from enable" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from enable" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from enable" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from enable" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from enable" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from enable" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from disable" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from disable" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from disable" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from disable" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from disable" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from disable" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from disable" -l yes -d 'Apply the deployment that stops them without prompting'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from disable" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from disable" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from down" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from down" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from down" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from down" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from down" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from down" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from down" -l yes -d 'Confirm stopping every stack of the selected environment'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from down" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from down" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from status" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from status" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from status" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from status" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from status" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from status" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from status" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from status" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from tls" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from tls" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from tls" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from tls" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from tls" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from tls" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from tls" -l system -d 'Use the privileged system trust store on macOS or Windows'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from tls" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from tls" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from tls" -f -a "status" -d 'Query CA files, leaf renewal, and the actual platform trust store'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from tls" -f -a "repair" -d 'Restore trust, renew stale leaves, and finish an interrupted rotation'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from tls" -f -a "rotate" -d 'Rotate the root with staged trust overlap and reissued leaves'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from tls" -f -a "uninstall" -d 'Remove only exact mm-owned trust entries and local private material'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from tls" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from services" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from services" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from services" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from services" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from services" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from services" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from services" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from services" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from services" -f -a "plan" -d 'Publish a deterministic reviewed lifecycle plan'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from services" -f -a "apply" -d 'Apply an exact install/convergence plan hash'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from services" -f -a "status" -d 'Show component health, ownership, image, route, certificate, and drift status'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from services" -f -a "upgrade" -d 'Apply an exact version upgrade plan hash'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from services" -f -a "repair" -d 'Recover an interrupted operation or reconcile drift from an exact plan'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from services" -f -a "remove" -d 'Remove only mm-owned runtime resources using an exact reviewed plan'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from services" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from dashboard" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from dashboard" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from dashboard" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from dashboard" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from dashboard" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from dashboard" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from dashboard" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from dashboard" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from dashboard" -f -a "serve" -d 'Run the ingress-authenticated dashboard backend'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from dashboard" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from mounts" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from mounts" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from mounts" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from mounts" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from mounts" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from mounts" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from mounts" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from mounts" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from mounts" -f -a "plan" -d 'Produce a deterministic mount proposal with discovery evidence'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from mounts" -f -a "configure" -d 'Persist the exact reviewed proposal in the project config'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from mounts" -f -a "disable" -d 'Remove a configured mount after reviewing its current plan'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from mounts" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from open" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from open" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from open" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from open" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from open" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from open" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from open" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from open" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "setup" -d 'Set up local trust, ingress, Mailpit, Cap, dashboard, and project routes'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "up" -d 'Review or apply the local development deployment'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "enable" -d 'Turn optional modules, services or jobs on for this machine; `mm dev up` starts them'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "disable" -d 'Turn optional units off for this machine, stop them and remove their routes; their volumes stay'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "down" -d 'Stop the project\'s containers; volumes, routes, and overrides are kept'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "status" -d 'Show local project and runtime status'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "tls" -d 'Inspect and maintain local HTTPS certificates and trust'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "services" -d 'Manage the installation-wide local service stack'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "dashboard" -d 'Serve the local developer dashboard'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "mounts" -d 'Discover, review, and disable source-code mounts'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "open" -d 'Print a configured local service URL'
complete -c mm -n "__fish_mm_using_subcommand dev; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "validate" -d 'Validate .config/mm.yaml, its split files, and local overrides, without probing a runtime'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "module" -d 'Review typed module declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "service" -d 'Review typed service declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "stack" -d 'Review stack declarations or compile generated artifacts'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "build" -d 'Review typed image-build declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "environment" -d 'Review typed environment declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "binding" -d 'Review typed environment bindings'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "secret" -d 'Review secret declarations and references, never material'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "ingress" -d 'Review ingress declarations or inspect compiled routes'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "mount" -d 'Review typed development mount declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "job" -d 'Review typed finite-job declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "schedule" -d 'Review typed schedule declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "access" -d 'Review typed application-access declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "user" -d 'Review who may reach remote environments (the users: key)'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "schema" -d 'Print the generated JSON Schema for .config/mm.yaml, or for one split file\'s value'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "split" -d 'Plan moving a top-level key into its own .config/mm/KEY.yaml; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "join" -d 'Plan moving a split key back into .config/mm.yaml; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "sources" -d 'Show which file declares each top-level key'
complete -c mm -n "__fish_mm_using_subcommand config; and not __fish_seen_subcommand_from validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from validate" -l host-policy -d 'With --for-remote: check against this copy of a host\'s host.yaml, its `allow` list and, without --environment, the environments it runs' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from validate" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from validate" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from validate" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from validate" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from validate" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from validate" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from validate" -l for-remote -d 'Also refuse Compose features a remote host blocks, in the services the environment runs; without --environment, in every environment whose target is not `local`'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from validate" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from validate" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from module" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from module" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from module" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from module" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from module" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from module" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from module" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from module" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from module" -f -a "add" -d 'Plan adding a new named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from module" -f -a "edit" -d 'Plan replacing one named resource definition; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from module" -f -a "remove" -d 'Plan removing one named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from module" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from service" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from service" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from service" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from service" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from service" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from service" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from service" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from service" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from service" -f -a "add" -d 'Plan adding a new named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from service" -f -a "edit" -d 'Plan replacing one named resource definition; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from service" -f -a "remove" -d 'Plan removing one named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from service" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from stack" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from stack" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from stack" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from stack" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from stack" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from stack" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from stack" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from stack" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from stack" -f -a "add" -d 'Plan adding a stack declaration'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from stack" -f -a "edit" -d 'Plan replacing a stack declaration'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from stack" -f -a "remove" -d 'Plan removing a stack declaration'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from stack" -f -a "compile" -d 'Publish deterministic Compose manifests and generated overlays'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from stack" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from build" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from build" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from build" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from build" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from build" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from build" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from build" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from build" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from build" -f -a "add" -d 'Plan adding a new named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from build" -f -a "edit" -d 'Plan replacing one named resource definition; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from build" -f -a "remove" -d 'Plan removing one named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from build" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from environment" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from environment" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from environment" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from environment" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from environment" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from environment" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from environment" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from environment" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from environment" -f -a "add" -d 'Plan adding a new named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from environment" -f -a "edit" -d 'Plan replacing one named resource definition; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from environment" -f -a "remove" -d 'Plan removing one named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from environment" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from binding" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from binding" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from binding" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from binding" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from binding" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from binding" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from binding" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from binding" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from binding" -f -a "add" -d 'Plan adding a new named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from binding" -f -a "edit" -d 'Plan replacing one named resource definition; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from binding" -f -a "remove" -d 'Plan removing one named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from binding" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from secret" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from secret" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from secret" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from secret" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from secret" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from secret" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from secret" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from secret" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from secret" -f -a "add" -d 'Plan adding a new named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from secret" -f -a "edit" -d 'Plan replacing one named resource definition; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from secret" -f -a "remove" -d 'Plan removing one named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from secret" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from ingress" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from ingress" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from ingress" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from ingress" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from ingress" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from ingress" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from ingress" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from ingress" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from ingress" -f -a "add" -d 'Plan adding an ingress declaration'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from ingress" -f -a "edit" -d 'Plan replacing an ingress declaration'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from ingress" -f -a "remove" -d 'Plan removing an ingress declaration'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from ingress" -f -a "list" -d 'List every service URL or the reason it was not routed'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from ingress" -f -a "explain" -d 'Explain ingress selection for one service name'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from ingress" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from mount" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from mount" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from mount" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from mount" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from mount" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from mount" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from mount" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from mount" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from mount" -f -a "add" -d 'Plan adding a new named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from mount" -f -a "edit" -d 'Plan replacing one named resource definition; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from mount" -f -a "remove" -d 'Plan removing one named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from mount" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from job" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from job" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from job" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from job" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from job" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from job" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from job" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from job" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from job" -f -a "add" -d 'Plan adding a new named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from job" -f -a "edit" -d 'Plan replacing one named resource definition; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from job" -f -a "remove" -d 'Plan removing one named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from job" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schedule" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schedule" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schedule" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schedule" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schedule" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schedule" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schedule" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schedule" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schedule" -f -a "add" -d 'Plan adding a new named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schedule" -f -a "edit" -d 'Plan replacing one named resource definition; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schedule" -f -a "remove" -d 'Plan removing one named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schedule" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from access" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from access" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from access" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from access" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from access" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from access" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from access" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from access" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from access" -f -a "add" -d 'Plan adding a new named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from access" -f -a "edit" -d 'Plan replacing one named resource definition; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from access" -f -a "remove" -d 'Plan removing one named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from access" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from user" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from user" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from user" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from user" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from user" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from user" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from user" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from user" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from user" -f -a "add" -d 'Plan adding a new named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from user" -f -a "edit" -d 'Plan replacing one named resource definition; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from user" -f -a "remove" -d 'Plan removing one named resource; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from user" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schema" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schema" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schema" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schema" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schema" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schema" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schema" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from schema" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from split" -l plan -d 'Exact hash printed by an earlier review of the same move' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from split" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from split" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from split" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from split" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from split" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from split" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from split" -l yes -d 'Apply the exact reviewed plan; never bypasses stale-file checks'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from split" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from split" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from join" -l plan -d 'Exact hash printed by an earlier review of the same move' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from join" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from join" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from join" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from join" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from join" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from join" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from join" -l yes -d 'Apply the exact reviewed plan; never bypasses stale-file checks'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from join" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from join" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from sources" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from sources" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from sources" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from sources" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from sources" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from sources" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from sources" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from sources" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "validate" -d 'Validate .config/mm.yaml, its split files, and local overrides, without probing a runtime'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "module" -d 'Review typed module declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "service" -d 'Review typed service declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "stack" -d 'Review stack declarations or compile generated artifacts'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "build" -d 'Review typed image-build declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "environment" -d 'Review typed environment declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "binding" -d 'Review typed environment bindings'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "secret" -d 'Review secret declarations and references, never material'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "ingress" -d 'Review ingress declarations or inspect compiled routes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "mount" -d 'Review typed development mount declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "job" -d 'Review typed finite-job declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "schedule" -d 'Review typed schedule declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "access" -d 'Review typed application-access declarations'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "user" -d 'Review who may reach remote environments (the users: key)'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "schema" -d 'Print the generated JSON Schema for .config/mm.yaml, or for one split file\'s value'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "split" -d 'Plan moving a top-level key into its own .config/mm/KEY.yaml; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "join" -d 'Plan moving a split key back into .config/mm.yaml; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "sources" -d 'Show which file declares each top-level key'
complete -c mm -n "__fish_mm_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand doctor; and not __fish_seen_subcommand_from recover-state help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand doctor; and not __fish_seen_subcommand_from recover-state help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand doctor; and not __fish_seen_subcommand_from recover-state help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand doctor; and not __fish_seen_subcommand_from recover-state help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand doctor; and not __fish_seen_subcommand_from recover-state help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand doctor; and not __fish_seen_subcommand_from recover-state help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand doctor; and not __fish_seen_subcommand_from recover-state help" -l check-updates -d 'Fetch the stable release manifest and compare the installed version'
complete -c mm -n "__fish_mm_using_subcommand doctor; and not __fish_seen_subcommand_from recover-state help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand doctor; and not __fish_seen_subcommand_from recover-state help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand doctor; and not __fish_seen_subcommand_from recover-state help" -f -a "recover-state" -d 'Review recovery of pre-baseline state; apply requires the reviewed hash and --yes'
complete -c mm -n "__fish_mm_using_subcommand doctor; and not __fish_seen_subcommand_from recover-state help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand doctor; and __fish_seen_subcommand_from recover-state" -l plan -d 'Exact hash from the recovery review' -r
complete -c mm -n "__fish_mm_using_subcommand doctor; and __fish_seen_subcommand_from recover-state" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand doctor; and __fish_seen_subcommand_from recover-state" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand doctor; and __fish_seen_subcommand_from recover-state" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand doctor; and __fish_seen_subcommand_from recover-state" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand doctor; and __fish_seen_subcommand_from recover-state" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand doctor; and __fish_seen_subcommand_from recover-state" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand doctor; and __fish_seen_subcommand_from recover-state" -l yes -d 'Confirm backup and replacement of the reviewed legacy state'
complete -c mm -n "__fish_mm_using_subcommand doctor; and __fish_seen_subcommand_from recover-state" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand doctor; and __fish_seen_subcommand_from recover-state" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand doctor; and __fish_seen_subcommand_from help" -f -a "recover-state" -d 'Review recovery of pre-baseline state; apply requires the reviewed hash and --yes'
complete -c mm -n "__fish_mm_using_subcommand doctor; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand ingress; and not __fish_seen_subcommand_from start stop status restart help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand ingress; and not __fish_seen_subcommand_from start stop status restart help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand ingress; and not __fish_seen_subcommand_from start stop status restart help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand ingress; and not __fish_seen_subcommand_from start stop status restart help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand ingress; and not __fish_seen_subcommand_from start stop status restart help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand ingress; and not __fish_seen_subcommand_from start stop status restart help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand ingress; and not __fish_seen_subcommand_from start stop status restart help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand ingress; and not __fish_seen_subcommand_from start stop status restart help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand ingress; and not __fish_seen_subcommand_from start stop status restart help" -f -a "start" -d 'Ensure ingress is installed and healthy, then start public forwarding on ports 80 and 443'
complete -c mm -n "__fish_mm_using_subcommand ingress; and not __fish_seen_subcommand_from start stop status restart help" -f -a "stop" -d 'Stop global public forwarding to free ports 80 and 443; keep project containers running'
complete -c mm -n "__fish_mm_using_subcommand ingress; and not __fish_seen_subcommand_from start stop status restart help" -f -a "status" -d 'Show public forwarding state and the health of the services it depends on (read-only)'
complete -c mm -n "__fish_mm_using_subcommand ingress; and not __fish_seen_subcommand_from start stop status restart help" -f -a "restart" -d 'Reconcile ingress prerequisites, then restart public forwarding on ports 80 and 443'
complete -c mm -n "__fish_mm_using_subcommand ingress; and not __fish_seen_subcommand_from start stop status restart help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from start" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from start" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from start" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from start" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from start" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from start" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from start" -l yes -d 'Approve required installation or repair plans without prompting'
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from start" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from start" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from stop" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from stop" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from stop" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from stop" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from stop" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from stop" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from stop" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from stop" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from status" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from status" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from status" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from status" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from status" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from status" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from status" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from status" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from restart" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from restart" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from restart" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from restart" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from restart" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from restart" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from restart" -l yes -d 'Approve required installation or repair plans without prompting'
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from restart" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from restart" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from help" -f -a "start" -d 'Ensure ingress is installed and healthy, then start public forwarding on ports 80 and 443'
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from help" -f -a "stop" -d 'Stop global public forwarding to free ports 80 and 443; keep project containers running'
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from help" -f -a "status" -d 'Show public forwarding state and the health of the services it depends on (read-only)'
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from help" -f -a "restart" -d 'Reconcile ingress prerequisites, then restart public forwarding on ports 80 and 443'
complete -c mm -n "__fish_mm_using_subcommand ingress; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand service; and not __fish_seen_subcommand_from status logs exec shell restart help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand service; and not __fish_seen_subcommand_from status logs exec shell restart help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand service; and not __fish_seen_subcommand_from status logs exec shell restart help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand service; and not __fish_seen_subcommand_from status logs exec shell restart help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand service; and not __fish_seen_subcommand_from status logs exec shell restart help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand service; and not __fish_seen_subcommand_from status logs exec shell restart help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand service; and not __fish_seen_subcommand_from status logs exec shell restart help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand service; and not __fish_seen_subcommand_from status logs exec shell restart help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand service; and not __fish_seen_subcommand_from status logs exec shell restart help" -f -a "status" -d 'Show desired and observed state, optionally for one service'
complete -c mm -n "__fish_mm_using_subcommand service; and not __fish_seen_subcommand_from status logs exec shell restart help" -f -a "logs" -d 'Read a service\'s log, or every service\'s, from all its containers in the order the lines were written'
complete -c mm -n "__fish_mm_using_subcommand service; and not __fish_seen_subcommand_from status logs exec shell restart help" -f -a "exec" -d 'Execute an exact argv vector; no shell is implied'
complete -c mm -n "__fish_mm_using_subcommand service; and not __fish_seen_subcommand_from status logs exec shell restart help" -f -a "shell" -d 'Open an explicitly approved high-risk shell'
complete -c mm -n "__fish_mm_using_subcommand service; and not __fish_seen_subcommand_from status logs exec shell restart help" -f -a "restart" -d 'Restart one service through runtime policy'
complete -c mm -n "__fish_mm_using_subcommand service; and not __fish_seen_subcommand_from status logs exec shell restart help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from status" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from status" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from status" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from status" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from status" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from status" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from status" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from status" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l stack -d 'Restrict the service to one configured stack' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l node -d 'On a cluster target, read only this node. Repeat it for more' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l tail -d 'Number of recent records requested before following' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l since -d 'Include records at or after this Unix millisecond timestamp' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l until -d 'Include records before this Unix millisecond timestamp' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l max-line-bytes -d 'Maximum bytes retained from any one log line' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l max-bytes-per-second -d 'Maximum aggregate log bytes accepted per second' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l max-lines-per-second -d 'Maximum aggregate log records accepted per second' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l follow -d 'Continue streaming new records until interrupted'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l timestamps -d 'Show when the engine recorded each line'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l stdout -d 'Include only standard output records'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l stderr -d 'Include only standard error records'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l raw -d 'Disable redaction. Requires --approve-high-risk'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -l approve-high-risk -d 'Confirm that raw application logs can disclose arbitrary secret values'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from logs" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -l stack -d 'Restrict the service to one configured stack' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -l container -d 'Select one exact container when the service has multiple replicas' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -l user -d 'User or numeric identity used inside the container' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -l workdir -d 'Working directory used inside the container' -r -F
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -l env -d 'Set NAME from a secret reference (NAME=provider://location); plaintext is rejected' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -l timeout -d 'Maximum command runtime before cancellation' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -l tty -d 'Allocate an interactive terminal; output capture is disabled'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -l stdin -d 'Forward standard input to the remote command'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from exec" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from shell" -l stack -d 'Restrict the service to one configured stack' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from shell" -l container -d 'Select one exact container when the service has multiple replicas' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from shell" -l user -d 'User or numeric identity used inside the container' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from shell" -l workdir -d 'Working directory used inside the container' -r -F
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from shell" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from shell" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from shell" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from shell" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from shell" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from shell" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from shell" -l approve-high-risk -d 'Confirm the shell\'s high-risk, authorization-gated behavior'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from shell" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from shell" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from restart" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from restart" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from restart" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from restart" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from restart" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from restart" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from restart" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from restart" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from help" -f -a "status" -d 'Show desired and observed state, optionally for one service'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from help" -f -a "logs" -d 'Read a service\'s log, or every service\'s, from all its containers in the order the lines were written'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from help" -f -a "exec" -d 'Execute an exact argv vector; no shell is implied'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from help" -f -a "shell" -d 'Open an explicitly approved high-risk shell'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from help" -f -a "restart" -d 'Restart one service through runtime policy'
complete -c mm -n "__fish_mm_using_subcommand service; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -f -a "list" -d 'List the declared jobs of the environment: when a deployment runs each, on which node, and how its last run ended'
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -f -a "run" -d 'Enqueue a named job and wait for its durable terminal status'
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -f -a "status" -d 'Show one durable run by UUID'
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -f -a "history" -d 'List bounded run history, optionally for one job'
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -f -a "cancel" -d 'Request cancellation of a queued or active run'
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -f -a "retry" -d 'Enqueue a new run from the current definition of a terminal run'
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -f -a "schedule" -d 'Operate schedules for configured jobs'
complete -c mm -n "__fish_mm_using_subcommand job; and not __fish_seen_subcommand_from list run status history cancel retry schedule help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from list" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from list" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from list" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from list" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from list" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from list" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from list" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from run" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from run" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from run" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from run" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from run" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from run" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from run" -l no-wait -d 'Return after durable enqueue instead of driving execution'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from run" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from run" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from status" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from status" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from status" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from status" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from status" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from status" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from status" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from status" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from history" -l limit -d 'Maximum number of newest runs to return' -r
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from history" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from history" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from history" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from history" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from history" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from history" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from history" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from history" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from cancel" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from cancel" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from cancel" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from cancel" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from cancel" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from cancel" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from cancel" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from cancel" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from retry" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from retry" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from retry" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from retry" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from retry" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from retry" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from retry" -l no-wait -d 'Return after durable enqueue instead of driving execution'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from retry" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from retry" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -f -a "reconcile" -d 'Plan or apply transactional definition reconciliation'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -f -a "list" -d 'List schedules and their durable next due time'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -f -a "show" -d 'Show one schedule'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -f -a "enable" -d 'Enable one reconciled schedule'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -f -a "disable" -d 'Disable one reconciled schedule without deleting history'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -f -a "run-now" -d 'Enqueue the schedule\'s job immediately, independent of enabled state'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -f -a "next" -d 'Show the durable next due time for one schedule'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -f -a "history" -d 'List bounded history for one schedule'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -f -a "tick" -d 'Execute one fake-clock-friendly controller cycle'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -f -a "daemon" -d 'Run the scheduler controller until interrupted'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from schedule" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from help" -f -a "list" -d 'List the declared jobs of the environment: when a deployment runs each, on which node, and how its last run ended'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from help" -f -a "run" -d 'Enqueue a named job and wait for its durable terminal status'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from help" -f -a "status" -d 'Show one durable run by UUID'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from help" -f -a "history" -d 'List bounded run history, optionally for one job'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from help" -f -a "cancel" -d 'Request cancellation of a queued or active run'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from help" -f -a "retry" -d 'Enqueue a new run from the current definition of a terminal run'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from help" -f -a "schedule" -d 'Operate schedules for configured jobs'
complete -c mm -n "__fish_mm_using_subcommand job; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -f -a "status" -d 'Show effective values, provenance, and secret status'
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -f -a "explain" -d 'Explain every use of one global key or local variable'
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -f -a "set" -d 'Set a workspace-local non-secret override. On a remote environment the value is staged on every host for the next deployment'
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -f -a "unset" -d 'Remove a workspace-local override. On a remote environment the removal is staged for the next deployment'
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -f -a "discard" -d 'Drop a variable change staged on a remote environment\'s hosts before it is deployed'
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -f -a "default" -d 'Get or change the project-wide default environment'
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -f -a "check" -d 'Resolve and compile every service environment in memory'
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -f -a "materialize" -d 'Explicitly write a module\'s conventional .env file'
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -f -a "diff" -d 'Compare a module\'s .env with the effective compiled environment'
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -f -a "review" -d 'Explain source and delivery without revealing secret values'
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -f -a "setup" -d 'Show missing bindings and the resolved delivery mode'
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -f -a "delivery" -d 'Show or persist environment delivery mode'
complete -c mm -n "__fish_mm_using_subcommand env; and not __fish_seen_subcommand_from status explain set unset discard default check materialize diff review setup delivery help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from status" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from status" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from status" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from status" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from status" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from status" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from status" -l no-check-secrets -d 'Do not contact secret providers'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from status" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from status" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from explain" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from explain" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from explain" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from explain" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from explain" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from explain" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from explain" -l no-check-secrets -d 'Do not contact secret providers'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from explain" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from explain" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from set" -l expected-revision -d 'Fail if the existing override has another revision' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from set" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from set" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from set" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from set" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from set" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from set" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from set" -l stdin -d 'Read the value from stdin instead of an argument'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from set" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from set" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from unset" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from unset" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from unset" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from unset" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from unset" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from unset" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from unset" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from unset" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from discard" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from discard" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from discard" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from discard" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from discard" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from discard" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from discard" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from discard" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from default" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from default" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from default" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from default" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from default" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from default" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from default" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from default" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from default" -f -a "show" -d 'Print the configured project default environment'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from default" -f -a "set" -d 'Persist a new project default environment'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from default" -f -a "unset" -d 'Remove the project default environment'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from default" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from check" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from check" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from check" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from check" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from check" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from check" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from check" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from check" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from materialize" -l service -d 'Limit materialization to one service in the module' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from materialize" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from materialize" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from materialize" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from materialize" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from materialize" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from materialize" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from materialize" -l force -d 'Overwrite an existing .env file'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from materialize" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from materialize" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from diff" -l service -d 'Limit the diff to one service in the module' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from diff" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from diff" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from diff" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from diff" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from diff" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from diff" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from diff" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from diff" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from review" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from review" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from review" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from review" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from review" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from review" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from review" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from review" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from setup" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from setup" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from setup" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from setup" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from setup" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from setup" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from setup" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from setup" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from delivery" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from delivery" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from delivery" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from delivery" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from delivery" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from delivery" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from delivery" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from delivery" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from delivery" -f -a "show" -d 'Show the resolved delivery mode and provenance'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from delivery" -f -a "set" -d 'Persist an explicit delivery mode for one environment'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from delivery" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from help" -f -a "status" -d 'Show effective values, provenance, and secret status'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from help" -f -a "explain" -d 'Explain every use of one global key or local variable'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from help" -f -a "set" -d 'Set a workspace-local non-secret override. On a remote environment the value is staged on every host for the next deployment'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from help" -f -a "unset" -d 'Remove a workspace-local override. On a remote environment the removal is staged for the next deployment'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from help" -f -a "discard" -d 'Drop a variable change staged on a remote environment\'s hosts before it is deployed'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from help" -f -a "default" -d 'Get or change the project-wide default environment'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from help" -f -a "check" -d 'Resolve and compile every service environment in memory'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from help" -f -a "materialize" -d 'Explicitly write a module\'s conventional .env file'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from help" -f -a "diff" -d 'Compare a module\'s .env with the effective compiled environment'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from help" -f -a "review" -d 'Explain source and delivery without revealing secret values'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from help" -f -a "setup" -d 'Show missing bindings and the resolved delivery mode'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from help" -f -a "delivery" -d 'Show or persist environment delivery mode'
complete -c mm -n "__fish_mm_using_subcommand env; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -f -a "providers" -d 'List available provider capabilities'
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -f -a "init" -d 'Initialize a secret provider for this project'
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -f -a "status" -d 'Show secret set/unset state without revealing values. On a remote environment: what each host holds and has staged'
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -f -a "set" -d 'Set a configured secret using a hidden prompt or stdin. On a remote environment the value is staged on every host for the next deployment'
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -f -a "unset" -d 'Remove a configured secret. On a remote environment the removal is staged for the next deployment'
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -f -a "discard" -d 'Drop a change staged on a remote environment\'s hosts before it is deployed'
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -f -a "generate" -d 'Create configured generated secrets without overwriting existing values'
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -f -a "rotate" -d 'Explicitly replace one secret using its configured source'
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -f -a "migrate" -d 'Move a cluster environment\'s secrets from the age store into the store mm runs on its nodes. Each becomes a version there with the revision it has now, so pointing the secrets at the store afterwards replaces no container. Safe to run again'
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -f -a "rotation" -d 'Plan or apply scheduled rotations'
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -f -a "provider" -d 'Manage OpenBao and Vault provider instances'
complete -c mm -n "__fish_mm_using_subcommand secret; and not __fish_seen_subcommand_from providers init status set unset discard generate rotate migrate rotation provider help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from providers" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from providers" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from providers" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from providers" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from providers" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from providers" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from providers" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from providers" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from init" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from init" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from init" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from init" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from init" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from init" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from init" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from init" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from status" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from status" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from status" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from status" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from status" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from status" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from status" -l local -d 'Read this machine\'s store, the one `mm target setup` pushes, even for a remote environment'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from status" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from status" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from set" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from set" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from set" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from set" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from set" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from set" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from set" -l stdin -d 'Read exactly one value from stdin without prompting'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from set" -l local -d 'Write this machine\'s store, the one `mm target setup` pushes, even for a remote environment'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from set" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from set" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from unset" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from unset" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from unset" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from unset" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from unset" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from unset" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from unset" -l yes -d 'Skip the destructive confirmation prompt'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from unset" -l local -d 'Remove it from this machine\'s store, even for a remote environment'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from unset" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from unset" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from discard" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from discard" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from discard" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from discard" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from discard" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from discard" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from discard" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from discard" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from generate" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from generate" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from generate" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from generate" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from generate" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from generate" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from generate" -l missing -d 'Generate the secrets the selected environment uses: those its bindings name, its services mount, and its registries, TLS, tunnel and deploy key read'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from generate" -l all -d 'Generate every configured generated secret, whichever environment uses it'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from generate" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from generate" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotate" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotate" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotate" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotate" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotate" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotate" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotate" -l stdin -d 'Read a prompted replacement from stdin'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotate" -l yes -d 'Apply the exact displayed rotation plan without an interactive confirmation'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotate" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotate" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from migrate" -l from -d 'The store the values come from' -r -f -a "age\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from migrate" -l to -d 'The managed store\'s name under `providers:`' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from migrate" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from migrate" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from migrate" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from migrate" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from migrate" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from migrate" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from migrate" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from migrate" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotation" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotation" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotation" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotation" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotation" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotation" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotation" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotation" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotation" -f -a "plan" -d 'Inspect metadata and produce an approval hash without changing secrets'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotation" -f -a "apply" -d 'Apply the exact reviewed plan using revision-guarded writes'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from rotation" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -f -a "list" -d 'List configured provider instances and registered drivers'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -f -a "show" -d 'Show one provider instance with its typed descriptor and capabilities'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -f -a "add" -d 'Configure a declared provider instance'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -f -a "edit" -d 'Reconfigure a declared provider instance without changing its driver or mode'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -f -a "test" -d 'Test one external provider instance'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -f -a "status" -d 'Show health and capabilities for one provider instance'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -f -a "plan" -d 'Produce a deterministic provider lifecycle plan'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -f -a "apply" -d 'Apply an exact provider lifecycle plan'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -f -a "backup" -d 'Write an encrypted provider snapshot after exact review'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -f -a "restore" -d 'Restore an encrypted provider snapshot after exact review. For the store mm runs on a cluster\'s nodes: put a snapshot from `snapshot` back on every node; without --plan it shows the plan'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -f -a "snapshot" -d 'Take a snapshot of the store mm runs on a cluster environment\'s nodes: its data and the keys it opens with, sealed to you'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -f -a "recovery" -d 'The recovery copy of the store mm runs on a cluster environment\'s nodes'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -f -a "remove" -d 'Remove a provider through its exact reviewed lifecycle plan'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from provider" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from help" -f -a "providers" -d 'List available provider capabilities'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from help" -f -a "init" -d 'Initialize a secret provider for this project'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from help" -f -a "status" -d 'Show secret set/unset state without revealing values. On a remote environment: what each host holds and has staged'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from help" -f -a "set" -d 'Set a configured secret using a hidden prompt or stdin. On a remote environment the value is staged on every host for the next deployment'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from help" -f -a "unset" -d 'Remove a configured secret. On a remote environment the removal is staged for the next deployment'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from help" -f -a "discard" -d 'Drop a change staged on a remote environment\'s hosts before it is deployed'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from help" -f -a "generate" -d 'Create configured generated secrets without overwriting existing values'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from help" -f -a "rotate" -d 'Explicitly replace one secret using its configured source'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from help" -f -a "migrate" -d 'Move a cluster environment\'s secrets from the age store into the store mm runs on its nodes. Each becomes a version there with the revision it has now, so pointing the secrets at the store afterwards replaces no container. Safe to run again'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from help" -f -a "rotation" -d 'Plan or apply scheduled rotations'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from help" -f -a "provider" -d 'Manage OpenBao and Vault provider instances'
complete -c mm -n "__fish_mm_using_subcommand secret; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -f -a "add" -d 'Review or create a password-cap policy using the managed local Cap service'
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -f -a "remove" -d 'Review or remove an unattached policy and its owned secret material'
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -f -a "show" -d 'Show redacted policy metadata'
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -f -a "attach" -d 'Review or attach one policy middleware to an owned router'
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -f -a "detach" -d 'Review or detach one policy middleware from an owned router'
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -f -a "user" -d 'Manage application-access principals (separate from deployment RBAC users)'
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -f -a "session" -d 'List, revoke, or rotate opaque sessions'
complete -c mm -n "__fish_mm_using_subcommand access; and not __fish_seen_subcommand_from add remove show attach detach user session help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from add" -l mode -d 'Driver evaluation mode for this policy' -r -f -a "any\t''
all\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from add" -l session-days -d 'Session lifetime in days (default seven, maximum thirty)' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from add" -l secret-provider -d 'Secret storage provider for generated gate keys and pepper material' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from add" -l plan -d 'Exact hash printed by the read-only policy review' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from add" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from add" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from add" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from add" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from add" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from add" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from add" -l yes -d 'Apply the exact reviewed policy plan'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from add" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from remove" -l plan -d 'Exact hash printed by the read-only removal review' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from remove" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from remove" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from remove" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from remove" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from remove" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from remove" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from remove" -l yes -d 'Apply the exact reviewed removal plan'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from remove" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from show" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from show" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from show" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from show" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from show" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from show" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from show" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from show" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from attach" -l target-id -d 'Target that owns the ingress router' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from attach" -l plan -d 'Exact hash printed by the read-only route review' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from attach" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from attach" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from attach" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from attach" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from attach" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from attach" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from attach" -l yes -d 'Apply the exact reviewed route plan'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from attach" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from attach" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from detach" -l target-id -d 'Target that owns the ingress router' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from detach" -l plan -d 'Exact hash printed by the read-only route review' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from detach" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from detach" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from detach" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from detach" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from detach" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from detach" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from detach" -l yes -d 'Apply the exact reviewed route plan'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from detach" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from detach" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from user" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from user" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from user" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from user" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from user" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from user" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from user" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from user" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from user" -f -a "add" -d 'Review or create an application-access user with a hidden password'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from user" -f -a "disable" -d 'Review or disable an application-access user and revoke active sessions'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from user" -f -a "enable" -d 'Review or re-enable an application-access user'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from user" -f -a "password-reset" -d 'Review or replace an application-access user\'s hidden password'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from user" -f -a "remove" -d 'Review or remove an application-access user and revoke active sessions'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from user" -f -a "list" -d 'Show redacted application-user metadata for one policy'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from user" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from session" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from session" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from session" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from session" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from session" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from session" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from session" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from session" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from session" -f -a "list" -d 'Show redacted active-session metadata for one policy'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from session" -f -a "revoke" -d 'Review or revoke one active session'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from session" -f -a "rotate-key" -d 'Review or rotate a policy\'s session encryption key'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from session" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from help" -f -a "add" -d 'Review or create a password-cap policy using the managed local Cap service'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from help" -f -a "remove" -d 'Review or remove an unattached policy and its owned secret material'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from help" -f -a "show" -d 'Show redacted policy metadata'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from help" -f -a "attach" -d 'Review or attach one policy middleware to an owned router'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from help" -f -a "detach" -d 'Review or detach one policy middleware from an owned router'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from help" -f -a "user" -d 'Manage application-access principals (separate from deployment RBAC users)'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from help" -f -a "session" -d 'List, revoke, or rotate opaque sessions'
complete -c mm -n "__fish_mm_using_subcommand access; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand build; and not __fish_seen_subcommand_from plan apply receipt import help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand build; and not __fish_seen_subcommand_from plan apply receipt import help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand build; and not __fish_seen_subcommand_from plan apply receipt import help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand build; and not __fish_seen_subcommand_from plan apply receipt import help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand build; and not __fish_seen_subcommand_from plan apply receipt import help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand build; and not __fish_seen_subcommand_from plan apply receipt import help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand build; and not __fish_seen_subcommand_from plan apply receipt import help" -l publish -d 'Push a multi-platform image index and reviewed tags to the registry'
complete -c mm -n "__fish_mm_using_subcommand build; and not __fish_seen_subcommand_from plan apply receipt import help" -l yes -d 'Skip the build confirmation prompt'
complete -c mm -n "__fish_mm_using_subcommand build; and not __fish_seen_subcommand_from plan apply receipt import help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand build; and not __fish_seen_subcommand_from plan apply receipt import help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand build; and not __fish_seen_subcommand_from plan apply receipt import help" -f -a "plan" -d 'Print the deterministic image build plan without running Docker'
complete -c mm -n "__fish_mm_using_subcommand build; and not __fish_seen_subcommand_from plan apply receipt import help" -f -a "apply" -d 'Execute an exact plan hash printed by `mm build plan`'
complete -c mm -n "__fish_mm_using_subcommand build; and not __fish_seen_subcommand_from plan apply receipt import help" -f -a "receipt" -d 'Write the latest published build of this checkout and environment to FILE, for `mm deploy plan --receipt` or `mm build import` on another machine'
complete -c mm -n "__fish_mm_using_subcommand build; and not __fish_seen_subcommand_from plan apply receipt import help" -f -a "import" -d 'Review or apply importing a build receipt from `mm build receipt`'
complete -c mm -n "__fish_mm_using_subcommand build; and not __fish_seen_subcommand_from plan apply receipt import help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from plan" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from plan" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from plan" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from plan" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from plan" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from plan" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from plan" -l publish -d 'Push a multi-platform image index and reviewed tags to the registry'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from plan" -l yes -d 'Skip the build confirmation prompt'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from plan" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from plan" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from apply" -l plan -d 'Hash printed by `mm build plan`' -r
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from apply" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from apply" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from apply" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from apply" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from apply" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from apply" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from apply" -l publish -d 'Push a multi-platform image index and reviewed tags to the registry'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from apply" -l yes -d 'Skip the build confirmation prompt'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from apply" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from apply" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from receipt" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from receipt" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from receipt" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from receipt" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from receipt" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from receipt" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from receipt" -l publish -d 'Push a multi-platform image index and reviewed tags to the registry'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from receipt" -l yes -d 'Skip the build confirmation prompt'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from receipt" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from receipt" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from import" -l plan -d 'Apply the import with this hash, printed by the review' -r
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from import" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from import" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from import" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from import" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from import" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from import" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from import" -l cache-only -d 'Only seed build reuse; never select deployment images'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from import" -l publish -d 'Push a multi-platform image index and reviewed tags to the registry'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from import" -l yes -d 'Skip the build confirmation prompt'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from import" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from import" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from help" -f -a "plan" -d 'Print the deterministic image build plan without running Docker'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from help" -f -a "apply" -d 'Execute an exact plan hash printed by `mm build plan`'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from help" -f -a "receipt" -d 'Write the latest published build of this checkout and environment to FILE, for `mm deploy plan --receipt` or `mm build import` on another machine'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from help" -f -a "import" -d 'Review or apply importing a build receipt from `mm build receipt`'
complete -c mm -n "__fish_mm_using_subcommand build; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -l nodes -d 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -l release -d 'The release\'s name, which templates read as MM.RELEASE. Defaults to sha- and the commit\'s first 12 characters' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -l release-input -d 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -l yes -d 'Skip the deployment confirmation prompt'
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -f -a "plan" -d 'Print the deterministic deployment plan without applying it'
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -f -a "apply" -d 'Apply an exact plan hash printed by `mm deploy plan`'
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -f -a "rollback" -d 'Plan a return to what a remote environment ran before, and apply it after review'
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -f -a "redeploy" -d 'Plan the commit a remote environment runs now again, and apply it after review, so that secrets and variables staged on its hosts go live'
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -f -a "status" -d 'Show what each host of a remote environment last deployed, what it ran before, a rollout that is running or was left unfinished, and what runs on each host right now'
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -f -a "logs" -d 'Show the output of the jobs the last deployment ran, node by node'
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -f -a "cancel" -d 'Stop a deployment or other operation that runs on a remote environment, end its processes on every host, and return the hosts it changed to what they ran before'
complete -c mm -n "__fish_mm_using_subcommand deploy; and not __fish_seen_subcommand_from plan apply rollback redeploy status logs cancel help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from plan" -l commit -d 'Commit a remote environment deploys; the host fetches it from the project\'s origin. Defaults to this checkout\'s HEAD' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from plan" -l receipt -d 'Build receipt from `mm build receipt`: deploy its images, built from the deployed commit for this or another environment' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from plan" -l nodes -d 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from plan" -l release -d 'The release\'s name, which templates read as MM.RELEASE. Defaults to sha- and the commit\'s first 12 characters' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from plan" -l release-input -d 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from plan" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from plan" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from plan" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from plan" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from plan" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from plan" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from plan" -l yes -d 'Skip the deployment confirmation prompt'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from plan" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from plan" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from apply" -l plan -d 'Hash printed by `mm deploy plan`' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from apply" -l receipt -d 'The build receipt the local plan was made with. A remote plan already carries its receipt' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from apply" -l nodes -d 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from apply" -l release -d 'The release\'s name, which templates read as MM.RELEASE. Defaults to sha- and the commit\'s first 12 characters' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from apply" -l release-input -d 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from apply" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from apply" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from apply" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from apply" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from apply" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from apply" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from apply" -l yes -d 'Skip the deployment confirmation prompt'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from apply" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from apply" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from rollback" -l nodes -d 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from rollback" -l release -d 'The release\'s name, which templates read as MM.RELEASE. Defaults to sha- and the commit\'s first 12 characters' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from rollback" -l release-input -d 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from rollback" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from rollback" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from rollback" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from rollback" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from rollback" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from rollback" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from rollback" -l yes -d 'Skip the deployment confirmation prompt'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from rollback" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from rollback" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from redeploy" -l nodes -d 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from redeploy" -l release -d 'The release\'s name, which templates read as MM.RELEASE. Defaults to sha- and the commit\'s first 12 characters' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from redeploy" -l release-input -d 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from redeploy" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from redeploy" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from redeploy" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from redeploy" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from redeploy" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from redeploy" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from redeploy" -l yes -d 'Skip the deployment confirmation prompt'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from redeploy" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from redeploy" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from status" -l nodes -d 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from status" -l release -d 'The release\'s name, which templates read as MM.RELEASE. Defaults to sha- and the commit\'s first 12 characters' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from status" -l release-input -d 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from status" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from status" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from status" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from status" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from status" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from status" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from status" -l yes -d 'Skip the deployment confirmation prompt'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from status" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from status" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from logs" -l nodes -d 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from logs" -l release -d 'The release\'s name, which templates read as MM.RELEASE. Defaults to sha- and the commit\'s first 12 characters' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from logs" -l release-input -d 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from logs" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from logs" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from logs" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from logs" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from logs" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from logs" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from logs" -l yes -d 'Skip the deployment confirmation prompt'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from logs" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from logs" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from cancel" -l nodes -d 'On a target that lists nodes, plan only these nodes. The others keep what they run now, and the plan says so' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from cancel" -l release -d 'The release\'s name, which templates read as MM.RELEASE. Defaults to sha- and the commit\'s first 12 characters' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from cancel" -l release-input -d 'A value of this release that `release-inputs:` declares, read as RELEASE.<NAME>. Repeat it for each input' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from cancel" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from cancel" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from cancel" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from cancel" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from cancel" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from cancel" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from cancel" -l yes -d 'Skip the deployment confirmation prompt'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from cancel" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from cancel" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from help" -f -a "plan" -d 'Print the deterministic deployment plan without applying it'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from help" -f -a "apply" -d 'Apply an exact plan hash printed by `mm deploy plan`'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from help" -f -a "rollback" -d 'Plan a return to what a remote environment ran before, and apply it after review'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from help" -f -a "redeploy" -d 'Plan the commit a remote environment runs now again, and apply it after review, so that secrets and variables staged on its hosts go live'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from help" -f -a "status" -d 'Show what each host of a remote environment last deployed, what it ran before, a rollout that is running or was left unfinished, and what runs on each host right now'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from help" -f -a "logs" -d 'Show the output of the jobs the last deployment ran, node by node'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from help" -f -a "cancel" -d 'Stop a deployment or other operation that runs on a remote environment, end its processes on every host, and return the hosts it changed to what they ran before'
complete -c mm -n "__fish_mm_using_subcommand deploy; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand team; and not __fish_seen_subcommand_from join add remove show whoami sync help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand team; and not __fish_seen_subcommand_from join add remove show whoami sync help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand team; and not __fish_seen_subcommand_from join add remove show whoami sync help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand team; and not __fish_seen_subcommand_from join add remove show whoami sync help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand team; and not __fish_seen_subcommand_from join add remove show whoami sync help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand team; and not __fish_seen_subcommand_from join add remove show whoami sync help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand team; and not __fish_seen_subcommand_from join add remove show whoami sync help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand team; and not __fish_seen_subcommand_from join add remove show whoami sync help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand team; and not __fish_seen_subcommand_from join add remove show whoami sync help" -f -a "join" -d 'Add yourself to the users file with this device\'s key (reviewed edit)'
complete -c mm -n "__fish_mm_using_subcommand team; and not __fish_seen_subcommand_from join add remove show whoami sync help" -f -a "add" -d 'Add a user with a given public key (reviewed edit)'
complete -c mm -n "__fish_mm_using_subcommand team; and not __fish_seen_subcommand_from join add remove show whoami sync help" -f -a "remove" -d 'Remove a user from the users file (reviewed edit)'
complete -c mm -n "__fish_mm_using_subcommand team; and not __fish_seen_subcommand_from join add remove show whoami sync help" -f -a "show" -d 'Show the users file: each user\'s keys and access per environment'
complete -c mm -n "__fish_mm_using_subcommand team; and not __fish_seen_subcommand_from join add remove show whoami sync help" -f -a "whoami" -d 'Show who the remote environment thinks you are and what you may do'
complete -c mm -n "__fish_mm_using_subcommand team; and not __fish_seen_subcommand_from join add remove show whoami sync help" -f -a "sync" -d 'Reconcile the selected remote environment\'s access from a commit\'s users file without deploying'
complete -c mm -n "__fish_mm_using_subcommand team; and not __fish_seen_subcommand_from join add remove show whoami sync help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from join" -l name -d 'User name, [a-z][a-z0-9-]*. Defaults to $USER' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from join" -l request -d 'Access to request, for example production=view. Repeatable' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from join" -l plan -d 'Exact hash printed by an earlier review of the same change' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from join" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from join" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from join" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from join" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from join" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from join" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from join" -l agent -d 'Add this device\'s agent key to your existing entry instead, marked `agent: true`. Create it first with `mm key create --agent`. After `mm key rotate --agent` it takes the place of the old agent key'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from join" -l yes -d 'Apply the exact reviewed plan'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from join" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from join" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from add" -l key -d 'OpenSSH public key line, `ssh-ed25519 AAAA… comment`' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from add" -l access -d 'Access per environment, for example staging=deploy or *=view. Repeatable' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from add" -l plan -d 'Exact hash printed by an earlier review of the same change' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from add" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from add" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from add" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from add" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from add" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from add" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from add" -l machine -d 'A CI or automation identity rather than a person'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from add" -l yes -d 'Apply the exact reviewed plan'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from add" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from remove" -l plan -d 'Exact hash printed by an earlier review of the same change' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from remove" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from remove" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from remove" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from remove" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from remove" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from remove" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from remove" -l yes -d 'Apply the exact reviewed plan'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from remove" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from show" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from show" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from show" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from show" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from show" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from show" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from show" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from show" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from whoami" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from whoami" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from whoami" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from whoami" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from whoami" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from whoami" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from whoami" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from whoami" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from sync" -l commit -d 'Commit whose users file to apply; defaults to HEAD' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from sync" -l nodes -d 'On a target that lists nodes, plan only these nodes. The others keep the access they enforce now' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from sync" -l plan -d 'Exact hash printed by an earlier review of the same change' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from sync" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from sync" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from sync" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from sync" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from sync" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from sync" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from sync" -l yes -d 'Apply the exact reviewed plan'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from sync" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from sync" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from help" -f -a "join" -d 'Add yourself to the users file with this device\'s key (reviewed edit)'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from help" -f -a "add" -d 'Add a user with a given public key (reviewed edit)'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from help" -f -a "remove" -d 'Remove a user from the users file (reviewed edit)'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from help" -f -a "show" -d 'Show the users file: each user\'s keys and access per environment'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from help" -f -a "whoami" -d 'Show who the remote environment thinks you are and what you may do'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from help" -f -a "sync" -d 'Reconcile the selected remote environment\'s access from a commit\'s users file without deploying'
complete -c mm -n "__fish_mm_using_subcommand team; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand forward; and not __fish_seen_subcommand_from save list remove up help" -l local -d 'Local port; defaults to the service port, or a free port if that is busy' -r
complete -c mm -n "__fish_mm_using_subcommand forward; and not __fish_seen_subcommand_from save list remove up help" -l bind -d 'Local address to listen on. Anything but loopback exposes the service to your network' -r
complete -c mm -n "__fish_mm_using_subcommand forward; and not __fish_seen_subcommand_from save list remove up help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand forward; and not __fish_seen_subcommand_from save list remove up help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand forward; and not __fish_seen_subcommand_from save list remove up help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand forward; and not __fish_seen_subcommand_from save list remove up help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand forward; and not __fish_seen_subcommand_from save list remove up help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand forward; and not __fish_seen_subcommand_from save list remove up help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand forward; and not __fish_seen_subcommand_from save list remove up help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand forward; and not __fish_seen_subcommand_from save list remove up help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand forward; and not __fish_seen_subcommand_from save list remove up help" -a "save" -d 'Save a forward for this project and environment, with a pinned local port'
complete -c mm -n "__fish_mm_using_subcommand forward; and not __fish_seen_subcommand_from save list remove up help" -a "list" -d 'List this project\'s saved forwards'
complete -c mm -n "__fish_mm_using_subcommand forward; and not __fish_seen_subcommand_from save list remove up help" -a "remove" -d 'Remove a saved forward'
complete -c mm -n "__fish_mm_using_subcommand forward; and not __fish_seen_subcommand_from save list remove up help" -a "up" -d 'Start every saved forward marked --auto'
complete -c mm -n "__fish_mm_using_subcommand forward; and not __fish_seen_subcommand_from save list remove up help" -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from save" -l local -d 'Local port to pin; defaults to the service port' -r
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from save" -l bind -d 'Local address to listen on; defaults to 127.0.0.1' -r
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from save" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from save" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from save" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from save" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from save" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from save" -l auto -d 'Start it with `mm forward up`'
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from save" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from save" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from list" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from list" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from list" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from list" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from list" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from list" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from list" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from remove" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from remove" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from remove" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from remove" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from remove" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from remove" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from remove" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from up" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from up" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from up" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from up" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from up" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from up" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from up" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from up" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from help" -f -a "save" -d 'Save a forward for this project and environment, with a pinned local port'
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from help" -f -a "list" -d 'List this project\'s saved forwards'
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from help" -f -a "remove" -d 'Remove a saved forward'
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from help" -f -a "up" -d 'Start every saved forward marked --auto'
complete -c mm -n "__fish_mm_using_subcommand forward; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand key; and not __fish_seen_subcommand_from create show protect rotate remove help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand key; and not __fish_seen_subcommand_from create show protect rotate remove help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand key; and not __fish_seen_subcommand_from create show protect rotate remove help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand key; and not __fish_seen_subcommand_from create show protect rotate remove help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand key; and not __fish_seen_subcommand_from create show protect rotate remove help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand key; and not __fish_seen_subcommand_from create show protect rotate remove help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand key; and not __fish_seen_subcommand_from create show protect rotate remove help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand key; and not __fish_seen_subcommand_from create show protect rotate remove help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand key; and not __fish_seen_subcommand_from create show protect rotate remove help" -f -a "create" -d 'Create this device\'s SSH key under MM_HOME, encrypted under a passphrase mm keeps in the system keychain. mm never uses ~/.ssh'
complete -c mm -n "__fish_mm_using_subcommand key; and not __fish_seen_subcommand_from create show protect rotate remove help" -f -a "show" -d 'Print this device\'s public key and fingerprint for the users file, and how the key is protected'
complete -c mm -n "__fish_mm_using_subcommand key; and not __fish_seen_subcommand_from create show protect rotate remove help" -f -a "protect" -d 'Encrypt this device\'s plain keys in place (reviewed). Public keys stay the same, so no host access changes'
complete -c mm -n "__fish_mm_using_subcommand key; and not __fish_seen_subcommand_from create show protect rotate remove help" -f -a "rotate" -d 'Replace a key with a new one (reviewed), without losing access: the new key is made beside the current one and mm offers both until you finish the rotation'
complete -c mm -n "__fish_mm_using_subcommand key; and not __fish_seen_subcommand_from create show protect rotate remove help" -f -a "remove" -d 'Delete a key and its passphrase from this device (reviewed)'
complete -c mm -n "__fish_mm_using_subcommand key; and not __fish_seen_subcommand_from create show protect rotate remove help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from create" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from create" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from create" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from create" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from create" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from create" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from create" -l security-key -d 'Use a FIDO2 security key (sk-ssh-ed25519); touch it when asked'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from create" -l agent -d 'Create the key `mm mcp serve` uses instead: a separate key for your AI agent, limited on the host by the `agents:` block'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from create" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from create" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from show" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from show" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from show" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from show" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from show" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from show" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from show" -l agent -d 'Print the agent key instead'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from show" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from show" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from protect" -l plan -d 'Exact hash printed by an earlier review of the same change' -r
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from protect" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from protect" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from protect" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from protect" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from protect" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from protect" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from protect" -l yes -d 'Apply the exact reviewed plan'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from protect" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from protect" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from rotate" -l plan -d 'Exact hash printed by an earlier review of the same change' -r
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from rotate" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from rotate" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from rotate" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from rotate" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from rotate" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from rotate" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from rotate" -l agent -d 'Rotate the agent key instead'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from rotate" -l finish -d 'Put the new key in place of the current one and delete the old key. Hosts whose users file does not list the new key refuse this device from then on'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from rotate" -l unlisted -d 'Finish even though the users file here does not list the new key'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from rotate" -l yes -d 'Apply the exact reviewed plan'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from rotate" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from rotate" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from remove" -l plan -d 'Exact hash printed by an earlier review of the same change' -r
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from remove" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from remove" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from remove" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from remove" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from remove" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from remove" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from remove" -l agent -d 'Delete the agent key instead'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from remove" -l yes -d 'Apply the exact reviewed plan'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from remove" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from help" -f -a "create" -d 'Create this device\'s SSH key under MM_HOME, encrypted under a passphrase mm keeps in the system keychain. mm never uses ~/.ssh'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from help" -f -a "show" -d 'Print this device\'s public key and fingerprint for the users file, and how the key is protected'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from help" -f -a "protect" -d 'Encrypt this device\'s plain keys in place (reviewed). Public keys stay the same, so no host access changes'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from help" -f -a "rotate" -d 'Replace a key with a new one (reviewed), without losing access: the new key is made beside the current one and mm offers both until you finish the rotation'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from help" -f -a "remove" -d 'Delete a key and its passphrase from this device (reviewed)'
complete -c mm -n "__fish_mm_using_subcommand key; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "local" -d 'Register this machine as the local target, preserving existing targets'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "list" -d 'List targets configured beneath MM_HOME'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "show" -d 'Show one target from the shared catalog'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "nodes" -d 'List every node of the selected environment\'s target: whether it answers (and why not), its mmd version, its mesh address, and the commit and time of its last applied deployment'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "mesh" -d 'The WireGuard mesh between the nodes of the selected environment\'s target: look at it, or replace one node\'s key'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "storage" -d 'The shared volumes of the selected environment\'s target: the node that keeps them, their sizes, and which nodes mount them; or delete one volume\'s files'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "connections" -d 'List the SSH connections this device keeps open to remote hosts between commands, or close them'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "trust" -d 'Review a repo-declared target\'s host key and daemon identity, and accept a change with --yes'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "add" -d 'Add an SSH target to the shared catalog'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "update" -d 'Update an SSH target without changing its stable identity'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "remove" -d 'Remove a target only if its reviewed fingerprint still matches'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "select" -d 'Select a private target preference for this project'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "preference" -d 'Show or change private runtime and target preferences'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "setup" -d 'Plan or apply target provisioning and project enrollment'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "ingress" -d 'Configure and plan the target-owned ingress stack'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "access" -d 'Administer remote developer access on a target'
complete -c mm -n "__fish_mm_using_subcommand target; and not __fish_seen_subcommand_from local list show nodes mesh storage connections trust add update remove select preference setup ingress access help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from local" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from local" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from local" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from local" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from local" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from local" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from local" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from local" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from list" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from list" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from list" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from list" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from list" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from list" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from list" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from show" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from show" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from show" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from show" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from show" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from show" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from show" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from show" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from nodes" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from nodes" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from nodes" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from nodes" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from nodes" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from nodes" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from nodes" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from nodes" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from mesh" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from mesh" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from mesh" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from mesh" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from mesh" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from mesh" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from mesh" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from mesh" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from mesh" -f -a "status" -d 'Show every node\'s mesh interface and, per peer, its endpoint, last handshake, and traffic. Lists what is wrong with what to do about it, such as the UDP port to open between two hosts. Exits 3 when a node does not answer or the mesh has a problem'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from mesh" -f -a "rotate" -d 'Replace one node\'s WireGuard key pair. Without --plan this prints the plan: the node makes a new key and every other node replaces its one entry for it. Nothing else changes. The private key never leaves the node'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from mesh" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from storage" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from storage" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from storage" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from storage" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from storage" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from storage" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from storage" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from storage" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from storage" -f -a "status" -d 'Show the node that keeps shared volumes, each volume with its size and the nodes that have it mounted, and whether every node reaches the storage node. Also lists volumes that are no longer in the configuration and still have files. Exits 3 when a node does not answer or cannot reach the storage node'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from storage" -f -a "delete" -d 'Delete a shared volume\'s files from the storage node. Without --plan this prints what would be removed. Refused while a container on any node uses the volume. This is the only command that removes shared data: taking a volume out of `shared-volumes` keeps it'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from storage" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from connections" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from connections" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from connections" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from connections" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from connections" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from connections" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from connections" -l close -d 'Close every kept connection'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from connections" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from connections" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from trust" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from trust" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from trust" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from trust" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from trust" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from trust" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from trust" -l yes -d 'Accept the repo\'s current keys after reviewing them'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from trust" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from trust" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -l host -d 'SSH hostname or address' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -l user -d 'SSH user' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -l port -d 'SSH port' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -l host-key-fingerprint -d 'OpenSSH SHA256 fingerprint accepted for this host' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -l host-key -d 'Full reviewed OpenSSH host public key used for strict known-hosts verification' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -l identity-file -d 'Absolute private-key path used for public-key client authentication' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -l remote-root -d 'Absolute project checkout path on the remote host' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -l daemon-fingerprint -d 'BLAKE3 daemon identity printed by `mmd --print-identity`' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from add" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -l host -d 'SSH hostname or address' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -l user -d 'SSH user' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -l port -d 'SSH port' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -l host-key-fingerprint -d 'OpenSSH SHA256 fingerprint accepted for this host' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -l host-key -d 'Full reviewed OpenSSH host public key used for strict known-hosts verification' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -l identity-file -d 'Absolute private-key path used for public-key client authentication' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -l remote-root -d 'Absolute project checkout path on the remote host' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -l daemon-fingerprint -d 'BLAKE3 daemon identity printed by `mmd --print-identity`' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -l expected-fingerprint -d 'Fingerprint from the reviewed target list' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from update" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from remove" -l expected-fingerprint -d 'Fingerprint from `mm target show`' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from remove" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from remove" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from remove" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from remove" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from remove" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from remove" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from remove" -l yes -d 'Confirm catalog mutation'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from remove" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from select" -l environment -d 'Limit the preference to one environment' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from select" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from select" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from select" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from select" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from select" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from select" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from preference" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from preference" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from preference" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from preference" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from preference" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from preference" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from preference" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from preference" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from preference" -f -a "show" -d 'Show resolved private runtime and target preferences'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from preference" -f -a "set-runtime" -d 'Persist a private runtime preference'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from preference" -f -a "unset-runtime" -d 'Remove a private runtime preference'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from preference" -f -a "set-target" -d 'Persist a private target preference'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from preference" -f -a "unset-target" -d 'Remove a private target preference'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from preference" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from setup" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from setup" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from setup" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from setup" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from setup" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from setup" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from setup" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from setup" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from setup" -f -a "plan" -d 'Produce a deterministic target setup plan. For a cluster target (`nodes:`), the plan also scans each node\'s SSH host key'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from setup" -f -a "apply" -d 'Execute an exact plan hash printed by `mm target setup plan`'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from setup" -f -a "deploy-key" -d 'Give the environment its cluster target\'s read-only git deploy key, stored in the secret `setup.deploy-key.secret` names, and print its public half. A target has one key: it is copied from another environment of the target that holds it, and created only when none does. Prints the existing key when there is one'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from setup" -f -a "record" -d 'Write each node\'s SSH host key, daemon identity, and private address from the last cluster setup into `.config/mm/targets.yaml`'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from setup" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from ingress" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from ingress" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from ingress" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from ingress" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from ingress" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from ingress" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from ingress" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from ingress" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from ingress" -f -a "configure" -d 'Configure digest-pinned Traefik and optional ACME metadata'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from ingress" -f -a "plan" -d 'Publish a deterministic target-owned ingress artifact and plan'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from ingress" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from access" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from access" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from access" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from access" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from access" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from access" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from access" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from access" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from access" -f -a "status" -d 'Explain the privileged remote-access administration boundary'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from access" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "local" -d 'Register this machine as the local target, preserving existing targets'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "list" -d 'List targets configured beneath MM_HOME'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "show" -d 'Show one target from the shared catalog'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "nodes" -d 'List every node of the selected environment\'s target: whether it answers (and why not), its mmd version, its mesh address, and the commit and time of its last applied deployment'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "mesh" -d 'The WireGuard mesh between the nodes of the selected environment\'s target: look at it, or replace one node\'s key'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "storage" -d 'The shared volumes of the selected environment\'s target: the node that keeps them, their sizes, and which nodes mount them; or delete one volume\'s files'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "connections" -d 'List the SSH connections this device keeps open to remote hosts between commands, or close them'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "trust" -d 'Review a repo-declared target\'s host key and daemon identity, and accept a change with --yes'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "add" -d 'Add an SSH target to the shared catalog'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "update" -d 'Update an SSH target without changing its stable identity'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "remove" -d 'Remove a target only if its reviewed fingerprint still matches'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "select" -d 'Select a private target preference for this project'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "preference" -d 'Show or change private runtime and target preferences'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "setup" -d 'Plan or apply target provisioning and project enrollment'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "ingress" -d 'Configure and plan the target-owned ingress stack'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "access" -d 'Administer remote developer access on a target'
complete -c mm -n "__fish_mm_using_subcommand target; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand agent; and not __fish_seen_subcommand_from status trust setup help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand agent; and not __fish_seen_subcommand_from status trust setup help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand agent; and not __fish_seen_subcommand_from status trust setup help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand agent; and not __fish_seen_subcommand_from status trust setup help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand agent; and not __fish_seen_subcommand_from status trust setup help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand agent; and not __fish_seen_subcommand_from status trust setup help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand agent; and not __fish_seen_subcommand_from status trust setup help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand agent; and not __fish_seen_subcommand_from status trust setup help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand agent; and not __fish_seen_subcommand_from status trust setup help" -f -a "status" -d 'Show what agents may do in each environment and whether this device trusts the project\'s `agents:` block'
complete -c mm -n "__fish_mm_using_subcommand agent; and not __fish_seen_subcommand_from status trust setup help" -f -a "trust" -d 'Review the project\'s `agents:` block and trust it on this device. Until then, and after any edit to the block, agents can only view'
complete -c mm -n "__fish_mm_using_subcommand agent; and not __fish_seen_subcommand_from status trust setup help" -f -a "setup" -d 'Connect coding tools to `mm mcp serve`'
complete -c mm -n "__fish_mm_using_subcommand agent; and not __fish_seen_subcommand_from status trust setup help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from status" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from status" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from status" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from status" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from status" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from status" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from status" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from status" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from trust" -l plan -d 'Exact hash printed by the review of the block' -r
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from trust" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from trust" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from trust" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from trust" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from trust" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from trust" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from trust" -l yes -d 'Trust the reviewed block'
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from trust" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from trust" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from setup" -l client -d 'Tool to connect. Repeatable. Defaults to the tools found on PATH' -r -f -a "claude-code\t''
opencode\t''
codex\t''
cursor\t''"
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from setup" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from setup" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from setup" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from setup" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from setup" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from setup" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from setup" -l user -d 'Register the server for this user instead of this project'
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from setup" -l yes -d 'Make the changes shown'
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from setup" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from setup" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from help" -f -a "status" -d 'Show what agents may do in each environment and whether this device trusts the project\'s `agents:` block'
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from help" -f -a "trust" -d 'Review the project\'s `agents:` block and trust it on this device. Until then, and after any edit to the block, agents can only view'
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from help" -f -a "setup" -d 'Connect coding tools to `mm mcp serve`'
complete -c mm -n "__fish_mm_using_subcommand agent; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand history; and not __fish_seen_subcommand_from show help" -l outcome -d 'Only commands that ended this way: succeeded, failed or interrupted' -r
complete -c mm -n "__fish_mm_using_subcommand history; and not __fish_seen_subcommand_from show help" -l since -d 'Only commands that started within this long, such as 30m, 2h or 3d' -r
complete -c mm -n "__fish_mm_using_subcommand history; and not __fish_seen_subcommand_from show help" -l limit -d 'How many commands to list' -r
complete -c mm -n "__fish_mm_using_subcommand history; and not __fish_seen_subcommand_from show help" -l before -d 'List the commands that started before this one' -r
complete -c mm -n "__fish_mm_using_subcommand history; and not __fish_seen_subcommand_from show help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand history; and not __fish_seen_subcommand_from show help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand history; and not __fish_seen_subcommand_from show help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand history; and not __fish_seen_subcommand_from show help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand history; and not __fish_seen_subcommand_from show help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand history; and not __fish_seen_subcommand_from show help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand history; and not __fish_seen_subcommand_from show help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand history; and not __fish_seen_subcommand_from show help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand history; and not __fish_seen_subcommand_from show help" -f -a "show" -d 'Show one command: what it printed, its steps and its result'
complete -c mm -n "__fish_mm_using_subcommand history; and not __fish_seen_subcommand_from show help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand history; and __fish_seen_subcommand_from show" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand history; and __fish_seen_subcommand_from show" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand history; and __fish_seen_subcommand_from show" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand history; and __fish_seen_subcommand_from show" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand history; and __fish_seen_subcommand_from show" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand history; and __fish_seen_subcommand_from show" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand history; and __fish_seen_subcommand_from show" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand history; and __fish_seen_subcommand_from show" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand history; and __fish_seen_subcommand_from help" -f -a "show" -d 'Show one command: what it printed, its steps and its result'
complete -c mm -n "__fish_mm_using_subcommand history; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand workspace; and not __fish_seen_subcommand_from list forget help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand workspace; and not __fish_seen_subcommand_from list forget help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand workspace; and not __fish_seen_subcommand_from list forget help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand workspace; and not __fish_seen_subcommand_from list forget help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand workspace; and not __fish_seen_subcommand_from list forget help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand workspace; and not __fish_seen_subcommand_from list forget help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand workspace; and not __fish_seen_subcommand_from list forget help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand workspace; and not __fish_seen_subcommand_from list forget help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand workspace; and not __fish_seen_subcommand_from list forget help" -f -a "list" -d 'List the project folders mm has run in on this machine, when each was last used, and which are gone'
complete -c mm -n "__fish_mm_using_subcommand workspace; and not __fish_seen_subcommand_from list forget help" -f -a "forget" -d 'Forget registered project folders through a reviewed plan'
complete -c mm -n "__fish_mm_using_subcommand workspace; and not __fish_seen_subcommand_from list forget help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from list" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from list" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from list" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from list" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from list" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from list" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from list" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from forget" -l plan -d 'Exact hash printed by an earlier review of the same change' -r
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from forget" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from forget" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from forget" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from forget" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from forget" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from forget" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from forget" -l stale -d 'Forget every registered workspace whose folder or mm.yaml is gone'
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from forget" -l yes -d 'Apply the exact reviewed plan'
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from forget" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from forget" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from help" -f -a "list" -d 'List the project folders mm has run in on this machine, when each was last used, and which are gone'
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from help" -f -a "forget" -d 'Forget registered project folders through a reviewed plan'
complete -c mm -n "__fish_mm_using_subcommand workspace; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand mcp; and not __fish_seen_subcommand_from serve help" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand mcp; and not __fish_seen_subcommand_from serve help" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand mcp; and not __fish_seen_subcommand_from serve help" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand mcp; and not __fish_seen_subcommand_from serve help" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand mcp; and not __fish_seen_subcommand_from serve help" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand mcp; and not __fish_seen_subcommand_from serve help" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand mcp; and not __fish_seen_subcommand_from serve help" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand mcp; and not __fish_seen_subcommand_from serve help" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand mcp; and not __fish_seen_subcommand_from serve help" -f -a "serve" -d 'Serve MCP on standard input/output for the project in the current directory (or --root). No network listener is opened'
complete -c mm -n "__fish_mm_using_subcommand mcp; and not __fish_seen_subcommand_from serve help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand mcp; and __fish_seen_subcommand_from serve" -l config -d 'Private limits for this device. Defaults to $MM_HOME/mcp.json, which is optional' -r -F
complete -c mm -n "__fish_mm_using_subcommand mcp; and __fish_seen_subcommand_from serve" -l root -d 'Project root or a path inside the project' -r -F
complete -c mm -n "__fish_mm_using_subcommand mcp; and __fish_seen_subcommand_from serve" -s e -l environment -d 'Environment to operate on (overrides MM_ENVIRONMENT and default-environment)' -r
complete -c mm -n "__fish_mm_using_subcommand mcp; and __fish_seen_subcommand_from serve" -l runtime -d 'Ephemeral runtime adapter for this invocation only' -r -f -a "podman-compose\t''
docker-compose\t''"
complete -c mm -n "__fish_mm_using_subcommand mcp; and __fish_seen_subcommand_from serve" -l target -d 'Ephemeral target name for this invocation only' -r
complete -c mm -n "__fish_mm_using_subcommand mcp; and __fish_seen_subcommand_from serve" -l mm-home -d 'Directory containing mm\'s local state and target catalog' -r -F
complete -c mm -n "__fish_mm_using_subcommand mcp; and __fish_seen_subcommand_from serve" -l output -d 'Human text or machine-readable JSON. JSON errors stay JSON' -r -f -a "human\t''
json\t''"
complete -c mm -n "__fish_mm_using_subcommand mcp; and __fish_seen_subcommand_from serve" -s h -l help -d 'Print help'
complete -c mm -n "__fish_mm_using_subcommand mcp; and __fish_seen_subcommand_from serve" -s V -l version -d 'Print version'
complete -c mm -n "__fish_mm_using_subcommand mcp; and __fish_seen_subcommand_from help" -f -a "serve" -d 'Serve MCP on standard input/output for the project in the current directory (or --root). No network listener is opened'
complete -c mm -n "__fish_mm_using_subcommand mcp; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "init" -d 'Initialize a project through a reviewed plan or interactive onboarding'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "dev" -d 'Set up and operate the local development environment'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "config" -d 'Validate and edit desired project configuration through typed reviews'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "doctor" -d 'Check configuration health and resolved runtime capabilities'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "ingress" -d 'Start or stop this workstation\'s global public HTTP/HTTPS forwarding'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "service" -d 'Inspect and operate deployed services'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "job" -d 'Run finite jobs and operate their schedules'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "env" -d 'Inspect and deliver non-secret environment values'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "secret" -d 'Manage secret values, rotation, and provider instances'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "access" -d 'Manage application access policies, users, routes, and sessions'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "build" -d 'Plan and execute deterministic OCI image builds'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "deploy" -d 'Plan and apply the selected environment to its target'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "team" -d 'Show who may reach remote environments, join, and sync access'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "forward" -d 'Reach a remote service\'s port on localhost; save forwards for reuse'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "key" -d 'Create, show, protect, replace, or remove this device\'s SSH keys for remote environments'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "target" -d 'Manage targets, private selection, setup, ingress, and remote access'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "agent" -d 'Show and trust what AI agents may do, and connect coding tools'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "history" -d 'List the commands run in this project on this machine, and show what one printed'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "workspace" -d 'List the project folders mm knows on this machine, and forget the ones that are gone'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "mcp" -d 'Serve the Model Context Protocol server that AI agents use'
complete -c mm -n "__fish_mm_using_subcommand help; and not __fish_seen_subcommand_from init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace mcp help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "setup" -d 'Set up local trust, ingress, Mailpit, Cap, dashboard, and project routes'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "up" -d 'Review or apply the local development deployment'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "enable" -d 'Turn optional modules, services or jobs on for this machine; `mm dev up` starts them'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "disable" -d 'Turn optional units off for this machine, stop them and remove their routes; their volumes stay'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "down" -d 'Stop the project\'s containers; volumes, routes, and overrides are kept'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "status" -d 'Show local project and runtime status'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "tls" -d 'Inspect and maintain local HTTPS certificates and trust'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "services" -d 'Manage the installation-wide local service stack'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "dashboard" -d 'Serve the local developer dashboard'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "mounts" -d 'Discover, review, and disable source-code mounts'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from dev" -f -a "open" -d 'Print a configured local service URL'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "validate" -d 'Validate .config/mm.yaml, its split files, and local overrides, without probing a runtime'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "module" -d 'Review typed module declarations'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "service" -d 'Review typed service declarations'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "stack" -d 'Review stack declarations or compile generated artifacts'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "build" -d 'Review typed image-build declarations'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "environment" -d 'Review typed environment declarations'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "binding" -d 'Review typed environment bindings'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "secret" -d 'Review secret declarations and references, never material'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "ingress" -d 'Review ingress declarations or inspect compiled routes'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "mount" -d 'Review typed development mount declarations'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "job" -d 'Review typed finite-job declarations'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "schedule" -d 'Review typed schedule declarations'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "access" -d 'Review typed application-access declarations'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "user" -d 'Review who may reach remote environments (the users: key)'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "schema" -d 'Print the generated JSON Schema for .config/mm.yaml, or for one split file\'s value'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "split" -d 'Plan moving a top-level key into its own .config/mm/KEY.yaml; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "join" -d 'Plan moving a split key back into .config/mm.yaml; apply only with --plan and --yes'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "sources" -d 'Show which file declares each top-level key'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from doctor" -f -a "recover-state" -d 'Review recovery of pre-baseline state; apply requires the reviewed hash and --yes'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from ingress" -f -a "start" -d 'Ensure ingress is installed and healthy, then start public forwarding on ports 80 and 443'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from ingress" -f -a "stop" -d 'Stop global public forwarding to free ports 80 and 443; keep project containers running'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from ingress" -f -a "status" -d 'Show public forwarding state and the health of the services it depends on (read-only)'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from ingress" -f -a "restart" -d 'Reconcile ingress prerequisites, then restart public forwarding on ports 80 and 443'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from service" -f -a "status" -d 'Show desired and observed state, optionally for one service'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from service" -f -a "logs" -d 'Read a service\'s log, or every service\'s, from all its containers in the order the lines were written'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from service" -f -a "exec" -d 'Execute an exact argv vector; no shell is implied'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from service" -f -a "shell" -d 'Open an explicitly approved high-risk shell'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from service" -f -a "restart" -d 'Restart one service through runtime policy'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from job" -f -a "list" -d 'List the declared jobs of the environment: when a deployment runs each, on which node, and how its last run ended'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from job" -f -a "run" -d 'Enqueue a named job and wait for its durable terminal status'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from job" -f -a "status" -d 'Show one durable run by UUID'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from job" -f -a "history" -d 'List bounded run history, optionally for one job'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from job" -f -a "cancel" -d 'Request cancellation of a queued or active run'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from job" -f -a "retry" -d 'Enqueue a new run from the current definition of a terminal run'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from job" -f -a "schedule" -d 'Operate schedules for configured jobs'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from env" -f -a "status" -d 'Show effective values, provenance, and secret status'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from env" -f -a "explain" -d 'Explain every use of one global key or local variable'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from env" -f -a "set" -d 'Set a workspace-local non-secret override. On a remote environment the value is staged on every host for the next deployment'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from env" -f -a "unset" -d 'Remove a workspace-local override. On a remote environment the removal is staged for the next deployment'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from env" -f -a "discard" -d 'Drop a variable change staged on a remote environment\'s hosts before it is deployed'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from env" -f -a "default" -d 'Get or change the project-wide default environment'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from env" -f -a "check" -d 'Resolve and compile every service environment in memory'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from env" -f -a "materialize" -d 'Explicitly write a module\'s conventional .env file'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from env" -f -a "diff" -d 'Compare a module\'s .env with the effective compiled environment'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from env" -f -a "review" -d 'Explain source and delivery without revealing secret values'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from env" -f -a "setup" -d 'Show missing bindings and the resolved delivery mode'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from env" -f -a "delivery" -d 'Show or persist environment delivery mode'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from secret" -f -a "providers" -d 'List available provider capabilities'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from secret" -f -a "init" -d 'Initialize a secret provider for this project'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from secret" -f -a "status" -d 'Show secret set/unset state without revealing values. On a remote environment: what each host holds and has staged'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from secret" -f -a "set" -d 'Set a configured secret using a hidden prompt or stdin. On a remote environment the value is staged on every host for the next deployment'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from secret" -f -a "unset" -d 'Remove a configured secret. On a remote environment the removal is staged for the next deployment'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from secret" -f -a "discard" -d 'Drop a change staged on a remote environment\'s hosts before it is deployed'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from secret" -f -a "generate" -d 'Create configured generated secrets without overwriting existing values'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from secret" -f -a "rotate" -d 'Explicitly replace one secret using its configured source'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from secret" -f -a "migrate" -d 'Move a cluster environment\'s secrets from the age store into the store mm runs on its nodes. Each becomes a version there with the revision it has now, so pointing the secrets at the store afterwards replaces no container. Safe to run again'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from secret" -f -a "rotation" -d 'Plan or apply scheduled rotations'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from secret" -f -a "provider" -d 'Manage OpenBao and Vault provider instances'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from access" -f -a "add" -d 'Review or create a password-cap policy using the managed local Cap service'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from access" -f -a "remove" -d 'Review or remove an unattached policy and its owned secret material'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from access" -f -a "show" -d 'Show redacted policy metadata'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from access" -f -a "attach" -d 'Review or attach one policy middleware to an owned router'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from access" -f -a "detach" -d 'Review or detach one policy middleware from an owned router'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from access" -f -a "user" -d 'Manage application-access principals (separate from deployment RBAC users)'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from access" -f -a "session" -d 'List, revoke, or rotate opaque sessions'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from build" -f -a "plan" -d 'Print the deterministic image build plan without running Docker'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from build" -f -a "apply" -d 'Execute an exact plan hash printed by `mm build plan`'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from build" -f -a "receipt" -d 'Write the latest published build of this checkout and environment to FILE, for `mm deploy plan --receipt` or `mm build import` on another machine'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from build" -f -a "import" -d 'Review or apply importing a build receipt from `mm build receipt`'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from deploy" -f -a "plan" -d 'Print the deterministic deployment plan without applying it'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from deploy" -f -a "apply" -d 'Apply an exact plan hash printed by `mm deploy plan`'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from deploy" -f -a "rollback" -d 'Plan a return to what a remote environment ran before, and apply it after review'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from deploy" -f -a "redeploy" -d 'Plan the commit a remote environment runs now again, and apply it after review, so that secrets and variables staged on its hosts go live'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from deploy" -f -a "status" -d 'Show what each host of a remote environment last deployed, what it ran before, a rollout that is running or was left unfinished, and what runs on each host right now'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from deploy" -f -a "logs" -d 'Show the output of the jobs the last deployment ran, node by node'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from deploy" -f -a "cancel" -d 'Stop a deployment or other operation that runs on a remote environment, end its processes on every host, and return the hosts it changed to what they ran before'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from team" -f -a "join" -d 'Add yourself to the users file with this device\'s key (reviewed edit)'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from team" -f -a "add" -d 'Add a user with a given public key (reviewed edit)'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from team" -f -a "remove" -d 'Remove a user from the users file (reviewed edit)'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from team" -f -a "show" -d 'Show the users file: each user\'s keys and access per environment'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from team" -f -a "whoami" -d 'Show who the remote environment thinks you are and what you may do'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from team" -f -a "sync" -d 'Reconcile the selected remote environment\'s access from a commit\'s users file without deploying'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from forward" -f -a "save" -d 'Save a forward for this project and environment, with a pinned local port'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from forward" -f -a "list" -d 'List this project\'s saved forwards'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from forward" -f -a "remove" -d 'Remove a saved forward'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from forward" -f -a "up" -d 'Start every saved forward marked --auto'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from key" -f -a "create" -d 'Create this device\'s SSH key under MM_HOME, encrypted under a passphrase mm keeps in the system keychain. mm never uses ~/.ssh'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from key" -f -a "show" -d 'Print this device\'s public key and fingerprint for the users file, and how the key is protected'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from key" -f -a "protect" -d 'Encrypt this device\'s plain keys in place (reviewed). Public keys stay the same, so no host access changes'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from key" -f -a "rotate" -d 'Replace a key with a new one (reviewed), without losing access: the new key is made beside the current one and mm offers both until you finish the rotation'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from key" -f -a "remove" -d 'Delete a key and its passphrase from this device (reviewed)'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "local" -d 'Register this machine as the local target, preserving existing targets'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "list" -d 'List targets configured beneath MM_HOME'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "show" -d 'Show one target from the shared catalog'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "nodes" -d 'List every node of the selected environment\'s target: whether it answers (and why not), its mmd version, its mesh address, and the commit and time of its last applied deployment'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "mesh" -d 'The WireGuard mesh between the nodes of the selected environment\'s target: look at it, or replace one node\'s key'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "storage" -d 'The shared volumes of the selected environment\'s target: the node that keeps them, their sizes, and which nodes mount them; or delete one volume\'s files'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "connections" -d 'List the SSH connections this device keeps open to remote hosts between commands, or close them'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "trust" -d 'Review a repo-declared target\'s host key and daemon identity, and accept a change with --yes'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "add" -d 'Add an SSH target to the shared catalog'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "update" -d 'Update an SSH target without changing its stable identity'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "remove" -d 'Remove a target only if its reviewed fingerprint still matches'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "select" -d 'Select a private target preference for this project'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "preference" -d 'Show or change private runtime and target preferences'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "setup" -d 'Plan or apply target provisioning and project enrollment'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "ingress" -d 'Configure and plan the target-owned ingress stack'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from target" -f -a "access" -d 'Administer remote developer access on a target'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from agent" -f -a "status" -d 'Show what agents may do in each environment and whether this device trusts the project\'s `agents:` block'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from agent" -f -a "trust" -d 'Review the project\'s `agents:` block and trust it on this device. Until then, and after any edit to the block, agents can only view'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from agent" -f -a "setup" -d 'Connect coding tools to `mm mcp serve`'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from history" -f -a "show" -d 'Show one command: what it printed, its steps and its result'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from workspace" -f -a "list" -d 'List the project folders mm has run in on this machine, when each was last used, and which are gone'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from workspace" -f -a "forget" -d 'Forget registered project folders through a reviewed plan'
complete -c mm -n "__fish_mm_using_subcommand help; and __fish_seen_subcommand_from mcp" -f -a "serve" -d 'Serve MCP on standard input/output for the project in the current directory (or --root). No network listener is opened'
