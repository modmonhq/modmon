---
title: Let an AI agent work on your project
description: Connect a coding tool to mm's MCP server and set what agents may do in each environment.
---

An AI agent works with mm through `mm mcp serve`, a Model Context Protocol (MCP)
server that your coding tool starts. The project's `agents:` block sets what an
agent may do in each environment. For anything else, the agent gets the exact
command and asks you to run it.

For example: an agent deploys and runs commands in `local`, restarts jobs in
`staging`, and only reads status and logs in `production`.

## Connect a coding tool

Run this in the project, review the changes it lists, then run it with `--yes`:

```sh
mm agent setup
mm agent setup --yes
```

Without `--client`, mm connects the tools it finds on `PATH`. Name one with
`--client claude-code`, `opencode`, `codex`, or `cursor`. Commit the files it
writes:

| File                                                                   | Purpose                                            |
| ---------------------------------------------------------------------- | -------------------------------------------------- |
| `.mcp.json`, `opencode.json`, `.cursor/mcp.json`, `.codex/config.toml` | Start `mm mcp serve` for this project              |
| `AGENTS.md` (and `CLAUDE.md` importing it)                             | Tell agents to use the mm tools and never run `mm` |
| `.claude/settings.json`                                                | Deny Claude Code running `mm` and `mmd` directly   |

`mm agent setup --user --yes` registers the server in each tool's own
configuration instead, for every project. `./install.sh` does this for Claude
Code and OpenCode when they are installed.

The server uses standard input and output and opens no network port. It serves
the project in the directory the tool starts it in.

## Set what agents may do

Add an `agents:` block to `.config/mm.yaml`. `mm init` writes this one:

```yaml
agents:
  "*": view # every environment not listed
  local: { level: deploy, exec: true }
```

Each environment takes a level, and optionally `exec`:

| Setting          | Allows                                                                                                             |
| ---------------- | ------------------------------------------------------------------------------------------------------------------ |
| `view`           | status, services, routes, jobs, redacted logs, and plans of any change                                             |
| `operate`        | `view` plus run, cancel, and retry jobs, and run a schedule now                                                    |
| `deploy`         | `operate` plus apply configuration edits, builds, and deployments, reconcile schedules, and enable or disable them |
| `exec: true`     | run a command in any service container                                                                             |
| `exec: [api, …]` | run a command in the listed services only                                                                          |
| `secrets: true`  | stage secrets and variables on a remote environment's hosts, when your user may too                                |

Rules:

- Without a block, and for an environment the block does not cover, agents get
  `view`.
- Agents are never `admin`. No level includes `exec`; grant it separately.
- `mm.overrides.yaml` cannot set `agents`. To give the block its own reviewers,
  run `mm config split agents` and list `.config/mm/agents.yaml` in `CODEOWNERS`.
- `mm config validate` checks the block.

A longer example:

```yaml
agents:
  "*": view
  local: { level: deploy, exec: true }
  staging: { level: operate, exec: [api] }
  production: view
```

## Trust the block

An agent can edit files in your checkout, including this block. So mm applies
the block only after you trust it on your machine:

```sh
mm agent trust                       # shows the block and its hash
mm agent trust --plan HASH --yes
```

Until then, and again after any edit to the block, agents have `view`
everywhere. `mm init` trusts the block it writes. `mm agent status` shows what
the block grants, what is in force, and whether it is trusted.

## When the agent may not do something

A tool call outside the grant changes nothing. It returns `MM_AGENT_HANDOFF`
with the commands for you, and the agent shows them to you:

```json
{
  "error": {
    "code": "MM_AGENT_HANDOFF",
    "message": "Agents have 'view' in environment 'production'; this needs exec. The user can run it instead.",
    "handoff": {
      "reason": "not_granted",
      "environment": "production",
      "granted": "view",
      "requires": "exec",
      "user_commands": [
        "mm --environment production service exec api -- rails db:migrate"
      ]
    }
  }
}
```

Run the command in your own terminal, outside the agent session. Plans work
the same way: an agent with `view` can plan a production deployment, and the
plan says `"applied_by": "user"` with the `mm deploy` commands for you.

## mm refuses to run for an agent

