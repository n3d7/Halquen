# Architecture

Halquen separates the renderer, desktop bridge, daemon, and authority-bearing core.

```text
React renderer
    │ typed invoke commands only
    ▼
Tauri Rust bridge
    │ halquen-protocol v5
    ▼
private Unix socket
    ▼
halquen-daemon
    ├── local chat resolver / response reuse
    ├── model router / AI gateway
    ├── provenance / resource labels / data-flow policy / capabilities
    ├── application registry / agent broker / typed runtime sessions
    ├── scoped grants / recent behaviour / agent configuration
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
- `halquen-capabilities` owns the deterministic registry, trusted application registry, executable
  identity inspection, the permanently available dry-run executor, and the narrowly scoped Linux
  real executor.
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

Contextual phrases such as “open that messenger” use bounded `intent_usage_events` before any AI
fallback. The scorer applies exponential half-life decay, strong correction signals, an absolute
confidence threshold, and a minimum top-two gap. An ambiguous result returns a clarification route;
behaviour is never copied into `PolicyContext` or a permission grant.

## Action authority lifecycle

The authority-bearing path is now:

```text
trusted request boundary
  → typed ActionProposal(ActionRequest + ActionContext)
  → bounded provenance validation
  → resource-label classification
  → immutable data-flow/resource rules
  → exact deny/allow grants
  → baseline/profile policy
  → optional concrete confirmation
  → exact non-clone ExecutionAuthorization
  → executor
```

`ActionContext` separates origin, trust, authority, typed provenance hops, resources and optional
data flow. The daemon assigns the `UserExplicit → LocalResolver` chain for locally parsed user
requests. AI/agent/plugin/external constructors produce `AuthorityClass::None`; wire DTOs for
permission creation contain no `granted_by` or authority field.

The precedence is immutable hard deny, exact deny, ordinary deny, confirmation, then allow. Exact
persistent grants match capability ID, typed arguments, resource descriptors and destination. A
grant for Telegram cannot match Discord. Once grants are consumed transactionally before execution;
session and expiry are checked by the daemon. Agent grants additionally bind `AgentId` and, for a
session lifetime, the exact `AgentSessionId`; they do not match local/user proposals.

`DryRunExecutor` remains separate and powers every explicit dry-run request. The daemon defaults to
dry-run mode. In explicit real mode, `RealLinuxExecutor` implements only `system.open_app`: an
`app:*` entity is resolved through the daemon-owned trusted registry, its recorded executable
identity is revalidated, and its fixed executable/arguments are spawned directly without a shell.
No path or argument supplied by AI/agent output is accepted by that registry.

## Agent Host

`halquen-ai::AgentHost` is connected to the daemon through a narrow two-phase broker protocol. The
daemon sends safe capability descriptions plus an `AgentId / AgentInstanceId / AgentSessionId`
identity; the subprocess returns typed proposals; the daemon evaluates and executes each proposal;
then the subprocess receives only structured dispositions and safe IDs. Agent output never contains
an `ActionContext` or authorization field: the host constructs `Agent` provenance with no authority.

The host keeps executable and arguments separate, clears the environment, bounds aggregate
stdin/stdout/stderr, validates strict JSON, enforces one wall deadline, and explicitly kills and
reaps failed or timed-out children. The Linux Bubblewrap backend unshares namespaces/network,
exposes read-only system runtime paths plus isolated `/tmp`, and does not mount the user home or
Halquen socket. A trusted `prlimit` wrapper applies CPU, address-space, process-count, file-size, and
open-file limits. Missing sandbox support fails closed; unsafe execution needs daemon startup opt-in.

`AgentId` names a stored configuration. `AgentInstanceId` identifies one spawned process.
`AgentSessionId` scopes one broker run and session grants. `DaemonSessionId` identifies one daemon
lifetime; startup marks unfinished prior agent sessions crashed and revokes stale agent/daemon
session grants. CLI, Tauri, and the Security screen expose agent runs and recent sessions. The CLI
transport is implemented; Unix-socket agent transport remains typed unsupported. Broker runs are
one-shot, so confirm-required proposals currently need an exact user grant followed by a rerun.

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

Migration `0003_authority_layer.sql` extends permission grants and adds security configuration,
resource labels, bounded intent-usage events, and agent configuration. Existing migrations remain
unchanged.

Migration `0004_real_execution_and_agent_broker.sql` adds trusted applications, executable identity
and resource-limit fields, typed daemon/agent sessions, and agent-bound permission scope. Existing
migrations remain unchanged.

Operational logs, activity events, and audit records are distinct:

- logs diagnose the process and rotate under XDG state limits;
- activity explains user-visible route/policy/memory facts without chain of thought;
- audit stores durable security/execution lifecycle records.

The current-thread daemon is event-driven and performs no idle polling or background model calls.
Socket connections are handled as concurrent Tokio tasks while authority-bearing service operations
remain serialized. A typed cancellation request bypasses that service lock, signals only its exact
active chat request ID, and causes the pending provider future to be dropped before its result can be
used.
