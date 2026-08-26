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

## Renderer and IPC

- The WebView loads only bundled local content. CSP permits only `self`, Tauri IPC, and data images;
  inline scripts/styles and remote content are not permitted.
- Markdown skips raw HTML. Links are rendered as inert text rather than clickable external anchors.
- Tauri exposes no filesystem or shell plugin and only the minimal core capability. Custom commands
  map to typed protocol requests; there is no secret getter, direct executor, or database command.
- IPC is a private `0600` Unix socket under a user-owned `0700` runtime directory. It accepts one
  newline-terminated JSON frame per connection, capped at 64 KiB with bounded I/O timeouts. No TCP
  control listener exists.

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
- Execution is dry-run only. A future real executor requires additional capability-specific threat
  modelling and cancellable side-effect semantics.
- The daemon handles requests sequentially. A long non-streaming provider call can delay other local
  clients; provider timeouts bound it, but protocol streaming/cancellation is not implemented.
- OpenAI-compatible/OpenAI/Ollama/LM Studio adapters are implemented. Anthropic and Gemini are typed
  unsupported boundaries, not guessed network implementations.
- The OS keyring must be available for cloud credentials; session-only secrets are not implemented.
- Persistent permission grants, background consolidation, semantic similarity reuse, embeddings,
  provider cost tables, and automatic model discovery are not implemented.
- Diagnostics exposes a bounded in-memory recent list and automatic log rotation; clearing/opening
  log files from the GUI is not yet exposed.
- Filesystem validation is path-based and does not claim race-free `openat2` resolution.
