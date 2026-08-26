# Core walkthrough

## Local capability request

For `Open Telegram` in Chat:

1. The GUI sends a typed protocol-v2 `ChatRequest` through the Tauri bridge and private Unix socket.
2. The daemon persists the user message and a structured request activity event.
3. The deterministic parser resolves `system.open_app` and constructs typed `OpenApp` arguments.
4. The trusted registry validates the argument shape and policy returns `Allow`, `Confirm`, or
   `Deny` from descriptor risk/side-effect metadata.
5. Only an allowed exact action receives a consumable authorization. The current executor returns a
   simulated receipt under the descriptor timeout and performs no OS side effect.
6. Audit lifecycle, activity, assistant message, and usage counters are persisted; the UI displays a
   concise result with expandable local-route metadata. No AI provider is called.

An action requiring confirmation is retained server-side under a random, expiring, single-use ID.
“Allow once” re-evaluates policy through the confirmation-only path and consumes the entry before
execution. Cancel, expiry, replay, or `Deny` cannot execute.

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

- Chat supports conversation history, Automatic/manual model selection, confirmation, feedback,
  sanitized errors, and a privacy-oriented AI request preview.
- Memory supports bounded search/filtering, evidence/trust metadata, pin/disable, and immutable
  revision history/restoration.
- AI supports provider/key setup, connection tests, model setup/defaults, privacy-aware routing
  settings, managed/personal prompt separation, and usage/estimated-efficiency metrics.
- Activity explains routes and outcomes; Diagnostics shows protocol/schema/paths/provider state and
  recent sanitized entries; Settings persists validated appearance, privacy, budgets, learning,
  retention, and logging controls.

All lists are bounded and refreshed on screen entry or explicit user action. There are no busy loops,
short polling intervals, background model calls, or renderer-owned business rules.
