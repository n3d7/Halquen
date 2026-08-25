# Deterministic core walkthrough

## Workspace responsibilities

`halquen-domain` defines capability, action, entity, intent, event, proposal, trust, and identifier
types. Its validated newtypes stop malformed IDs at deserialization boundaries, and it has no runtime
or persistence dependencies.

`halquen-policy` maps trusted descriptors to a typed decision and machine-readable reason. An allowed
authorization owns the exact typed action, full descriptor/version, execution ID, and normalized
scope context. It is non-clone and consumed by execution.

`halquen-capabilities` owns the deterministic registry, trusted built-in descriptors, and async
executor contract. Its dry-run executor validates the descriptor/action consistency embedded in the
authorization and performs no OS action.

`halquen-memory` contains bounded working context, semantic/procedural revision logic, episodic
records, evidence, and procedural promotion rules. Restoration creates a new revision.

`halquen-audit` defines durable policy/execution/memory events and execution receipts. These types are
structured rather than free-form log messages.

`halquen-storage` owns XDG data paths, SQLite configuration, migrations, transactional persistence,
and aggregate memory/audit statistics. It also enforces trusted evidence for persistent procedural
memory.

`halquen-protocol` defines protocol version 1, request and response envelopes, frame limits, codecs,
and secure XDG runtime-path validation.

`halquen-daemon` composes these services and owns all business decisions. `halquen-cli` only parses a
small command surface, exchanges one frame, and displays the typed response.

## One request end to end

For `halquen dry-run open-app app:telegram`:

1. The CLI creates `ActionRequest { capability_id: system.open_app, arguments: OpenApp(...) }`.
2. It wraps the action with protocol version 1 and a request ID, encodes at most 64 KiB, and connects
   to the private Unix socket.
3. The daemon bounds and parses the frame, then looks up `system.open_app` in `CapabilityRegistry`.
4. The action's `OpenApp` argument kind is checked against the trusted descriptor.
5. `PolicyEngine` evaluates the descriptor's trusted risk and binds an `Allow` token to this exact
   request and scope context; `Confirm` and `Deny` do not receive a token.
6. The daemon consumes the token in `DryRunExecutor` under the descriptor's deadline. It returns the
   safe result code `Simulated` and performs no OS operation.
7. The daemon builds an `ExecutionReceipt` and explicit requested/policy/started/terminal audit
   lifecycle.
8. SQLite inserts the receipt and all audit events in one transaction.
9. The daemon returns the decision and receipt through the versioned protocol; the CLI displays IDs
   and status only.

For `Confirm` or `Deny`, step 6 is skipped, no `ExecutionStarted` event exists, a `NotExecuted`
receipt is recorded, and the response returns immediately. An unknown interactive request is
unsupported or becomes an explicit learning
queue item through a future caller; it never waits silently for batch execution.

## Memory changes

An accepted memory change carries unique evidence IDs and creates an immutable revision. The memory
item points to its newest revision while previous revisions remain addressable. Persistence resolves
exactly those IDs inside the transaction and requires every successor to extend the stored current
head. Procedural candidates also name their exact evidence IDs before policy review. Persistent
procedural memory additionally requires referenced `UserExplicit` or
`UserConfirmedResult` evidence. AI inference, external content, and unrelated trusted evidence
cannot cross this authority boundary.

## Intentionally absent

The core has no model SDK, inference runtime, embeddings, graph/vector database, arbitrary shell,
real OS automation, plugin loader, MCP, network listener, GUI, voice, clipboard monitoring, or secret
storage. Interfaces were added only for boundaries exercised by the deterministic core.
