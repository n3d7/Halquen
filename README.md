# Halquen

Halquen is a Linux-first, local-first personal assistant and authority layer for AI-assisted
workflows. Routing, memory, policy, and execution remain deterministic and local. Optional AI
providers can help with novel requests, but model output is never permission, trusted memory, or
execution authority.

Halquen is currently an early `v0.1.0` pre-release. The implemented scope is intentionally narrow:
the daemon defaults to dry-run, and the only real side effect is opening a daemon-registered
application.

```text
LLM output and external content are advice.
Capabilities, deterministic policy, and trusted evidence are authority.
```

## What is implemented

- A versioned JSON protocol over a private Unix socket, shared by the desktop and CLI clients.
- Deterministic chat routes for application intents, explicit memory operations, corrections, and
  exact feedback-approved response reuse before any model call.
- Evidence-backed memory with immutable revisions, explicit trust classes, guarded heads, history,
  and restoration without deleting prior revisions.
- Typed capability proposals, resource classification, `Allow / Confirm / Deny` policy, exact
  scoped grants, expiring single-use confirmations, and durable execution audit records.
- A permanent dry-run executor plus an opt-in Linux real executor limited to `system.open_app` for
  registered applications. Executables and fixed arguments are daemon-owned; no shell is used.
- Optional OpenAI-compatible AI routing for OpenAI, Ollama, LM Studio, and custom compatible
  endpoints, with bounded context, privacy checks, model-call budgets, and OS-keyring credentials.
- A daemon-brokered CLI Agent Host with typed identities and sessions, bounded two-phase JSON,
  resource limits, and a fail-closed Bubblewrap sandbox.
- A Tauri v2 and React desktop client with Chat, Activity, Memory, AI, Diagnostics, Security, and
  Settings screens, plus a daemon client CLI.
- SQLite persistence, numbered migrations, XDG paths, activity and usage records, and bounded
  operational logging.

Core chat, memory, policy, dry-run, audit, CLI, and desktop flows remain usable without an AI
provider. Halquen performs no idle provider polling or background model calls.

## Architecture

```text
React renderer
    │ typed Tauri commands
    ▼
Tauri Rust bridge
    │ halquen-protocol v5
    ▼
private Unix socket
    ▼
halquen-daemon
    ├── deterministic chat and response reuse
    ├── memory, policy, capabilities, and audit
    ├── optional model routing and provider access
    ├── trusted application registry and Agent Host broker
    └── SQLite, XDG paths, logging, and OS keyring
```

The daemon is the sole composition and business-logic root. The React renderer and Tauri bridge are
clients; they do not own policy, persistence, provider networking, or execution.

| Component | Responsibility |
| --- | --- |
| `apps/daemon` | IPC server, deterministic routing, orchestration, policy/execution lifecycle |
| `apps/cli` | Typed command-line client for the daemon |
| `apps/desktop` | React UI and narrow Tauri-to-protocol bridge |
| `halquen-domain` | Fundamental IDs and typed domain data |
| `halquen-policy` | Policy decisions and exact action-bound authorization |
| `halquen-capabilities` | Capability and application registries, dry-run and real executors |
| `halquen-memory` | Evidence, immutable revisions, trust, and promotion rules |
| `halquen-storage` | SQLite, migrations, transactions, bounded queries, and XDG paths |
| `halquen-audit` | Typed policy and execution receipts |
| `halquen-protocol` | Protocol v5 DTOs, bounded framing, socket discovery, and daemon client |
| `halquen-ai` | Context and prompt bounds, model routing, provider networking, keyring, Agent Host |

See [Architecture](docs/architecture.md) and the [Core walkthrough](docs/core-walkthrough.md) for
the detailed request and authority flows.

## Security model

- AI, agents, plugins, and external content produce untrusted data or proposals. They cannot create
  authorization, grant permissions, or directly promote trusted procedural memory.
- Only `halquen-policy` can construct an execution authorization, bound to the exact normalized
  action. `Deny` cannot be overridden through confirmation or a persistent grant.
- The renderer has no direct SQLite, filesystem, shell, executor, keyring, or provider-network
  access. Tauri exposes only typed daemon requests.
- IPC uses a `0600` Unix socket under a user-owned `0700` runtime directory. No TCP control listener
  is opened.
- Provider credentials are stored in the operating-system credential service. SQLite stores only
  opaque credential IDs, and there is no plaintext fallback.
- Real execution is default-off and limited to registered `system.open_app` targets. The executor
  revalidates the stored executable identity and spawns it directly without a shell.
- Agent proposals return to normal daemon policy and audit. Bubblewrap isolation fails closed unless
  unsafe unsandboxed agents are explicitly enabled at daemon startup.

These controls define implemented boundaries, not a claim of complete system security. Read
[Security boundaries](docs/security.md) before enabling real execution or external agents.

