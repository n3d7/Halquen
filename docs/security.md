# Security boundaries

This document describes implemented controls, not a claim of complete system security.

## Authority

- Model text and external content are untrusted data. They cannot create an execution token, modify
  trusted memory, alter policy, grant permission, or invoke shell/process APIs.
- Capabilities are registered typed descriptors. There is no arbitrary shell, script, raw SQL, raw
  provider request, or generic file-execute protocol command.
- The executor consumes a non-clone authorization bound to the exact action, descriptor/version,
  execution ID, and normalized scope. `Confirm` creates a server-side, expiring, single-use token;
  dismissing/cancelling is not approval. `Deny` still cannot reach the executor.
- `MemoryValue` derives its authoritative kind. SQLite resolves exactly a revision's referenced
  evidence inside the write transaction. Missing, duplicate, unrelated, cross-item, stale-head, and
  kind-spoofing cases fail. `AiInferred`, `ExternalContent`, plugin assertions, local verification,
  and passive behaviour cannot independently authorize procedural memory.
- Reusable conversational responses remain separate from semantic/procedural memory and never carry
  execution authorization. AI candidates require explicit feedback before reuse and have bounded
  confidence, priority, trust, and validity fields.
- Every action proposal carries typed, bounded provenance. `UserExplicit` is assigned only by the
  trusted daemon path; AI, external content, plugins and agents are represented with no authority.
- Resource and data classifications are evaluated with origin, typed action arguments and
  destination. Immutable rules deny secret-to-external flow, destructive production actions,
  dangerous system-critical actions, and untrusted policy/permission mutation. Confirmation and
  persistent grants cannot override these rules.
- Persistent grants are exact-action/resource/destination scoped, revocable, expiring or
  session-bound, and visible in the desktop Security screen. Passive behaviour never creates one.
- `ExecutionAuthorization` can be constructed only inside `halquen-policy`. The daemon records
  authorization creation separately from proposal, policy, confirmation, execution start, and
  terminal outcome. Agent identity/session metadata is sanitized into lifecycle events without raw
  prompts, arguments, executable paths, or subprocess output.

## Threat model

Halquen assumes the model may be malicious or fully compromised, external content may contain
prompt injection, plugins may be untrusted, and an external agent subprocess may be hostile. Model
text is therefore parsed only as data/proposals; it never becomes `ExecutionAuthorization`.

Prompt-injection protection relies on the authority boundary, not a fallible injection classifier.
For example, an external-content proposal carrying `Secret → UntrustedExternal` is hard-denied even
after an attempted human-confirmation path.

## Renderer and IPC

- The WebView loads only bundled local content. CSP permits only `self`, Tauri IPC, and data images;
  inline scripts/styles and remote content are not permitted.
- Markdown skips raw HTML. Links are rendered as inert text rather than clickable external anchors.
- Tauri exposes no filesystem or shell plugin and only the minimal core capability. Custom commands
  map to typed protocol requests; there is no secret getter, direct executor, or database command.
- IPC is a private `0600` Unix socket under a user-owned `0700` runtime directory. It accepts one
  newline-terminated JSON frame per connection, capped at 64 KiB with bounded I/O timeouts. No TCP
  control listener exists.

## Real execution and agent containment

- Dry-run remains the default and explicit dry-run requests never use the real executor. Real mode
  is a daemon startup choice and implements only `system.open_app` for daemon-registered entities.
  There is no shell, generic process, filesystem, network, database, or messaging executor.
- Application and CLI-agent registration canonicalizes the executable and rejects symlink path
  components, non-regular or non-executable files, disallowed owners, and group/world-writable
  executable or parent directories. Stored device, inode, owner, size, timestamps, and optional
  SHA-256 are rechecked before spawn. AI/agent proposals contain only an `app:*` entity and cannot
  provide a path or executable arguments.
- Agent output uses a strict two-phase broker schema and receives no policy internals or authority.
  Every proposal re-enters daemon classification, policy, exact-grant matching and audit. Agent
  grants bind the exact `AgentId` and optional `AgentSessionId`; they cannot authorize local calls.
- Bubblewrap unshares user/PID/network/IPC/UTS/cgroup namespaces, clears the environment, exposes
  read-only system runtime paths, and creates an isolated `/tmp` without home or daemon-socket mounts.
  A trusted `prlimit` wrapper applies CPU, address-space, process-count, file-size, and open-file
  limits. One wall deadline covers the full exchange; every error/timeout path kills and reaps the
  child.
