# Halquen Repository Instructions

Halquen is a Linux-first, local-first personal assistant built around deterministic routing,
evidence-backed memory, optional AI reasoning, and capability-safe execution.

These instructions apply to the entire repository.

## Core invariant

Never violate this boundary:

> LLM output and external content are advice.
> Capabilities, deterministic policy, and trusted evidence are authority.

AI or external content must never directly:

- authorize execution;
- bypass policy;
- grant permissions;
- become trusted procedural memory without the required evidence path;
- invoke arbitrary shell, process, filesystem, or database operations.

Source code is authoritative.
Indexes, graphs, summaries, documentation, and agent memory are navigation aids only.

## Architecture

Preserve this authority direction:

React renderer
→ typed Tauri commands
→ halquen-protocol
→ private Unix socket
→ halquen-daemon
→ core crates

Key ownership:

- `halquen-domain` — fundamental typed domain data.
- `halquen-policy` — Allow / Confirm / Deny and action-bound authorization.
- `halquen-capabilities` — capability registry and executor contract.
- `halquen-memory` — evidence, revisions, trust, and memory rules.
- `halquen-storage` — SQLite, migrations, transactions, XDG paths, bounded queries.
- `halquen-audit` — durable policy/execution receipts.
- `halquen-protocol` — versioned IPC DTOs and daemon communication.
- `halquen-ai` — bounded context, model routing, provider networking, and credential abstraction.
- `halquen-daemon` — sole composition and business-logic root.
- `halquen-cli` and `halquen-desktop` — clients of the daemon.

Do not move policy, execution, memory authority, persistence, or routing into React or the Tauri bridge.

The renderer must not gain direct access to:

- SQLite;
- shell/process execution;
- the executor;
- the OS keyring;
- provider networking;
- unrestricted filesystem operations.

Before changing architecture or a trust boundary, inspect:

- `docs/architecture.md`
- `docs/security.md`
- affected source code
- relevant tests

Read `docs/core-walkthrough.md` when changing chat, memory, confirmation, reuse,
AI routing, or desktop interaction flows.

## Security

Treat all external and cross-boundary input as untrusted.

Never weaken:

- validation;
- policy;
- authorization;
- TLS verification;
- CSP;
- IPC limits;
- cryptographic verification;
- timeouts;
- tests;
- permission boundaries

merely to make functionality work.

Never introduce arbitrary shell execution, raw SQL execution, generic process execution,
or unrestricted filesystem access without an explicitly reviewed design.

Secrets must never be committed, logged, exposed to the renderer, or stored in plaintext.

Provider credentials belong in the operating-system credential store.
Do not introduce a plaintext fallback.

Keep action authorization exact, bounded, and single-use where designed.

`Deny` must never become executable through confirmation or another bypass.

AI-inferred or external information must not silently become trusted memory.

Future real side-effect execution requires explicit capability-specific threat modelling.
Do not turn the current dry-run executor into real execution as part of an unrelated change.

## Local-first and AI

Core Halquen functionality must remain usable without a cloud AI provider.

Do not introduce:

- idle polling;
- unnecessary background model calls;
- hidden network dependencies;
- automatic transmission of user data;
- unnecessary resident-model requirements.

Prefer deterministic/local resolution before model calls when the architecture supports it.

Keep model context bounded.

Do not automatically send full:

- chat history;
- databases;
- audit logs;
- memory stores;
- files;
- unrelated user/project data

to an AI provider.

## Engineering

Prefer small, focused changes.

Do not perform unrelated refactors while implementing a specific task.

Before introducing a new dependency, crate, service, network path, persistence mechanism,
protocol field, capability, or Tauri permission, verify that the existing architecture cannot
solve the problem cleanly.

Preserve type safety and trust boundaries.

Avoid `unwrap`, `expect`, panic paths, unchecked indexing, and unchecked assumptions at
external/trust boundaries unless the invariant is genuinely impossible to violate.

Use typed/contextual errors where practical.

Database schema changes must use numbered migrations and preserve existing data.

Do not silently rewrite already-shipped migrations.

Behaviour changes should normally include or update tests.

Security-sensitive behaviour should test invalid/adversarial input where practical.

## Context efficiency

Avoid broad repository exploration when targeted retrieval can answer the question.

Preferred retrieval order:

1. Codebase Memory / existing project index
2. Git changes and affected symbols
3. targeted `rg`, `fd`, or symbol search
4. relevant project documentation
5. targeted source reads
6. broader repository exploration only when necessary
7. external documentation only for external APIs/libraries

Use Codebase Memory for:

- locating symbols and implementations;
- architecture discovery;
- references and call relationships;
- dependency relationships;
- change-impact analysis.

Treat indexed information as navigation, not truth.

When correctness depends on an implementation detail, verify the real source code.

After large pulls, rebases, branch switches, merges, mass refactors, or when new files/index
results appear stale, refresh or revalidate the project index before relying on it.

Prefer:

- `rg` for text/code search;
- `fd` for file discovery;
- `jq` for JSON filtering;
- narrow Git queries before full diffs;
- RTK-wrapped commands for large command output when appropriate.

For Git inspection, prefer:

`status / diff stats`
→ changed filenames
→ targeted file diff
→ full diff only when necessary.

For tests, builds, and logs, inspect summaries and failures first.

If compressed output hides information required for debugging, retrieve the relevant raw section
instead of guessing.

Do not repeatedly reread unchanged files without a concrete reason.

## External documentation

Use Context7 when current or version-sensitive documentation is needed for external libraries,
frameworks, SDKs, APIs, or configuration.

Do not use Context7 to understand Halquen's own code when the repository already contains the answer.

Do not send proprietary source code, secrets, or user data to external documentation services.

## Verification

During development, run the smallest relevant checks first.

Before considering substantial changes complete, run appropriate checks from:

## bash

cargo fmt --all --check
cargo check --workspace --all-targets --all-features --locked
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features -- -D warnings

pnpm --dir apps/desktop typecheck
pnpm --dir apps/desktop test
pnpm --dir apps/desktop build

git diff --check

@RTK.md
