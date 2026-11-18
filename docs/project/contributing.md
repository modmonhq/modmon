---
title: Contributing
description: Set up a checkout and run the checks for your change.
---

The repository's `CONTRIBUTING.md` has the full setup instructions and code map.
Run commands from the repository root. You need Docker for the Rust check
scripts, plus Node.js 24 and npm 11 for docs and dashboard work.

```sh
docker build -f Dockerfile.cargo -t mm-rust:1.98.0 .
```

## Choose your checks

| Change                         | Command                       |
| ------------------------------ | ----------------------------- |
| Documentation                  | `make docs links`             |
| Rust                           | `make rust deny`              |
| Dashboard                      | `make ui`                     |
| Generated reference or schemas | `make generated`              |
| Release tooling                | `make release release-source` |
| Public files                   | `make public-content`         |
| Everything                     | `make quality`                |

Runtime and deployment changes also need the relevant live checks listed in
`CONTRIBUTING.md`. Run those on a test machine.

## Preview documentation

The documentation is Markdown in `docs/`. The website in `site/` renders it
under `/docs`:

```sh
npm --prefix site ci
npm --prefix site run dev
```

Then open <http://localhost:4317/docs>.

Write guides around a task: requirements, commands, expected result, and fixes
for likely problems. Keep public commands and configuration docs updated with
the code. Generate reference files with `scripts/generate-artifacts.sh`.

In your PR, explain what changed and which checks ran. Keep credentials out of
examples and test output. For publishing, follow the [release guide](../contribute/releases.md).
