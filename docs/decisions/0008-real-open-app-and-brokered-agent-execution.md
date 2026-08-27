# ADR 0008: Narrow real application execution and brokered agents

## Status

Accepted for the real-execution and brokered-agent milestone.

## Context

Halquen needs one useful real side effect and executable external agents without turning model or
subprocess output into authority. A generic command, path, process, filesystem, or network API would
cross that boundary. Process isolation and pathname validation also have Linux-specific limits that
must be stated honestly.

## Decision

The daemon defaults to dry-run and selects real execution only through an explicit startup mode.
`DryRunExecutor` remains independent and handles every explicit dry-run request. The only real
capability is `system.open_app`.

Real application execution resolves an `app:*` entity through a trusted daemon-owned registry. A
registration contains the canonical executable, fixed arguments, an ownership policy, metadata
identity, and optional SHA-256. Registration and pre-spawn checks reject symlink components,
non-regular/non-executable files, unexpected owners, and group/world-writable executables or parent
directories. Execution uses `tokio::process::Command` directly with no shell and a cleared, narrow
environment. AI and agents can propose an entity but cannot provide a path or arguments.

External CLI agents use a daemon-brokered two-phase protocol. The daemon sends bounded input, safe
capability descriptions, and typed `AgentId`, `AgentInstanceId`, and `AgentSessionId`. Strict agent
JSON returns only typed proposals. The host assigns no authority, and every proposal re-enters
resource classification, policy, exact grants, authorization, the selected executor, and audit.
Structured safe results are returned before the child exits. `DaemonSessionId` allows startup to
crash unfinished sessions and revoke stale agent/daemon grants.

Bubblewrap is the enforced Linux backend. It isolates namespaces/network, omits home and the daemon
socket, exposes only read-only runtime paths, and supplies an isolated `/tmp`. A trusted `prlimit`
wrapper applies CPU, address-space, process-count, file-size, and open-file limits. Aggregate output,
input, proposal count, framing, and wall time are bounded, and error/timeout paths kill and reap the
child. Unsafe unsandboxed execution requires a separate daemon startup opt-in.

Derived data inherits the maximum source sensitivity independent of source ordering. Lowering that
classification requires a typed trusted declassification authority; agent/model output cannot
request declassification.

## Confirmation and permission semantics

Confirmation IDs remain daemon-owned, expiring, exact, and single-use. Consumption removes the
pending item before re-policy and execution, so replay cannot execute. `Deny` never becomes
confirmable. Once grants are claimed atomically using an immediate write transaction and conditional
update. Agent grants additionally bind an exact `AgentId`; session grants bind an exact
`AgentSessionId` and are revoked when the session ends or is recovered as crashed.

Broker sessions are currently one-shot. A confirm-required agent proposal returns a structured safe
result but does not retain a live token/process for interactive confirmation. The user must create an
exact agent grant and rerun. This is a deliberate safe scaffold, not a completed persistent-agent
confirmation UX.

## Residual limitations

- Path identity checks reduce executable replacement attacks but do not remove the time-of-check to
  time-of-use window between validation and `execve`. No race-free executable-open claim is made.
- `temp_bytes` is validated and persisted, but the isolated tmpfs does not yet enforce that aggregate
  quota. Memory and per-file-size limits still apply. No cgroup or seccomp enforcement is claimed.
- A launched application is not supervised or cancelled for its complete lifetime.
- Unix-socket agent transport remains typed unsupported.

## Future filesystem capabilities

No filesystem capability is added by this decision. A future capability-specific threat model must
use trusted base directory descriptors and descriptor-relative operations. On Linux, resolution
should use `openat2` with at least `RESOLVE_BENEATH`, `RESOLVE_NO_SYMLINKS`,
`RESOLVE_NO_MAGICLINKS`, and, where the capability requires a single mount, `RESOLVE_NO_XDEV`.
Authorization must bind the base descriptor identity, normalized relative path, operation, flags,
byte/entry limits, and lifetime. It must not accept arbitrary absolute paths or silently fall back to
weaker pathname checks when `openat2` is unavailable. This is design guidance only; it is not
implemented in this milestone.

## Consequences

- A compromised model or agent can propose but cannot authorize or select an executable.
- Real side effects remain narrowly reviewable and default-off.
- Agent lifecycle, permission scope, and audit events have explicit identities.
- The documented scaffolds and Linux race/resource limitations remain visible instead of being
  presented as completed security guarantees.
