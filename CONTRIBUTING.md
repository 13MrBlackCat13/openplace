# Contributing to openplace

Thanks for your interest in improving openplace! 🦀

## Project layout

```
backend-rs/          # the backend (Rust)
  src/routes/        # HTTP handlers, one module per API domain
  src/services/      # business logic (paint pipeline, tiles, leaderboards, …)
  migrations/        # PostgreSQL schema (sqlx migrations)
  src/bin/loadgen.rs # HTTP load generator used for benchmarks
frontend/            # web client (git submodule, served as static files)
frontend2/           # the newer Nuxt web client
translations/        # community README translations
protocol.md          # the wplace protocol reference
```

## Getting started

```sh
git clone --recurse-submodules https://github.com/openplaceteam/openplace.git
cd openplace/backend-rs
cp ../.env.example ../.env     # then set DATABASE_URL / JWT_SECRET
cargo run --release -- setup
cargo run --release -- serve
```

## Before you open a pull request

CI runs on every PR and **all of these must pass**:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --release
cargo build --release
```

Guidelines:

- Keep handlers thin: validation in `routes/`, logic in `services/`.
- Preserve API compatibility — the JSON shapes and status codes are contracts
  with existing frontends (see `protocol.md`).
- Hot paths (paint, tile serving) must stay allocation-conscious and must not
  gain synchronous sleeps or per-request DB round trips.
- Add a unit test for pure logic you touch (see `src/utils/*` for examples).
- Update `.env.example` and the README configuration table when adding
  configuration.

## Reporting issues

Please use the issue templates. For security-sensitive reports (exploits,
account takeover, DoS), prefer contacting the maintainers privately via the
[Discord server](https://discord.gg/ZRC4DnP9Z2) rather than opening a public
issue.

## Translations

See the [translation section in the README](README.md#adding-a-translation).
AI-assisted translations are welcome — see the README for the
translation workflow and versioning conventions.
