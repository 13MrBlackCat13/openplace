<!--
Thanks for contributing to openplace! 🦀
See CONTRIBUTING.md for the full guidelines.
-->

## What does this PR change?

<!-- A short description. Link related issues with "Fixes #123" / "Closes #123". -->

## Type of change

- [ ] Bug fix (non-breaking)
- [ ] New feature (non-breaking)
- [ ] Breaking change (API contract, DB schema, or config change)
- [ ] Performance work (paint / tile hot paths)
- [ ] Documentation or translations
- [ ] CI / build / tooling

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test --release` passes
- [ ] API JSON shapes and status codes are preserved (contracts with existing
      frontends — see `protocol.md`)
- [ ] Hot paths (paint, tile serving) gained no per-request DB round trips or
      synchronous sleeps
- [ ] Pure logic I touched has a unit test (`src/utils/*` style)
- [ ] `.env.example` and the README configuration table updated for any new
      configuration
- [ ] README version header bumped if `README.md` changed

## Testing

<!-- How did you verify this? Commands, endpoints hit, migration path, etc. -->
