# Architecture

Halquen separates the renderer, desktop bridge, daemon, and authority-bearing core.

```text
React renderer
    │ typed invoke commands only
    ▼
Tauri Rust bridge
    │ halquen-protocol v3
    ▼
private Unix socket
    ▼
halquen-daemon
    ├── local chat resolver / response reuse
    ├── model router / AI gateway
    ├── memory validation / policy / capabilities
    ├── SQLite / audit / activity / usage
    └── OS credential store
```

The renderer has no SQLite, filesystem, shell, process, executor, keyring, or provider-network API.
Tauri commands translate typed GUI calls to the same protocol used by the CLI and contain no policy,
memory, routing, or execution implementation.

## Workspace boundaries

- `halquen-domain` contains identifiers and fundamental action, provider, chat, settings, activity,
  diagnostics, and usage types. It has no async runtime, database, UI, or networking dependency.
- `halquen-policy` produces `Allow`, `Confirm`, or `Deny` and binds a non-clone execution
  authorization to the exact typed action. An explicit confirmation can authorize that action once
  but can never override `Deny`.
- `halquen-capabilities` owns the deterministic registry and executor contract. The current executor
  is dry-run only.
- `halquen-memory` owns evidence, immutable revisions, derived memory kind, and promotion rules.
- `halquen-storage` owns XDG paths, numbered migrations, SQLite transactions, and query limits.
- `halquen-audit` defines durable policy/execution receipts.
- `halquen-protocol` owns versioned IPC DTOs, bounded framing, secure runtime paths, and the shared
  daemon client.
- `halquen-ai` owns bounded context projections, managed prompt composition, deterministic routing,
  provider-neutral requests/responses, OpenAI-compatible HTTP, and the keyring abstraction.
- `halquen-daemon` is the sole composition/business-logic root.
- `halquen-cli` and `halquen-desktop` are clients of the daemon.

Dependencies remain directed toward domain/core crates; the GUI is not a peer authority.

## Chat cascade

The implemented conversational path is:

```text
request validation
  → exact deterministic local parser
  → validated exact response reuse
  → eligible provider/model routing
  → bounded context + managed prompt
  → provider-neutral completion
```

`Open <application>` becomes a typed capability request without AI. Supported explicit
`remember`/alias/preference and `forget` phrases become versioned semantic-memory operations with
`UserExplicit` evidence. Other requests may use AI only if settings, provider/model availability,
privacy class, and manual-selection policy all permit it.

An AI answer is stored as a non-reusable response candidate with `AiInferred` trust. It becomes
eligible for exact normalized reuse only after explicit positive feedback such as “Always reuse”.
It never becomes action authorization or trusted procedural memory.

## Provider and prompt model

Providers and models are separate typed records. The router checks enabled/configured status, task
eligibility, provider/model privacy agreement, cloud/local settings, personal-context policy,
selection mode, preset, default flag, and priority. Manual selection runs through the same checks.

Every model call composes:

1. the immutable Halquen security contract;
2. a versioned task profile and optional output schema;
3. bounded user-editable personal instructions;
4. a bounded structured context projection with explicit untrusted markers;
5. the current request.

Chat history shown in the GUI is not automatically model context. No entire database, audit log, or
conversation archive is sent.

## Operational state

Migration `0002_desktop_interaction.sql` adds explicit settings, provider/model metadata, chat,
response candidates, activity, usage, and memory-state metadata. Credentials are outside SQLite.

Operational logs, activity events, and audit records are distinct:

- logs diagnose the process and rotate under XDG state limits;
- activity explains user-visible route/policy/memory facts without chain of thought;
- audit stores durable security/execution lifecycle records.

The current-thread daemon is event-driven and performs no idle polling or background model calls.
Socket connections are handled as concurrent Tokio tasks while authority-bearing service operations
remain serialized. A typed cancellation request bypasses that service lock, signals only its exact
active chat request ID, and causes the pending provider future to be dropped before its result can be
used.