- Startup marks unfinished agent sessions crashed and revokes stale agent/daemon session grants.
  Ending a session revokes its session grant. Once-grant claims use an `IMMEDIATE` SQLite transaction
  and conditional update so concurrent attempts have at most one winner.

## Provider and secret security

- Provider networking exists only in `halquen-ai`. The HTTP client enables normal TLS verification,
  disables redirects, applies connect/read/overall timeouts, and bounds completion bodies to 1 MiB.
- Cloud endpoints require HTTPS. Plain HTTP is accepted only for an explicitly local provider on
  `localhost`, `127.0.0.1`, or `::1`. Credentials, query strings, and fragments in base URLs are
  rejected.
- Provider status and HTTP errors are mapped to sanitized enums/messages. Response bodies and
  Authorization headers are not returned or logged.
- API keys pass transiently through the local GUI request and are immediately cleared from the
  controlled input after invoke serialization. JavaScript strings cannot be reliably zeroized;
  the renderer therefore has no read-secret API and never stores them in localStorage, SQLite,
  source, logs, or audit.
- The daemon stores secrets in the OS keyring using opaque IDs. Endpoint validation happens before a
  keyring mutation. Keyring/SQLite provider updates capture the previous secret and compensate on a
  database failure. No plaintext fallback exists.
- Manual cloud-model selection cannot bypass cloud-disabled or personal-context privacy policy.

## Persistence and observability

- SQLite uses bound parameters, foreign keys, a bounded busy timeout, transactional numbered
  migrations, and verified WAL for persistent databases. Memory writes and referenced evidence are
  checked in the same transaction.
- Audit APIs are append-oriented and exclude raw action arguments, prompts, secrets, file contents,
  and clipboard data. This is not cryptographic tamper evidence.
- Operational logs live in a private XDG state directory. Startup applies validated level,
  diagnostic toggle, retention days, and maximum total size. Central redaction covers representative
  Authorization, bearer-token, API-key, password, and oversized-token patterns.
- Diagnostics and activity expose structured reason/status metadata, never hidden chain of thought.

## Current limitations

- Linux/Unix sockets and Linux ownership discovery are the supported platform path.
- Real execution reports that the registered application was launched; Halquen does not yet track
  or cancel the application's complete lifetime after a successful background spawn.
- Authority-bearing service operations remain serialized, while socket connections are handled
  concurrently. Typed chat cancellation bypasses the busy service lock, targets an exact active
  request ID, and drops the provider future. Provider responses are still non-streaming, and other
  non-cancellation clients can wait behind a long provider call until its timeout.
- OpenAI-compatible/OpenAI/Ollama/LM Studio adapters are implemented. Anthropic and Gemini are typed
  unsupported boundaries, not guessed network implementations.
- The OS keyring must be available for cloud credentials; session-only secrets are not implemented.
- Background consolidation, semantic similarity reuse, embeddings, provider cost tables, and
  automatic model discovery are not implemented.
- Resource labels are persisted and applied to application proposals. There are no real file,
  network, database, or messaging executors yet, so those labels and the data-flow guard are ready
  policy inputs rather than a claim of end-to-end OS information-flow control.
- Daemon, CLI and GUI expose one-shot brokered CLI-agent runs. A confirm-required proposal is safely
  reported but no live confirmation token is retained after the run; an exact user grant followed by
  a rerun is required. Persistent interactive broker sessions remain future work. Unix-socket agent
  invocation is typed unsupported.
- Bubblewrap isolation is Linux-specific and depends on a correctly installed OS binary and kernel
  namespace support. Halquen does not download it. Unsafe unsandboxed execution requires the
  explicit `--allow-unsafe-agents` daemon flag. The configured `temp_bytes` value is validated and
  persisted, but the isolated tmpfs does not yet enforce that aggregate byte quota; memory and file
  size limits still apply. No cgroup or seccomp enforcement is claimed.
- Diagnostics exposes a bounded in-memory recent list, automatic log rotation, and GUI cleanup of
  historical log files that preserves the active log and audit records. Opening the log directory
  from the GUI is not exposed.
- Executable validation is path-based. Identity revalidation narrows replacement attacks but does
  not eliminate the TOCTOU window before `execve`; no race-free `openat2`/dirfd execution claim is
  made. Future filesystem capabilities require a separate threat model and the ADR 0008 design.
- The current Linux Tauri 2.x/WebKitGTK stack transitively resolves `glib 0.18.5`, which is covered
  by `RUSTSEC-2024-0429`; RustSec lists `glib >=0.20.0` as patched, while Tauri's GTK4/WebKitGTK 6
  migration remains upstream work. The advisory is intentionally reported by dependency checks
  rather than silently ignored in `deny.toml`.
