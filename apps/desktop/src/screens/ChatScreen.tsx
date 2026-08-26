import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { ChevronDown, MessageSquarePlus, Send, ShieldCheck, Sparkles, Square } from "lucide-react";
import { daemon, commandMessage } from "../lib/daemon";
import type {
  AiModel,
  ChatMessage,
  ChatRequest,
  ChatSession,
  ConfirmationPrompt,
  PromptPreview,
} from "../lib/types";
import { Button, EmptyState, ErrorNotice, Modal, Select, Spinner, StatusBadge } from "../components/Common";
import { SafeMarkdown } from "../components/SafeMarkdown";

function formatTime(timestamp: number): string {
  return new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" }).format(timestamp);
}

function routeLabel(message: ChatMessage): string {
  switch (message.route) {
    case "local_capability":
      return "Local capability";
    case "local_memory":
      return "Local memory";
    case "response_cache":
      return "Validated reuse";
    case "ai":
      return "AI response";
    case "clarification":
      return "Clarification";
    case "unavailable":
      return "Unavailable";
    default:
      return message.origin === "user" ? "You" : "Halquen";
  }
}

function Message({ message, onFeedback }: { message: ChatMessage; onFeedback: (id: string, feedback: "useful" | "wrong" | "do_not_remember" | "always_use" | "prefer") => Promise<void> }) {
  const [feedbackState, setFeedbackState] = useState<string | null>(null);
  const isUser = message.role === "user";

  async function submit(feedback: "useful" | "wrong" | "do_not_remember" | "always_use" | "prefer") {
    if (!message.reusable_candidate_id) return;
    setFeedbackState("Saving…");
    try {
      await onFeedback(message.reusable_candidate_id, feedback);
      setFeedbackState("Saved");
    } catch (error) {
      setFeedbackState(commandMessage(error));
    }
  }

  return (
    <article className={isUser ? "message message-user" : "message message-assistant"}>
      <div className="message-heading">
        <strong>{isUser ? "You" : "Halquen"}</strong>
        <time dateTime={new Date(message.created_at_ms).toISOString()}>{formatTime(message.created_at_ms)}</time>
      </div>
      <div className="message-content">
        {isUser ? <p>{message.content}</p> : <SafeMarkdown>{message.content}</SafeMarkdown>}
      </div>
      {!isUser ? (
        <details className="message-details">
          <summary>
            <ChevronDown size={14} /> {routeLabel(message)}
          </summary>
          <dl className="detail-grid">
            <div><dt>Route</dt><dd>{message.route?.replaceAll("_", " ") ?? "system"}</dd></div>
            <div><dt>Latency</dt><dd>{message.latency_ms === null ? "—" : `${message.latency_ms} ms`}</dd></div>
            <div><dt>Provider</dt><dd>{message.provider_id ? "Configured provider" : "No AI used"}</dd></div>
            <div><dt>Tokens</dt><dd>{message.input_tokens === null ? "—" : `${message.input_tokens} in / ${message.output_tokens ?? 0} out`}</dd></div>
          </dl>
        </details>
      ) : null}
      {message.reusable_candidate_id ? (
        <div className="message-feedback" aria-label="Response feedback">
          <button onClick={() => void submit("useful")}>Useful</button>
          <button onClick={() => void submit("wrong")}>Wrong</button>
          <button onClick={() => void submit("do_not_remember")}>Don't reuse</button>
          <button onClick={() => void submit("prefer")}>Prefer</button>
          <button onClick={() => void submit("always_use")}>Always reuse</button>
          {feedbackState ? <span>{feedbackState}</span> : null}
        </div>
      ) : null}
    </article>
  );
}

function Confirmation({ prompt, onDone }: { prompt: ConfirmationPrompt; onDone: (allow: boolean) => Promise<void> }) {
  const [busy, setBusy] = useState(false);
  return (
    <section className="confirmation" aria-label="Action confirmation">
      <ShieldCheck size={20} />
      <div>
        <strong>{prompt.title}</strong>
        <p>{prompt.reason}</p>
        <small>Allow once. This approval expires automatically.</small>
      </div>
      <div className="confirmation-actions">
        <Button disabled={busy} onClick={() => { setBusy(true); void onDone(false); }}>Cancel</Button>
        <Button variant="primary" disabled={busy} onClick={() => { setBusy(true); void onDone(true); }}>Allow once</Button>
      </div>
    </section>
  );
}