Coding tools set environment variables for the commands their agents run. mm
checks for `CLAUDECODE`, `OPENCODE`, `CODEX_CI`, `CODEX_THREAD_ID`,
`CODEX_SANDBOX`, `CODEX_SANDBOX_NETWORK_DISABLED`, `CURSOR_AGENT`,
`GEMINI_CLI`, `AI_AGENT`, and `AGENT`. When one is set, every `mm` command
except `mm mcp serve` exits with code 3 and points the agent at the MCP tools.
Set `MM_AGENT=1` to get the same behavior in a tool mm does not know.

This also applies to commands you type into the agent's own shell. Use a
separate terminal.

## Remote environments

A remote host limits agents itself, so a change in someone's checkout cannot
widen what an agent does there. The MCP server reaches a remote host only with
a separate agent key:

```sh
mm key create --agent
mm team join --agent                  # reviewed edit of your users entry
mm team join --agent --plan HASH --yes
```

Commit the change and have it merged. After the environment is deployed or
synced, the host gives the agent key your access limited by the `agents:` block
of the commit it deployed. `mm team sync` plans show the agent as its own
line, such as `alice (agent): none -> view, +1 key(s)`. See
[remote access](./remote-access.md#agent-keys).

Without an agent key, tools for a remote environment return a handoff with
these commands.

When a host cannot be reached, or refuses the agent key because the users
file it last synced does not list it, the tool returns `MM_UNREACHABLE` with
the host, what happened, and the next step for you in `next_action`:

```json
{
  "error": {
    "code": "MM_UNREACHABLE",
    "message": "target 'production' (203.0.113.10:22) refused this device's agent key.",
    "next_action": "The users file the host last synced does not list the key `mm key show --agent` prints; a rotated key is a new key. Add it to your entry with `mm team join --agent`, commit the change, and have an admin run `mm team sync`. An agent cannot run `mm` commands: tell the user what failed and this next step."
  }
}
```

The message is mm's own; what `ssh` printed is not passed to the agent.

A request mm itself refuses, such as `mm_deploy_plan` with `rollback` for an
environment that runs on this machine, or a change to the schedules of a
remote environment, returns its own code and mm's message, which names what
to do instead:

```json
{
  "error": {
    "code": "MM_ENVIRONMENT",
    "message": "a rollback returns a remote environment to what its hosts ran before, and environment 'local' runs on this machine. To return to an earlier version here, check out its commit and plan a deployment.",
    "next_action": "The message names what to do instead. Do it with these tools where one covers it; an agent cannot run `mm` commands, so tell the user any command it names."
  }
}
```

Any other failure gets a fixed text for its kind of error, so nothing a
provider, an engine, or a host printed reaches the agent.

On a cluster target, the reads `nodes` and `deployments` do not fail when a
node refuses the agent key or does not answer, and neither does `services`
while another node answers: the call succeeds and that node's entry says why
in its `problem` field (`null` when it answered), so an agent should look at
each node's `problem` before relying on the rest.

The read `secret_store` answers for the managed secret store of a cluster
environment what `mm secret provider status NAME` prints for it: each member's
OpenBao release, seal and Raft role as its node reports it, the leader, and
the `MM_SECRET_STORE_*` findings. A member whose node does not answer has a
`problem` instead of a `status`. On an environment that runs on this machine
the read fails; a local provider is checked with `mm secret provider status`.

The read `deployments` also lists, per node, what runs there now under
`operations`: its `id`, `what`, the `principal` who started it, and when it
started and was last seen at work. A call that runs out of time fails with
`MM_TIMEOUT`, and one stopped by a cancellation with `MM_CANCELLED`; their
`next_action` sends the agent to that read. The agent does not cancel: it
tells you the operation and the command, `mm deploy cancel OPERATION`.

## Set secrets and variables

On a remote environment, `mm_stage_value` stages a secret or a variable on
every host for the next deployment, as `mm secret set` and `mm env set` do,
through the same service, the same access check and the same audit on each
host, as the agent's principal. It needs `secrets: true` for the environment
in the `agents:` block and for your user in the users file:

```yaml
agents:
  uat: { level: deploy, secrets: true }
```

The call takes `kind` (`secret` or `variable`), `key`, `action` (`set`,
`unset` or `discard`) and, with `set`, the `value`. The value is sealed to
each host before it leaves your machine; the result names the hosts, the key
and the revision, and no tool returns a value. Only give an agent a value
you would type into `mm secret set` yourself. A staged change is not live:
`mm_deploy_plan` with `"redeploy": true` reviews the deployment of the
running commit that makes it live, and `mm_deploy_apply` applies it.

`mm_read` kind `secrets` lists, per host, each declared secret and variable
with its live and staged revision and who changed it.

## Read what commands printed

Every `mm` command run in the project on this machine is kept in its command
history (see [troubleshooting](help/troubleshooting.md#see-what-a-command-printed)).
`mm_read` with kind `commands` lists them, newest first, `limit` at a time; pass
the returned `next` as `cursor` for the page after. A `filter` narrows the list:

```json
{
  "scope": { "environment": "production" },
  "kind": "commands",
  "filter": { "environment": "production", "outcome": "failed", "since-unix-ms": 1763150400000 }
}
```

`outcome` is `succeeded`, `failed` or `interrupted`. To read one command, pass
its ID as `command`: the answer holds its plain output (no colors, one line per
finished step with its time), its progress events, its exit and error code,
and its JSON result when it was run with `--output json` and the result fits
the response limit. Secret values never reach a record, and the answer is
scrubbed like every other MCP answer.

Each change an agent makes through MCP (a deployment, a job run, a
configuration edit) is listed there too, as `mcp <operation>` run by the
agent, with its outcome and the ID of its entry in the MCP audit journal.

## What this does and does not protect

- **Remote environments are enforced by the host.** An agent key gets only what
  the deployed commit's `agents:` block allows, whatever the checkout or the
  agent says.
- **Your own key is encrypted, which stops shortcuts, not a determined
  program.** `mm` keeps your key under a passphrase in the system keychain,
  so an agent cannot use the key file with `ssh`, and no `mm` command gives
  it the passphrase. An agent that sets out to act as you can still ask the
  keychain, as any program running as you can. Use a security key (`mm key
create --security-key`), which needs a touch for every connection, or run
  the agent where it reaches neither your session nor your mm home. See
  [what the encryption stops](./remote-access.md#what-this-stops-and-what-it-does-not).
  The connections `mm` keeps open between your own commands are never made
  with a security key, and the MCP server keeps none: see
  [remote access](./remote-access.md#connections-stay-open-for-five-minutes).
- **Local environments are a guardrail.** An agent with a shell on your machine
  can call `docker` or `podman` directly. The block, the trust step, and the
  refusal to run `mm` stop an agent that follows its instructions; they do not
  contain one that works around them. Restrict the agent's shell in your coding
  tool if you need that.
- **`exec` output is not redacted.** A command in a container can print what the
  service can read, including its secrets. Grant `exec` only where the agent
  may see that.

## Private limits on your machine

To cap agents on one machine below what a project grants, create `mcp.json` in
your mm home (normally `~/.mm`), owned by you with mode `0600`:

```json
{
  "schema_version": 2,
  "ceiling": { "*": "operate", "production": "view" }
}
```

The file is optional and can only narrow the project's block. It also accepts
`principal` (the name in audit records, default `agent`),
`maximum_page_size`, `maximum_response_bytes`, and `request_timeout_ms`
(how long a call may take, 15000 by default and at most 30000).
`mm_deploy_apply`, `mm_build_apply`, and `mm_job_apply` are not bound by it:
a deployment, a build, or a job runs until it ends, because one stopped half
way is left unfinished (a build or a job run cut off is killed and builds
nothing).
A version 1 file, which listed environments and capabilities, is rejected:
delete it or replace it with the form above.

## Tools

| Tool                                                               | Needs                  | Does                                                                                                                     |
| ------------------------------------------------------------------ | ---------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `mm_access`                                                        | nothing                | Lists environments and what the agent may do in each                                                                     |
| `mm_read`                                                          | `view`                 | One category: project, environments, targets, plans, services, routes, health, operations, jobs, schedules, gates, nodes, mesh, storage, names, deployments, deploy_logs, commands, secrets, secret_store, optional |
| `mm_logs`                                                          | `view`                 | Up to 200 redacted log records of one service                                                                            |
| `mm_exec`                                                          | `exec` for the service | Runs an exact argv in a service container; no shell, TTY, or stdin; up to 300 seconds and 256 KiB of output              |
| `mm_config_plan`, `mm_build_plan`, `mm_deploy_plan`, `mm_job_plan` | `view`                 | Reviews a change without making it                                                                                       |
| `mm_job_apply`                                                     | `operate` or `deploy`  | Runs, cancels, or retries a job (`operate`); reconciles or toggles schedules (`deploy`)                                  |
| `mm_config_apply`, `mm_build_apply`, `mm_deploy_apply`             | `deploy`               | Applies the plan just reviewed                                                                                           |
| `mm_stage_value`                                                   | `secrets`              | Stages a secret or variable on a remote environment's hosts for the next deployment                                      |
| `mm_dev_unit`                                                      | `deploy`               | Turns an [optional unit](./onboarding.md#run-part-of-a-large-project) on or off in the development environment, as `mm dev enable` and `mm dev disable` do |

Only the environment is required in a tool's `scope`; the target and runtime
default to what the project and your machine select.

On a remote environment, `mm_read` `jobs` returns the declared jobs and the
recent runs from every host (`recent_runs`, each with its `node`), and
`schedules` the declared schedules with the runs they started. A project that
declares none reads as empty lists. A job run through `mm_job_apply` runs on
its host, like `mm job run`; reconciling or toggling a remote environment's
schedules is refused with `MM_REMOTE_SCHEDULES`, since its deployment
reconciles them.

`mm_read` `deploy_logs` returns what `mm deploy logs` prints: the job runs
of the last deployment that ran any, in the order they ran, each with its
`node`, `state`, `exit_code`, and `output`, as a page of `items` (`cursor`
and `limit` as for `operations`). The output is redacted against the
environment's secrets like a job's own record. The runs on one page share the
256 KiB log bound; a run whose output did not fit keeps its last part and
says `output_truncated: true`. An agent reads it when a deployment fails in a
job, before or after the environment's first deployment finished.

A remote environment whose first deployment failed still answers
`operations`, `health`, `services`, `jobs`, and `deploy_logs` from the
attempt, with empty lists where nothing ran. A host that never began a
deployment of the environment refuses with `MM_NOT_DEPLOYED` and its own
message, which names the command that plans one.

`mm_read` `optional` lists the modules, services, and jobs with
`development: optional`, each with `on` (it runs on this machine) and
`enabled` (the local overrides turn it on). `mm_dev_unit` with a `unit` and
`on` turns one on or off for the development environment only; it changes the
gitignored local overrides and nothing else, so follow it with
`mm_deploy_plan` and `mm_deploy_apply` to start or stop the unit. Without
`deploy` on the development environment the result is the `mm dev enable` or
`mm dev disable` command for the user.

An apply tool takes the plan hash, the plan's single-use token, and the
confirmation `apply:<plan_hash>`. Tokens expire after ten minutes. If apply
reports stale state, request a new plan.

`mm_deploy_plan` with `"rollback": true` reviews a rollback of a remote
environment, the plan `mm deploy rollback` makes: it undoes a rollout that
was left unfinished, or else the last finished deployment. The plan has a
`rollback` object with what it `undoes`, the `commit` the nodes return to,
and the `nodes`. `mm_deploy_apply` applies it like any deployment plan.
With `"redeploy": true` it reviews the commit the environment runs now, as
`mm deploy redeploy` does, so that staged secrets and variables go live; the
plan has a `redeploy` object with the `commit` and the `nodes`.

`mm_deploy_plan` takes the release's name and inputs as
`"release": {"name": "v26.6.0", "inputs": {"BUILD_ID": "4757"}}`, the
`--release` and `--release-input` of `mm deploy plan`; a rollback returns to
the ones it ran with, and a redeploy keeps the ones the hosts run. For an
environment on this machine, `mm_deploy_apply` plans again, so give it the
same `release`; a remote plan carries its own.

Not available through MCP: an interactive shell, port forwards, reading secret values,
edits to the users file, and target, key, and host administration. Every call
that names an environment, including each refusal, is recorded in the local
audit journal with its operation and outcome, and without its arguments.

Generated snapshots are available for the
[tool contract](/schema/mcp-tools.schema.json),
[resource contract](/schema/mcp-resources.schema.json),
[project contract](/schema/mcp-project.schema.json), and
[private limits](/schema/mcp-config.schema.json). They come from the MCP
router and the Rust types through `./scripts/generate-artifacts.sh` and must
not be edited directly.
