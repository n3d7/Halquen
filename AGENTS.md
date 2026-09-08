# Halquen Repository Instructions

Halquen is a Linux-first, local-first personal assistant and authority layer for AI-assisted workflows,
built around deterministic routing, evidence-backed memory, optional AI reasoning, typed capabilities,
and policy-controlled execution.

These instructions apply to the entire repository.
## Core invariant

Never violate this boundary:

> LLM output, agent output, plugins, and external content are advice/data.
> Capabilities, deterministic policy, explicit authority, and trusted evidence are authority.

Untrusted or inferred content must never directly:
- authorize execution or construct execution authorization;
- bypass policy, immutable hard-deny rules, or data-flow rules;
- grant permissions or manufacture trusted user authority;
- become trusted procedural memory without the required evidence path;
- invoke arbitrary shell, process, filesystem, network, database, or provider operations.

Source code is authoritative. Indexes, graphs, summaries, documentation, and agent memory are navigation aids only.
## Architecture

Preserve this authority direction:

React renderer
→ typed Tauri commands
→ `halquen-protocol`
→ private Unix socket
→ `halquen-daemon`
→ authority-bearing core crates

Key ownership:
- `halquen-domain` — typed identifiers, actions, provenance, security, chat, settings, and core data.
- `halquen-policy` — Allow / Confirm / Deny, grants, and exact non-clone execution authorization.
- `halquen-capabilities` — capability/application registries, executable identity, dry-run and real executors.
- `halquen-memory` — evidence, immutable revisions, trust, derived memory kind, and promotion rules.
- `halquen-storage` — SQLite, migrations, transactions, XDG paths, and bounded queries.
- `halquen-audit` — durable policy/execution lifecycle receipts.
- `halquen-protocol` — versioned IPC DTOs, bounded framing, runtime paths, and daemon client.
- `halquen-ai` — bounded context/prompt composition, model routing, provider networking, keyring abstraction,
  and the external Agent Host boundary.
- `halquen-daemon` — sole composition and business-logic root.
- `halquen-cli` and `halquen-desktop` — clients of the daemon.

Do not move policy, execution, memory authority, persistence, provider routing, grants, or authorization
into React or the Tauri bridge.

The renderer must not gain direct access to SQLite, the executor, the OS keyring, provider networking,
shell/process execution, or unrestricted filesystem APIs.

Before changing architecture or a trust boundary, inspect `docs/architecture.md`, `docs/security.md`,
affected source, and relevant tests. Read `docs/core-walkthrough.md` when changing chat, memory,
confirmation, reuse, AI routing, or desktop interaction flows.

## Action authority

Do not collapse the authority pipeline into a generic permission check.

Preserve the conceptual order:

trusted request boundary
→ typed `ActionProposal(ActionRequest + ActionContext)`
→ provenance validation
→ resource classification
→ immutable data-flow/resource rules
→ exact grants
→ baseline/profile policy
→ optional concrete confirmation
→ exact `ExecutionAuthorization`
→ executor

Important invariants:
- only trusted daemon paths may assign user authority such as `UserExplicit`;
- AI, agents, plugins, and external-content proposals carry no authority;
- immutable hard-deny/data-flow rules cannot be overridden by confirmation or persistent grants;
- `Deny` must never become executable through confirmation, grants, or another bypass;
- grants must remain exact, bounded, revocable/expiring where designed, and correctly scoped;
- single-use authorization/grants must remain single-use and concurrency-safe;
- `ExecutionAuthorization` must stay exact and construction must remain policy-controlled.

## Execution and agents

Dry-run remains the default execution mode.

Real execution is capability-specific and deliberately narrow. The current real executor is limited
to daemon-registered `system.open_app` actions: executable identity is revalidated and fixed
executable/arguments are spawned directly without a shell.

Never accept an executable path, shell command, or arbitrary process arguments from AI/agent output.
Any new real side-effect capability requires explicit threat modelling of arguments, resources,
destinations, authorization, audit behaviour, and failure modes.

External agents are untrusted principals, not authority-bearing extensions of the daemon.

