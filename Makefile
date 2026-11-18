.PHONY: dashboard-assets rust deny generated ui docs site examples links public-content release-source release quality

# The mm binary embeds core/ui/dist, which is build output rather than tracked source.
dashboard-assets:
	npm --prefix core/ui ci
	npm --prefix core/ui run build

rust: dashboard-assets
	docker run --rm --cpus=1 --memory=4g --memory-swap=4g --pids-limit=512 -e CARGO_BUILD_JOBS=1 -e RUST_TEST_THREADS=1 -v "$$PWD:/workspace" -w /workspace mm-rust:1.99.1 sh -c "cargo fmt --all -- --check && cargo clippy --workspace --all-targets --all-features --locked -- -D warnings && cargo check --workspace --all-targets --all-features --locked && cargo test --workspace --all-targets --all-features --locked && RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked"

deny:
	./scripts/cargo-deny.sh

generated:
	./scripts/generate-artifacts.sh
	git diff --exit-code -- site/public/schema docs/reference/cli.md contrib core/ui/src/dashboard.schema.json core/ui/src/api.generated.ts core/ui/src/api.validator.generated.js core/ui/src/api.validator.generated.d.ts

ui: generated
	npm --prefix core/ui ci
	npm --prefix core/ui run lint
	npm --prefix core/ui run typecheck
	npm --prefix core/ui run unit
	npm --prefix core/ui run component
	npm --prefix core/ui run accessibility
	npm --prefix core/ui run build

# The website in site/ renders the Markdown in docs/; `make site` is the same check.
docs:
	npm --prefix site ci
	npm --prefix site run check
	npm --prefix site run build

site: docs

examples:
	npm --prefix examples/node-fullstack/backend ci
	npm --prefix examples/node-fullstack/backend run check
	npm --prefix examples/node-fullstack/frontend ci
	npm --prefix examples/node-fullstack/frontend run check

links:
	node ./scripts/check-links.mjs

public-content:
	./scripts/check-public-content.sh
	./scripts/test-check-public-content.sh

release-source:
	./scripts/test-release-source.sh

release:
	bash -n packaging/release/install.sh
	for script in scripts/release/build-linux.sh scripts/release/sign.sh scripts/release/verify.sh; do bash -n "$$script" || exit 1; done
	node ./scripts/release/check-workflow.mjs
	node ./scripts/release/release-notes.mjs Unreleased >/dev/null
	./scripts/release/test-verify.sh
	node ./scripts/release/test-tag-validation.mjs
	node ./scripts/release/test-contract.mjs
	node ./scripts/release/test-package-unix.mjs
	node ./scripts/release/test-install-sh.mjs

quality: rust deny ui docs examples links public-content release-source release
