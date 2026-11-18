# Working in this repository

## Working material

`.modmon/` is the local warzone: put plans, TODOs, investigations, agent
handoffs, scratch scripts, logs, and temporary exports there. Create subfolders
only when useful. Do not leave these files in the root, source tree, or docs.
Never force-add this directory to Git or make builds depend on its contents.

Keep this file short. It is repository guidance, not a progress log.

## Production documentation

Tracked docs are for developers using or contributing to mm. Lead with the task,
requirements, commands, and expected result. Explain unfamiliar terms when they
first matter. Keep limitations accurate and near the affected instructions.
Internal design notes and implementation checklists belong in `.modmon/dev/`.
Promote useful findings into public docs only after rewriting and verifying them.
Public docs must work from a clean clone and must not link into `.modmon/`.

## Changes and checks

Follow nearby code conventions. Keep domain and application code independent of
concrete adapters. Never put secret values in config examples, logs, or plans.
Update docs alongside changes to public commands or configuration.

Use `CONTRIBUTING.md` and the root `Makefile` for setup and checks. Run checks
appropriate to the change and report what actually ran. Generate CLI reference,
schemas, completions, and manpages with `scripts/generate-artifacts.sh`; edit
their sources, not generated output. Keep reusable tests and tools tracked.
