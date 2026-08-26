# Halquen

Halquen is a Linux-first, local-first personal assistant built around typed knowledge, evidence,
deterministic routing, and capability-safe execution. Its desktop application is a client of the
independently managed daemon; AI providers are optional reasoning backends, not the authority for
actions or trusted memory.

The current milestone includes:

- a Tauri v2 + React/TypeScript desktop client with Chat, Activity, Memory, AI, Diagnostics, and
  Settings screens;
- a versioned Unix-socket protocol shared by the CLI and GUI;
- local-first chat routing, explicit memory commands, single-use confirmation, and conservative
  reusable-response candidates;
- an OpenAI-compatible AI gateway for OpenAI, Ollama, LM Studio, and custom compatible endpoints;
- provider/model routing with cloud/local privacy classes and bounded token/model-call budgets;
- OS-keyring secret storage, typed SQLite settings, schema-v2 migrations, usage counters, activity,
  structured audit records, and bounded operational logs.

The invariant is:

```text
LLM output is advice.
Capabilities, policy, and trusted memory are authority.
```

## Development

Start the daemon in one terminal:

```bash
cargo run -p halquen-daemon
```

Start the desktop client in another:

```bash
pnpm --dir apps/desktop install
pnpm --dir apps/desktop tauri dev
```

The existing CLI remains available:

```bash
cargo run -p halquen-cli --bin halquen -- health
cargo run -p halquen-cli --bin halquen -- chat "Open Telegram"
cargo run -p halquen-cli --bin halquen -- memory stats
cargo run -p halquen-cli --bin halquen -- audit stats
```

`system.open_app` currently uses the safe dry-run executor. Halquen returns and audits a simulated
result; it does not launch an application.

## Local data

- SQLite: `$XDG_DATA_HOME/halquen/halquen.sqlite3`, falling back to
  `$HOME/.local/share/halquen/halquen.sqlite3`.
- IPC: `$XDG_RUNTIME_DIR/halquen/halquen.sock`; no TCP control listener is opened.
- Logs: `$XDG_STATE_HOME/halquen/logs`, falling back to `$HOME/.local/state/halquen/logs`.
- Provider secrets: the operating-system credential service under `halquen.ai-provider`. SQLite
  stores only an opaque credential ID. If the keyring is unavailable, credential operations fail;
  there is no plaintext fallback.

## Verification

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --all-features --locked
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features -- -D warnings
pnpm --dir apps/desktop typecheck
pnpm --dir apps/desktop test
pnpm --dir apps/desktop build
pnpm --dir apps/desktop tauri build --no-bundle
git diff --check
cargo audit
cargo deny check
```

The optional Cargo tools and `rustfmt` must be installed by the development environment; the project
does not install global binaries automatically.

See [architecture](docs/architecture.md), [security](docs/security.md), and the
[core walkthrough](docs/core-walkthrough.md) for the implemented boundaries and known limitations.
