# Core architecture

Halquen's implemented core is a directed acyclic workspace. The domain crate is the lowest layer;
the daemon is the composition root.

```text
halquen-domain
├── halquen-policy
├── halquen-memory
├── halquen-audit ──────────────→ halquen-policy
├── halquen-capabilities ───────→ halquen-policy
├── halquen-storage ────────────→ halquen-memory + halquen-audit
└── halquen-protocol ───────────→ halquen-policy + halquen-audit

halquen-daemon ─────────────────→ all core crates
halquen-cli ────────────────────→ halquen-domain + halquen-protocol
```

There are no circular crate dependencies. `halquen-domain` has no Tokio, SQLite, networking, UI, or
operating-system execution code.

## Request path

The CLI serializes one versioned, request-ID-bearing JSON frame and connects to a private Unix
socket. The daemon validates the bounded frame, resolves the typed capability ID in a deterministic
registry, checks argument shape, and asks the policy engine for `Allow`, `Confirm`, or `Deny`.

Only `Allow` includes an `ExecutionAuthorization`. Policy creates it only after checking the exact
typed action against the trusted descriptor. The non-clone token owns the execution ID, complete
descriptor/version, exact action arguments, and normalized policy scopes. Executor ownership
consumes it, so a token cannot be reused through the API or paired with another request. The only
executor is `DryRunExecutor`; it performs no operating-system action.

Execution is an async, cooperatively cancellable future wrapped in a Tokio deadline derived from the
trusted descriptor. `Confirm` and `Deny` produce no token and therefore never emit
`ExecutionStarted`. Allowed runs emit `ActionRequested`, `PolicyEvaluated`, `ExecutionStarted`, and
one terminal completion/failure/timeout event. Blocked runs emit a confirmation or denial event.

## Persistence and concurrency

SQLite is owned by the single-threaded daemon service. Connections explicitly enable foreign keys
and a five-second busy timeout. Persistent databases request and verify WAL mode. Migrations are
ordered, transactional, and recorded in `schema_migrations`.

The Tokio runtime is current-thread and event-driven. The daemon waits on socket acceptance or
shutdown, with no polling loop or background worker. Requests are handled sequentially, keeping
SQLite ownership and audit ordering straightforward.

## Memory

Working memory is bounded and in-process. Semantic and procedural memory use immutable revisions,
explicit evidence links, temporal validity fields, and a current-revision pointer. `MemoryValue`
derives its kind; in-memory and SQLite trust boundaries reject any caller-supplied kind mismatch.

SQLite resolves exactly the evidence IDs referenced by the new revision inside the same transaction
as the write. Procedural authority is computed only from that resolved set. Existing stored kind,
creation timestamp, and head are authoritative. A successor must name the current head, the head
must belong to the same item, and the final head update is compare-and-swap guarded. Restoring an old
value creates a new successor rather than deleting or rewinding history.

AI proposals, unknown cases, and corrections have typed domain representations and persistent
tables. They do not execute foreground requests and AI proposals cannot mutate trusted memory.
