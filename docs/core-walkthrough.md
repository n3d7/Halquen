# Core walkthrough

## Local capability request

For `Open Telegram` in Chat:

1. The GUI sends a typed protocol-v5 `ChatRequest` through the Tauri bridge and private Unix socket.
2. The daemon persists the user message and a structured request activity event.
3. The deterministic parser resolves `system.open_app`, constructs typed `OpenApp` arguments, and
   the daemon assigns a bounded `UserExplicit → LocalResolver` provenance chain.
4. The resource-label matcher classifies the application. Policy evaluates provenance, resources,
   immutable hard rules, exact grants, profile and descriptor metadata before returning `Allow`,
   `Confirm`, or `Deny` with a typed rule ID/reason.
5. Only an allowed exact action receives a consumable authorization. In the default daemon mode the
   executor returns a simulated receipt. In explicit real mode, only a registered `app:*` entity can
   reach the narrow `system.open_app` executor.
6. Audit lifecycle, activity, assistant message, and usage counters are persisted; the UI displays a
   concise result with expandable local-route metadata. No AI provider is called.

An action requiring confirmation is retained server-side under a random, expiring, single-use ID.
“Allow once” re-evaluates policy through the confirmation-only path and consumes the entry before
execution. Cancel, expiry, replay, or `Deny` cannot execute.

The confirmation card shows operation, exact target, origin and resource classification. A user can
choose once, current session, one hour, or always for the exact scope. Persistent grants never widen
the typed arguments/resources and never override a hard deny.

## Real registered application execution

1. A trusted user registers an application entity, display name, absolute executable, fixed
   arguments, ownership rule, and optional SHA-256 through typed IPC.
2. The daemon rejects symlink components, non-regular/non-executable files, unsafe ownership or
   group/world-writable executable/parent paths, then records canonical metadata identity.
3. `execute open-app app:…` follows normal policy. The policy crate alone constructs the exact,
   non-clone `ExecutionAuthorization`.
4. Immediately before spawn, the real executor resolves the entity from the daemon-owned registry
   and rechecks device, inode, owner, size, timestamps, and optional hash.
5. Tokio spawns that exact path and fixed arguments directly with a cleared/narrow environment. A
   launched receipt and sanitized audit lifecycle are persisted. No shell is involved.

`dry-run open-app` always uses the dedicated dry executor. Path-based revalidation reduces but does
not eliminate replacement races between the final check and process creation; see ADR 0008.

## Brokered agent run

1. The daemon creates typed daemon/agent/instance/session identities and persists a running session.
2. The Agent Host sends bounded input plus public capability descriptions. It never sends policy
   internals, grants, secrets, the daemon socket, the home directory, or an authorization object.
3. Strict agent JSON can contain only a message and typed action proposals. The host assigns
   `Agent` provenance and no authority.
4. Each proposal re-enters resource classification, policy, exact-grant lookup, authorization, the
   selected runtime executor, and audit. Unknown capabilities fail without execution.
5. The broker returns structured executed/simulated/denied/failed/confirmation-required results and
   waits for the bounded child to exit. Timeout/error paths explicitly kill and reap the child.
6. The daemon closes the session and revokes session-scoped grants. Startup crashes stale sessions
   and revokes their stale agent/daemon grants.

Agent sessions are currently one-shot. A proposal needing confirmation does not receive a live
confirmation token; the user creates an exact agent grant and reruns it. This preserves authority but
is not yet a persistent interactive-agent workflow.

## Contextual application intent

For “open that messenger”:

1. The local resolver queries at most 512 retained application usage events from the last 90 days.
2. Each event receives `2^(-age / half_life)` decay with a default seven-day half-life. Success is
   `+1.25`, failure `-0.75`, accepted correction `+3`, and rejected correction `-4`.
3. Positive raw score becomes `1000 × (1 - exp(-score / 3))`. Automatic resolution requires at
   least 650 permille and a 120-permille lead over the runner-up.
4. Close scores or no evidence produce clarification. An explicit “not Telegram, Discord” records
   correction signals but creates no permission and does not weaken policy.

Storage deletes events older than 90 days and compacts each intent/context lane to 512 rows.

## Security control plane

The Security screen reads daemon-owned state through typed IPC. It exposes profile, immutable rule
IDs, exact and agent-bound grants with revocation, resource labels, trusted application registry,
agent configuration, broker runs, and recent typed sessions. React contains display and form state
only; validation, timestamps, executable inspection, grant authority, persistence and policy remain
in the daemon/core crates.

## Explicit memory request

For `Remember that when I say "editor" I mean Zed`:

1. The local parser recognizes a bounded alias/preference grammar; it does not ask a model.
2. The user's exact statement creates `UserExplicit` evidence and a typed preference value.
3. Memory kind, evidence linkage, trust, conflict/head state, and revision shape are validated.
4. SQLite inserts evidence and the immutable revision transactionally and advances the guarded head.
5. Chat and Activity report the committed preference. History restoration later creates another new
   revision instead of deleting history.

AI interpretation is not used as evidence for this path. Unsupported or ambiguous procedural prose
does not become executable code or a trusted procedure.

## Novel conversational request and reuse

For a request not resolved locally:

1. The daemon checks exact normalized, feedback-approved, fresh reusable responses.
2. On a miss, the deterministic router chooses an eligible configured model or returns a clear local
   unavailable response when AI is disabled/unconfigured.
3. `ContextBuilder` selects a bounded projection and preserves the trust/untrusted marker of every
   item. `PromptComposer` prepends the immutable security contract before task and personal text.
4. The provider-neutral gateway calls an approved OpenAI-compatible endpoint with normal TLS,
   redirect denial, timeouts, and a bounded response.
5. The answer is persisted as chat plus an `AiInferred`, initially non-reusable candidate. Usage and
   route/activity facts are recorded without provider bodies or chain of thought.
6. If the user explicitly marks it “Always reuse”, a later exact normalized request can return the
   validated local response with zero provider calls. Negative feedback lowers/revokes reuse.

This implements the milestone's core product test: a novel request may use AI once, while a repeated
validated request is checked locally first and can avoid both provider tokens and a resident local
model.

## Desktop controls

- Chat supports conversation history, Automatic/manual model selection, confirmation, all response
  feedback states, cancellable in-flight provider calls, sanitized errors, and a privacy-oriented AI
  request preview.
- Memory supports bounded search/filtering, evidence/trust metadata, pin/disable, and immutable
  revision history/restoration.
- AI supports provider/key setup, connection tests, model setup/defaults, privacy-aware routing
  settings, managed/personal prompt separation, and usage/estimated-efficiency metrics.
- Activity explains routes and outcomes; Diagnostics shows protocol/schema/paths/provider state,
  recent sanitized entries, and safely clears historical logs while preserving the active log and
  audit records; Settings persists validated appearance, privacy, budgets, learning, retention, and
  logging controls.

All lists are bounded and refreshed on screen entry or explicit user action. There are no busy loops,
short polling intervals, background model calls, or renderer-owned business rules.