Preserve the Agent Host boundary:
- subprocess output contains typed proposals, never `ActionContext` or authorization;
- every proposal re-enters daemon provenance/resource classification, policy, grants, and audit;
- agent grants remain bound to the intended agent and session scope;
- executable and arguments remain separated and validated;
- environment, I/O, runtime, and resource limits remain bounded;
- sandbox failure fails closed unless the daemon was explicitly started with the unsafe opt-in;
- sandboxed agents must not silently gain user-home, daemon-socket, or network access.

Do not weaken containment merely to make an agent integration work.
## Memory, AI, and local-first behaviour

AI-inferred, agent/plugin-supplied, external, or passive behavioural information must not silently
become trusted memory or authority.

Reusable conversational responses remain separate from semantic/procedural memory and never carry
execution authorization.

Core Halquen functionality must remain usable without a cloud AI provider. Do not introduce idle
polling, unnecessary background model calls, hidden network dependencies, automatic user-data
transmission, or unnecessary resident-model requirements.

Prefer deterministic/local resolution before model calls when supported. Keep model context bounded.
Do not automatically send full chat history, databases, audit logs, memory stores, files, or unrelated
user/project data to a provider.

Provider networking stays behind `halquen-ai`. Secrets must never be committed, logged, exposed
through renderer read APIs, or stored in plaintext. Provider credentials belong in the OS credential
store; do not add a plaintext fallback.

## Security and engineering

Treat every external or cross-boundary input as untrusted.

Never weaken validation, authorization, TLS verification, CSP, IPC limits, cryptographic checks,
timeouts, sandboxing, tests, or permission boundaries merely to make functionality pass.

Never introduce arbitrary shell execution, raw SQL execution, generic process execution,
unrestricted filesystem access, or generic provider/network execution without an explicitly reviewed design.

Prefer small, focused changes and avoid unrelated refactors. Before adding a dependency, crate,
service, network path, persistence mechanism, protocol field, capability, executor surface, or Tauri
permission, verify that the existing architecture cannot solve the problem cleanly.

Preserve type safety and trust boundaries. Avoid `unwrap`, `expect`, panic paths, unchecked indexing,
and unchecked assumptions at external/trust boundaries unless the invariant is genuinely impossible
to violate. Use typed/contextual errors where practical.

Database schema changes must use numbered migrations, preserve existing data, and never silently
rewrite already-shipped migrations.

Behaviour changes should normally include/update tests. Security-sensitive changes should test
invalid/adversarial input and fail-closed behaviour where practical.
## Context efficiency

Avoid broad repository exploration when targeted retrieval can answer the question.

Preferred retrieval order:
1. Codebase Memory / existing project index
2. Git status, changed files, and affected symbols
3. targeted `rg`, `fd`, or symbol search
4. relevant project documentation
5. targeted source reads
6. broader repository exploration only when necessary
7. external documentation only for external APIs/libraries

Use Codebase Memory for navigation, relationships, and change-impact analysis, never as source truth.
When correctness depends on implementation detail, verify the real source. Refresh/revalidate stale
indexes after large pulls, rebases, branch switches, merges, or mass refactors.

Prefer `rg` for text search, `fd` for file discovery, `jq` for JSON filtering, narrow Git queries
before full diffs, and RTK-wrapped commands when large output would otherwise waste context.

For tests/builds/logs, inspect summaries and failures first. If compressed output hides required
debugging detail, retrieve the relevant raw section instead of guessing. Do not repeatedly reread
unchanged files without a concrete reason.
## External documentation

Use Context7 for current or version-sensitive external library/framework/SDK/API documentation.
Do not use it to infer Halquen's own architecture when the repository contains the answer.
Never send proprietary source, secrets, or user data to external documentation services.

## Verification

Run the smallest relevant checks during development. Before considering substantial changes complete,
run the appropriate checks from:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --all-features --locked
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features -- -D warnings

pnpm --dir apps/desktop typecheck
pnpm --dir apps/desktop test
pnpm --dir apps/desktop build

git diff --check
```

Use `RTK.md` when its output-wrapping guidance applies.
