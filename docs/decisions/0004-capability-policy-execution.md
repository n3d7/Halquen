# ADR 0004: Capability and policy gated execution

Status: accepted

Every action names a registered, typed capability. Policy evaluates trusted descriptor metadata and
only an `Allow` result contains an authorization token. The non-clone token owns the exact action,
descriptor/version, execution ID, and scope context; the executor consumes it without accepting a
second caller-supplied request. This makes cross-argument and cross-version reuse unavailable through
the API. `Confirm` and `Deny` cannot reach execution.

Executors are cooperative async futures. The daemon wraps the future in the trusted descriptor's
deadline and emits distinct completed, failed, or timed-out audit events. Blocking work that survives
future cancellation is outside this contract and must not be introduced as `spawn_blocking` without
a separately enforceable process boundary. No generic shell capability is permitted.
