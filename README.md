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
- OS-keyring secret storage, typed SQLite settings, schema-v4 migrations, usage counters, activity,
  structured audit records, and bounded operational logs.
- typed action provenance, resource/data classification, deterministic immutable hard-deny rules,
  scoped persistent permissions, and a Security/Permissions/Agents desktop control plane;
- bounded recency-weighted application behaviour with correction-aware contextual resolution;
- a daemon-brokered Agent Host with typed identities/sessions, safe capability discovery, bounded
  two-phase JSON, deadlines, resource limits, and a fail-closed Bubblewrap backend;
- an explicit real-execution mode whose only real capability is daemon-registered
  `system.open_app`; the default mode remains dry-run.

The invariant is:

```text
LLM output is advice.
Capabilities, policy, and trusted memory are authority.
```

## Release artifacts

Halquen v0.1.0 is a Linux x86-64 pre-release. The portable archive contains the CLI and daemon:

- `halquen-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`
- `SHA256SUMS`

Verify and install it without root privileges:

```bash
sha256sum -c SHA256SUMS
tar -xzf halquen-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
install -Dm755 halquen-v0.1.0-x86_64-unknown-linux-gnu/halquen ~/.local/bin/halquen
install -Dm755 halquen-v0.1.0-x86_64-unknown-linux-gnu/halquen-daemon ~/.local/bin/halquen-daemon
```

Desktop bundles are published as separate AppImage, Debian, or RPM assets when the corresponding
Tauri packager succeeds. The desktop is a client, not a daemon supervisor: start
`halquen-daemon` separately before using either the desktop or CLI. This release does not install
a system service, enable autostart, or provide an automatic updater.

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
cargo run -p halquen-cli --bin halquen -- capabilities list
cargo run -p halquen-cli --bin halquen -- applications list
cargo run -p halquen-cli --bin halquen -- agents sessions
```

The daemon starts in dry-run mode. `dry-run open-app` is always simulated, even when the daemon was
started for real execution. Real execution must be selected explicitly:

```bash
cargo run -p halquen-daemon -- --execution-mode real
cargo run -p halquen-cli --bin halquen -- applications register app:safe-fixture SafeFixture /usr/bin/true
cargo run -p halquen-cli --bin halquen -- execute open-app app:safe-fixture
```

The real executor accepts only a registered application entity, revalidates its stored executable
identity, and directly spawns the registered path and fixed arguments without a shell. AI and agents
cannot supply executable paths. Filesystem, network, database, messaging, and generic process
execution capabilities are not exposed.

Configured CLI agents run only through the daemon broker. Their output is proposal data with no
authority; every proposal re-enters normal policy and execution. Bubblewrap is the supported sandbox
and missing support fails closed. Unsafe unsandboxed agents require the separate
`--allow-unsafe-agents` daemon flag. Agent sessions are currently one-shot: a confirm-required
proposal returns a safe result but is not kept alive for interactive confirmation; create an exact
user grant and run the agent again.

For a safe manual end-to-end check, use `/usr/bin/true` as shown above: verify `Succeeded`, then run
`audit stats`. While the daemon remains in real mode, `dry-run open-app app:safe-fixture` must still
report `DryRunSucceeded`. In the desktop Security screen, create an exact `Deny` permission for
`app:safe-fixture`; another `execute` must report `Deny`/`NotExecuted`, produce no process launch,
and add the policy-denial audit lifecycle. Revoke the fixture permission when finished.

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