export function ChatScreen({ daemonOnline, onOpenAi }: { daemonOnline: boolean; onOpenAi: () => void }) {
  const [sessions, setSessions] = useState<ChatSession[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [models, setModels] = useState<AiModel[]>([]);
  const [modelValue, setModelValue] = useState("automatic");
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [cancellationRequested, setCancellationRequested] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<ConfirmationPrompt | null>(null);
  const [confirmationResult, setConfirmationResult] = useState<string | null>(null);
  const [preview, setPreview] = useState<PromptPreview | null>(null);
  const [previewBusy, setPreviewBusy] = useState(false);
  const endRef = useRef<HTMLDivElement>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const activeRequestRef = useRef<string | null>(null);

  useEffect(() => {
    let active = true;
    Promise.all([daemon.chatSessions(), daemon.models()])
      .then(([loadedSessions, loadedModels]) => {
        if (!active) return;
        setSessions(loadedSessions);
        setModels(loadedModels.filter((model) => model.enabled));
        setSessionId((current) => current ?? loadedSessions[0]?.id ?? null);
      })
      .catch((reason: unknown) => active && setError(commandMessage(reason)));
    return () => { active = false; };
  }, []);

  useEffect(() => {
    if (!sessionId) {
      setMessages([]);
      return;
    }
    let active = true;
    daemon.chatMessages(sessionId)
      .then((loaded) => active && setMessages(loaded))
      .catch((reason: unknown) => active && setError(commandMessage(reason)));
    return () => { active = false; };
  }, [sessionId]);

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "nearest" });
  }, [messages, busy, confirmation]);

  function requestFor(message: string): ChatRequest {
    return {
      session_id: sessionId,
      message,
      model_selection: modelValue === "automatic"
        ? { kind: "automatic" }
        : { kind: "model", model_id: modelValue.slice("model:".length) },
    };
  }

  async function send() {
    const message = draft.trim();
    if (!message || busy || !daemonOnline) return;
    setDraft("");
    setBusy(true);
    setCancellationRequested(false);
    setError(null);
    setConfirmation(null);
    setConfirmationResult(null);
    const requestId = `request:chat:${crypto.randomUUID()}`;
    activeRequestRef.current = requestId;
    try {
      const result = await daemon.sendChat(requestId, requestFor(message));
      setSessionId(result.session.id);
      setSessions((current) => [result.session, ...current.filter((item) => item.id !== result.session.id)]);
      setMessages((current) => [...current, result.user_message, result.assistant_message]);
      setConfirmation(result.confirmation);
    } catch (reason) {
      setDraft(message);
      setError(commandMessage(reason));
    } finally {
      if (activeRequestRef.current === requestId) activeRequestRef.current = null;
      setBusy(false);
      setCancellationRequested(false);
      composerRef.current?.focus();
    }
  }

  async function cancelRequest() {
    const requestId = activeRequestRef.current;
    if (!requestId || cancellationRequested) return;
    setError(null);
    try {
      const requested = await daemon.cancelChat(requestId);
      if (requested) {
        setCancellationRequested(true);
      } else {
        setError("The request already completed or is no longer cancellable.");
      }
    } catch (reason) {
      setError(commandMessage(reason));
    }
  }

  async function confirm(allow: boolean) {
    if (!confirmation) return;
    try {
      const result = await daemon.confirm(confirmation.confirmation_id, allow);
      setConfirmationResult(result.message);
      setConfirmation(null);
    } catch (reason) {
      setError(commandMessage(reason));
    }
  }

  async function showPreview() {
    const message = draft.trim();
    if (!message) return;
    setPreviewBusy(true);
    try {
      setPreview(await daemon.preview(requestFor(message)));
    } catch (reason) {
      setError(commandMessage(reason));
    } finally {
      setPreviewBusy(false);
    }
  }

  function composerKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void send();
    }
  }

  return (
    <div className="chat-layout">
      <aside className="conversation-list">
        <div className="conversation-list-header">
          <strong>Conversations</strong>
          <Button variant="ghost" aria-label="New conversation" onClick={() => { setSessionId(null); setMessages([]); setConfirmation(null); composerRef.current?.focus(); }}>
            <MessageSquarePlus size={18} />
          </Button>
        </div>
        <div className="conversation-scroll">
          {sessions.map((session) => (
            <button key={session.id} className={sessionId === session.id ? "conversation active" : "conversation"} onClick={() => setSessionId(session.id)}>
              <strong>{session.title}</strong>
              <span>{new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" }).format(session.updated_at_ms)}</span>
            </button>
          ))}
          {sessions.length === 0 ? <p className="muted compact">No previous conversations.</p> : null}
        </div>
      </aside>
      <section className="chat-main">
        <header className="chat-toolbar">
          <div>
            <strong>{sessions.find((item) => item.id === sessionId)?.title ?? "New conversation"}</strong>
            <span>Local routes are checked before AI</span>
          </div>
          <Select aria-label="Model selection" value={modelValue} onChange={(event) => setModelValue(event.target.value)}>
            <option value="automatic">Automatic</option>
            {models.map((model) => <option key={model.id} value={`model:${model.id}`}>{model.display_name}</option>)}
          </Select>
        </header>
        <div className="message-scroll" aria-live="polite">
          {!daemonOnline ? (
            <EmptyState title="Daemon unavailable" description="Start halquen-daemon, then retry the connection from the sidebar status." />
          ) : messages.length === 0 ? (
            <EmptyState
              title="What can I help with?"
              description="Halquen works locally first. AI providers are optional and cloud access is off by default."
              action={<Button onClick={onOpenAi}>Configure AI</Button>}
            />
          ) : messages.map((message) => (
            <Message key={message.id} message={message} onFeedback={(id, feedback) => daemon.feedback(id, feedback)} />
          ))}
          {busy ? <div className="processing"><Spinner label={cancellationRequested ? "Cancelling request" : "Processing request"} /><span>{cancellationRequested ? "Cancellation requested; waiting for the daemon to stop the provider call…" : "Checking local routes, then eligible AI if needed…"}</span></div> : null}
          {confirmation ? <Confirmation prompt={confirmation} onDone={confirm} /> : null}
          {confirmationResult ? <div className="inline-success">{confirmationResult}</div> : null}
          {error ? <ErrorNotice message={error} /> : null}
          <div ref={endRef} />
        </div>
        <footer className="composer-wrap">
          <div className="composer">
            <textarea
              ref={composerRef}
              aria-label="Message Halquen"
              placeholder="Message Halquen…"
              value={draft}
              rows={1}
              disabled={!daemonOnline || busy}
              onChange={(event) => setDraft(event.target.value.slice(0, 16_384))}
              onKeyDown={composerKeyDown}
            />
            <Button variant="ghost" aria-label="Preview AI request" title="Preview AI request" disabled={!draft.trim() || busy || previewBusy} onClick={() => void showPreview()}>
              {previewBusy ? <Spinner /> : <Sparkles size={18} />}
            </Button>
            {busy ? (
              <Button variant="danger" aria-label="Cancel request" title="Cancel request" disabled={cancellationRequested} onClick={() => void cancelRequest()}>
                <Square size={16} />
              </Button>
            ) : (
              <Button variant="primary" aria-label="Send message" disabled={!draft.trim() || !daemonOnline} onClick={() => void send()}>
                <Send size={18} />
              </Button>
            )}
          </div>
          <span>Enter to send · Shift+Enter for a new line</span>
        </footer>
      </section>
      {preview ? (
        <Modal title="AI request preview" onClose={() => setPreview(null)}>
          <p className="muted">Sanitized metadata only. Secrets and hidden reasoning are never included.</p>
          <dl className="detail-list">
            <div><dt>Task</dt><dd>{preview.task.replaceAll("_", " ")}</dd></div>
            <div><dt>Provider</dt><dd>{preview.provider_id ? "Selected provider" : "Not selected"}</dd></div>
            <div><dt>Model</dt><dd>{preview.model_id ? "Selected model" : "Not selected"}</dd></div>
            <div><dt>Estimated context</dt><dd>{preview.estimated_context_tokens.toLocaleString()} tokens</dd></div>
            <div><dt>Context categories</dt><dd>{preview.context_categories.join(", ") || "Current request only"}</dd></div>
            <div><dt>Core security contract</dt><dd><StatusBadge tone="good">Managed by Halquen</StatusBadge></dd></div>
          </dl>
          {preview.personal_instructions ? <div className="code-preview">{preview.personal_instructions}</div> : null}
        </Modal>
      ) : null}
    </div>
  );
}
