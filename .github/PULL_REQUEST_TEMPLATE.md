## Summary

## Behavior

What can an operator now rely on? Do not describe only types.

## Tests / evidence

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-targets --all-features --locked`
- [ ] Relevant docs/example path-aware checks
- [ ] Live runtime job (if this changes apply/observe/ownership): command and result

## Risk flags

- [ ] Changes a runtime capability advertisement
- [ ] Changes a plan hash, state schema, or secret path
- [ ] Changes target mutation or TLS identity
- [ ] Includes a design proposal for a substantial public behavior change
