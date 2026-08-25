# Security boundaries

This milestone establishes security invariants; it is not claimed to be production-ready or fully
secure.

## Authority boundaries

- Capability descriptors are registered from trusted code. Duplicate IDs are rejected and IDs must
  follow lowercase `namespace.operation` syntax.
- Actions contain typed arguments. There is no shell, command, script, eval, or generic file-execute
  capability.
- Policy is independent of interpretation confidence. Read-only, non-reversible local side effects,
  and configured genuinely reversible local writes may be allowed; external and destructive actions
  require confirmation; privileged and unknown risk are denied. Invalid risk/side-effect/
  reversibility combinations are rejected at descriptor registration and policy evaluation.
- An executor requires a non-clone `ExecutionAuthorization` produced only by an `Allow` decision.
  It owns the exact typed action, trusted descriptor/version, execution ID, and normalized scope
  context, and execution consumes it.
- `ExternalContent`, `AiInferred`, plugin assertions, local verification, and observed behaviour do
  not independently authorize procedural memory. SQLite resolves only the revision's referenced
  evidence IDs inside the write transaction; unrelated supplied evidence is rejected and cannot
  lend authority. The preliminary promotion validator applies the same exact-ID binding before a
  candidate can reach policy review. Repeated AI evidence therefore cannot bootstrap its own
  authority.
- AI proposals remain separate, typed, pending records until a future deterministic validation and
  user-authorized acceptance path exists.

## IPC and filesystem

The daemon binds only `$XDG_RUNTIME_DIR/halquen/halquen.sock`. The runtime root and application
directory must be absolute, real, owned by the current UID, and inaccessible to group/other users.
The socket is set to `0600`. Existing symlinks and non-socket objects are never replaced. If a secure
runtime directory is unavailable, startup fails closed.

Persistent data uses XDG data paths. The Halquen directory is `0700` and the SQLite file is `0600`.
The runtime socket is deliberately kept outside persistent storage.

IPC uses one newline-terminated JSON frame per connection, capped at 64 KiB with five-second I/O
timeouts. EOF without the terminator, partial disconnects, trailing messages, malformed JSON,
oversized frames, and unknown versions fail cleanly. Request IDs are validated.

## Database and audit

All value-bearing SQL operations use bound parameters. Dynamic table selection is limited to a
closed internal match. Foreign keys are verified active; multi-row execution and memory writes are
transactional. Persistent databases verify WAL and connections use a bounded busy timeout.
Migration failure is returned without recreating the database.

Memory persistence treats the stored kind and head as authoritative. It rejects first revisions with
a predecessor, stale branches, cross-item predecessors, duplicate evidence references, caller kind
mutation, and head changes detected by a guarded update.

Receipts contain IDs, policy, status, timing, reversibility, result/error codes, and sanitized errors.
Audit records do not contain full request bodies, action arguments, secrets, file contents,
clipboard data, or prompt context. The storage API exposes insertion but no audit update/delete, and
duplicate IDs cannot overwrite an existing record. This is append-oriented application semantics,
not cryptographic immutability or tamper evidence. Memory rollback is restoration through a new
revision. External actions are not falsely labelled rollbackable.

## Current limitations

- Only dry-run execution exists; application launching is intentionally not implemented.
- Async deadlines cancel cooperative executor futures by dropping them. A future real executor must
  not hide irreversible blocking work behind an uncancellable `spawn_blocking` task.
- The daemon handles one local client at a time. Timeouts bound a stalled connection, but per-user
  denial of service has not been fully hardened.
- There is no authentication beyond local Unix ownership and permissions, and no OS credential-store
  integration because the core stores no secrets.
- Linux `/proc/self/status` is used to determine the current UID. Other operating systems are not yet
  supported.
- Filesystem ownership/mode checks reduce symlink substitution but are path-based and do not claim
  race-free `openat2`-style resolution.
- `permission_grants` is reserved schema only. There is no API to create/revoke persistent grants,
  and policy does not load or honor rows from that table.
- Plugin, MCP, model-provider, GUI, voice, browser, and synchronization layers are absent.
