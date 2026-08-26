import { useEffect, useMemo, useState } from "react";
import { RefreshCw, Route, Search } from "lucide-react";
import { Button, EmptyState, ErrorNotice, Input, PageHeader, Select, StatusBadge } from "../components/Common";
import { commandMessage, daemon } from "../lib/daemon";
import type { ActivityEvent, ActivityKind } from "../lib/types";

function toneFor(kind: ActivityKind): "good" | "warn" | "bad" | "neutral" | "info" {
  if (kind === "error" || kind === "ai_failed") return "bad";
  if (kind === "confirmation_required") return "warn";
  if (kind === "execution_completed" || kind === "memory_committed") return "good";
  if (kind === "ai_selected" || kind === "ai_completed") return "info";
  return "neutral";
}

export function ActivityScreen() {
  const [events, setEvents] = useState<ActivityEvent[]>([]);
  const [kind, setKind] = useState("all");
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function load() {
    setBusy(true);
    setError(null);
    try {
      setEvents(await daemon.activity());
    } catch (reason) {
      setError(commandMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => { void load(); }, []);

  const filtered = useMemo(() => {
    const query = search.trim().toLocaleLowerCase();
    return events.filter((event) => {
      if (kind !== "all" && event.kind !== kind) return false;
      if (!query) return true;
      return `${event.summary} ${event.detail ?? ""}`.toLocaleLowerCase().includes(query);
    });
  }, [events, kind, search]);

  return (
    <div className="page">
      <PageHeader
        title="Activity"
        description="Structured facts about routing, policy, memory, and execution. Operational logs are separate."
        actions={<Button onClick={() => void load()} disabled={busy}><RefreshCw size={16} /> Refresh</Button>}
      />
      <div className="filter-bar">
        <label className="search-input"><Search size={16} /><Input aria-label="Search activity" placeholder="Search activity" value={search} onChange={(event) => setSearch(event.target.value)} /></label>
        <Select aria-label="Activity kind" value={kind} onChange={(event) => setKind(event.target.value)}>
          <option value="all">All events</option>
          <option value="local_route_hit">Local routes</option>
          <option value="ai_selected">AI selected</option>
          <option value="ai_completed">AI completed</option>
          <option value="memory_committed">Memory changes</option>
          <option value="policy_evaluated">Policy</option>
          <option value="execution_completed">Execution</option>
          <option value="confirmation_required">Confirmations</option>
          <option value="error">Errors</option>
        </Select>
      </div>
      {error ? <ErrorNotice message={error} onRetry={() => void load()} /> : null}
      {filtered.length === 0 && !busy ? (
        <EmptyState title="No matching activity" description="Activity appears here after Halquen handles requests." />
      ) : (
        <div className="timeline">
          {filtered.map((event) => (
            <article className="timeline-event" key={event.id}>
              <div className="timeline-marker"><Route size={15} /></div>
              <div className="timeline-card">
                <header>
                  <time dateTime={new Date(event.created_at_ms).toISOString()}>
                    {new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "medium" }).format(event.created_at_ms)}
                  </time>
                  <StatusBadge tone={toneFor(event.kind)}>{event.kind.replaceAll("_", " ")}</StatusBadge>
                </header>
                <strong>{event.summary}</strong>
                {event.detail ? <p>{event.detail}</p> : null}
                <details>
                  <summary>Developer details</summary>
                  <dl className="detail-grid">
                    <div><dt>Correlation</dt><dd>{event.correlation_id}</dd></div>
                    <div><dt>Conversation</dt><dd>{event.session_id ? "Linked" : "None"}</dd></div>
                  </dl>
                </details>
              </div>
            </article>
          ))}
        </div>
      )}
    </div>
  );
}
