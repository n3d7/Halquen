# ADR 0007: Provenance, resource-aware policy, and fail-closed Agent Host

## Status

Accepted for the authority-layer milestone; the deferred broker connection is completed by ADR
0008.

## Decision

Halquen represents execution candidates as typed `ActionProposal` values containing the existing
typed action plus an `ActionContext`. The context separates origin, trust, authority, bounded
provenance, resources and optional data flow. Only the trusted daemon path assigns user/local
authority; provider and agent output remains proposal-only.

Policy retains `Allow / Confirm / Deny`, adds typed rule metadata, and evaluates immutable hard
rules before exact grants and baseline/profile rules. Exact grants include typed arguments,
resources and destination and are stored by numbered migration.

Recent behaviour is stored separately from trusted memory and contributes only to deterministic
intent ranking. Exponential half-life decay and correction weights are fixed and testable.

External CLI agents use a bounded `tokio::process::Command` adapter without shell interpolation.
Bubblewrap is the supported Linux sandbox backend; absence fails closed. Unsandboxed execution is
explicitly unsafe and refused by the default host. Agent output is bounded typed JSON and all
returned actions receive `Agent` origin with no authority.

## Consequences

- A compromised model can propose a forbidden action but cannot construct authorization.
- Secret-to-external and other immutable rules cannot be bypassed by confirmation or grants.
- Passive usage improves convenience without modifying permissions.
- Agent configuration remains non-authoritative. ADR 0008 connects invocation through a daemon
  broker while preserving the fail-closed sandbox and proposal-only boundary.
