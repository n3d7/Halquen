# Halquen

Halquen is a Linux-first deterministic core for capability-safe local automation. The current
milestone implements typed actions, action-bound policy authorization, deadline-bounded dry-run
execution, versioned Unix-socket IPC, append-oriented audit records, evidence-backed linear memory
revisions, and SQLite persistence.

The central security rule is: model output and external content are advice, while registered
capabilities, deterministic policy, and trusted evidence are authority. No model provider, plugin
loader, network service, or arbitrary shell execution exists in this milestone.

## Run locally

The daemon requires a private `XDG_RUNTIME_DIR`. On a normal Linux desktop this is already set.

```bash
cargo run -p halquen-daemon
```

In another terminal:

```bash
cargo run -p halquen-cli --bin halquen -- health
cargo run -p halquen-cli --bin halquen -- capabilities list
cargo run -p halquen-cli --bin halquen -- capability get system.open_app
cargo run -p halquen-cli --bin halquen -- evaluate open-app app:telegram
cargo run -p halquen-cli --bin halquen -- dry-run open-app app:telegram
cargo run -p halquen-cli --bin halquen -- memory stats
cargo run -p halquen-cli --bin halquen -- audit stats
```

`system.open_app` is classified as a non-reversible local side effect. `dry-run open-app` records a
simulated result; it does not launch an application.

Persistent data is stored at `$XDG_DATA_HOME/halquen/halquen.sqlite3`, falling back to
`$HOME/.local/share/halquen/halquen.sqlite3`. Runtime IPC uses
`$XDG_RUNTIME_DIR/halquen/halquen.sock` and never opens a TCP port.

## Verification

```bash
cargo check --workspace --all-targets --all-features --locked
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
cargo audit
cargo deny check
```

`cargo fmt`, `cargo clippy`, `cargo audit`, and `cargo deny` require their respective tools to be
installed; the project does not install global tools automatically. The `permission_grants` schema
is reserved for a future design and no persistent grant is currently loaded or honored.

See [the core walkthrough](docs/core-walkthrough.md), [architecture](docs/architecture.md), and
[security boundaries](docs/security.md) for implementation details and current limitations.
