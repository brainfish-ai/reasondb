# Contributing to ReasonDB

Thank you for your interest in contributing to ReasonDB. This guide will help you get set up and submit changes.

## Development setup

You need a local clone and a Rust toolchain. No Docker is required for building the server or CLI, but Docker is useful for running a full stack or testing the image.

### Prerequisites

- **Rust 1.91+** — The repo pins the toolchain; if you have [rustup](https://rustup.rs), run `rustup show` in the repo to use the pinned version.
- **Git** — To clone and branch.

### Clone and build

```bash
git clone https://github.com/reasondb/reasondb.git
cd reasondb
cargo build --release
```

The first build can take a few minutes. The server binary is `target/release/reasondb-server`; the CLI is `target/release/reasondb`. You can run the server with:

```bash
./target/release/reasondb config init   # set LLM provider and API key once
./target/release/reasondb serve
```

Or use the installed binary if you ran `cargo install --path crates/reasondb-cli` and `cargo install --path crates/reasondb-server`.

### Running tests

From the repository root:

```bash
cargo test --workspace --exclude reasondb
```

CI runs the same test command. Fix any failing tests before submitting a PR.

### Optional: Docker for development

To run the server in Docker (e.g. to test the image or use a consistent environment):

```bash
# From repo root, with .env containing REASONDB_LLM_PROVIDER and REASONDB_LLM_API_KEY
make docker-up        # build and start
make docker-up-d      # background
make docker-logs      # follow logs
make docker-down      # stop
```

See the root [docker-compose.yml](docker-compose.yml). For a **consumer-style** run (no build), use the published image and a minimal compose as in the [Quick Start](https://reason-db.devdoc.sh/documentation/page/quickstart) docs.

### Optional: Desktop client (Tauri)

If you work on the ReasonDB Client app:

```bash
make client-install      # npm install in apps/reasondb-client
make client-dev          # web dev server
make client-app          # Tauri desktop app (dev)
make client-app-build    # production build
```

See [apps/reasondb-client/README.md](apps/reasondb-client/README.md) for client-specific tests and scripts.

## Submitting changes

1. **Open an issue** (optional but helpful) — For bugs or features, an issue helps align on scope before you invest in a patch.
2. **Fork and branch** — Create a branch from `main` (e.g. `fix/issue-123` or `feat/your-feature`).
3. **Make your changes** — Keep commits focused and messages clear.
4. **Run tests** — `cargo test --workspace --exclude reasondb` must pass.
5. **Open a pull request** — Target `main`. Describe what changed and why; link any related issue.

We’ll review as soon as we can. Small, focused PRs are easier to merge.

## Code and conventions

- **Rust** — Format with `cargo fmt`, check with `cargo clippy`. The project uses the default Rust style.
- **Docs** — User-facing docs live under `docs/` (MDX). Update the relevant guide or API reference when behavior or config changes.
- **API and config** — Avoid breaking changes when possible. New optional fields and new endpoints are preferred over changing existing contracts.

## Reporting issues

Use [GitHub Issues](https://github.com/reasondb/reasondb/issues) for:

- Bug reports — Include version, OS, steps to reproduce, and logs if relevant.
- Feature ideas — Describe the use case and desired behavior.

## Documentation and resources

- [Full documentation](https://reason-db.devdoc.sh)
- [Quick Start](https://reason-db.devdoc.sh/documentation/page/quickstart) — Running ReasonDB (Docker, Homebrew, from source)
- [API Reference](https://reason-db.devdoc.sh/api-reference/page/api-reference/introduction) — REST API and OpenAPI

Thank you for contributing to ReasonDB.