## Quick start

### Prerequisites

- Linux with Unix sockets. A desktop credential service is required only for stored AI provider
  credentials.
- Rust `1.97.1` (pinned by `rust-toolchain.toml`).
- Node.js `24` and pnpm `11.19.0` for the desktop client.
- The normal Tauri v2 Linux build dependencies. On Ubuntu 22.04:

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential curl file libayatana-appindicator3-dev librsvg2-dev \
  libssl-dev libwebkit2gtk-4.1-dev libxdo-dev patchelf wget xdg-utils
npm install --global pnpm@11.19.0
pnpm --dir apps/desktop install --frozen-lockfile
```

### Run the daemon and clients

Start the daemon in its default dry-run mode:

```bash
cargo run -p halquen-daemon
```

In another terminal, use the CLI:

```bash
cargo run -p halquen-cli --bin halquen -- health
cargo run -p halquen-cli --bin halquen -- capabilities list
cargo run -p halquen-cli --bin halquen -- chat "Open Telegram"
```

Or start the desktop client while the daemon is running:

```bash
pnpm --dir apps/desktop tauri dev
```

AI is optional and is configured from the desktop AI screen. The daemon must be running separately;
the desktop and CLI do not install or supervise it.

### Opt in to real application launch

Real execution is an explicit daemon startup choice. A safe smoke-test target is `/usr/bin/true`:

```bash
cargo run -p halquen-daemon -- --execution-mode real
```

Then, from another terminal:

```bash
cargo run -p halquen-cli --bin halquen -- \
  applications register app:safe-fixture SafeFixture /usr/bin/true
cargo run -p halquen-cli --bin halquen -- execute open-app app:safe-fixture
```

This mode does not enable arbitrary commands. Only registered application entities can reach the
real executor; AI and agents cannot supply executable paths or arguments.

## Build and verification

Build the Rust workspace and desktop application:

```bash
cargo build --release --workspace --locked
pnpm --dir apps/desktop build
pnpm --dir apps/desktop tauri build --no-bundle
```

Run the repository verification gates:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --all-features --locked
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features -- -D warnings
pnpm --dir apps/desktop typecheck
pnpm --dir apps/desktop test
pnpm --dir apps/desktop build
git diff --check
```

Optional dependency-policy tools used for release checks are `cargo audit` and `cargo deny`.

## Local data

- Database: `$XDG_DATA_HOME/halquen/halquen.sqlite3`, falling back to
  `$HOME/.local/share/halquen/halquen.sqlite3`.
- IPC: `$XDG_RUNTIME_DIR/halquen/halquen.sock`.
- Logs: `$XDG_STATE_HOME/halquen/logs`, falling back to `$HOME/.local/state/halquen/logs`.
- Provider credentials: the OS credential service under `halquen.ai-provider`; never plaintext
  SQLite values.

## Current limitations

- Linux and a single-user local deployment are the supported path. Processes running as the same
  user are not separated by additional IPC roles.
- The daemon is managed separately. There is no installer-managed service, autostart integration,
  automatic updater, or application-lifetime supervision after a successful launch.
- Real execution supports only registered `system.open_app`. There are no generic shell, process,
  filesystem, network, database, or messaging executors.
- Executable identity checks reduce but do not eliminate the path validation-to-`execve` race.
  Registered applications are trusted, unsandboxed same-user programs.
- Brokered agent runs are one-shot and currently support CLI subprocess transport only. Interactive
  confirmation requires an exact user grant followed by a rerun. Bubblewrap and `prlimit` are
  required for sandboxed agents; the persisted `temp_bytes` value is not yet enforced as a tmpfs
  quota, and no cgroup or seccomp enforcement is claimed.
- Provider responses are non-streaming. Anthropic and Gemini remain typed unsupported boundaries;
  automatic model discovery, embeddings, semantic reuse, and background consolidation are not
  implemented.
- Authority-bearing service operations are serialized. Chat cancellation can bypass the busy
  service lock for its exact request, but other clients may wait behind a provider call.
- The `v0.1.0` Linux x86-64 pre-release provides a portable CLI/daemon archive plus Debian and RPM
  desktop packages. It does not include an AppImage, system service, or updater.
- The current Tauri/WebKitGTK dependency chain includes the documented `RUSTSEC-2024-0429`
  advisory; see [Security boundaries](docs/security.md) for the maintained project position.

## Documentation

- [Architecture](docs/architecture.md)
- [Core walkthrough](docs/core-walkthrough.md)
- [Security boundaries](docs/security.md)
- [Architecture decisions](docs/decisions/)
- [v0.1.0 release notes](docs/releases/v0.1.0.md)

## License

Halquen is licensed under the [Apache License 2.0](LICENSE).
