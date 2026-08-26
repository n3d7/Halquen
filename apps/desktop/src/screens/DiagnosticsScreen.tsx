import { useEffect, useMemo, useState } from "react";
import { Check, Copy, Database, RefreshCw, Search, Server, Waypoints } from "lucide-react";
import { Button, EmptyState, ErrorNotice, Input, PageHeader, Select, StatusBadge } from "../components/Common";
import { commandMessage, daemon } from "../lib/daemon";
import type { DiagnosticEntry, DiagnosticsSnapshot } from "../lib/types";

function severityTone(severity: DiagnosticEntry["severity"]): "bad" | "warn" | "info" | "neutral" {
  if (severity === "error") return "bad";
  if (severity === "warn") return "warn";
  if (severity === "info") return "info";
  return "neutral";
}

export function DiagnosticsScreen() {
  const [snapshot, setSnapshot] = useState<DiagnosticsSnapshot | null>(null);
  const [severity, setSeverity] = useState("all");
  const [search, setSearch] = useState("");
  const [copied, setCopied] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function load() {
    setBusy(true);
    setError(null);
    try {
      setSnapshot(await daemon.diagnostics());
    } catch (reason) {
      setError(commandMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => { void load(); }, []);

  const entries = useMemo(() => {
    const query = search.trim().toLocaleLowerCase();
    return (snapshot?.recent ?? []).filter((entry) => {
      if (severity !== "all" && entry.severity !== severity) return false;
      return !query || `${entry.component} ${entry.code} ${entry.message} ${entry.correlation_id ?? ""}`.toLocaleLowerCase().includes(query);
    });
  }, [search, severity, snapshot]);

  async function copy(entry: DiagnosticEntry) {
    const sanitized = `${new Date(entry.timestamp_ms).toISOString()} [${entry.severity.toUpperCase()}] ${entry.component}/${entry.code}: ${entry.message}${entry.correlation_id ? ` (${entry.correlation_id})` : ""}`;
    try {
      await navigator.clipboard.writeText(sanitized);
      setCopied(entry.code);
      window.setTimeout(() => setCopied(null), 1_500);
    } catch {
      setError("The sanitized diagnostic could not be copied.");
    }
  }

  return (
    <div className="page">
      <PageHeader title="Diagnostics" description="Sanitized technical status and recent operational errors. Activity and audit records have separate purposes." actions={<Button disabled={busy} onClick={() => void load()}><RefreshCw size={16} /> Refresh</Button>} />
      {error ? <ErrorNotice message={error} onRetry={() => void load()} /> : null}
      {snapshot ? <>
        <div className="health-grid">
          <article><Server size={18} /><span>Protocol</span><strong>v{snapshot.protocol_version}</strong></article>
          <article><Database size={18} /><span>Database schema</span><strong>v{snapshot.schema_version}</strong></article>
          <article><Waypoints size={18} /><span>Runtime socket</span><strong>Available</strong></article>
          <article><Database size={18} /><span>Audit records</span><strong>{snapshot.audit_records.toLocaleString()}</strong></article>
        </div>
        <section className="diagnostic-paths">
          <div><span>Database</span><code>{snapshot.database_path}</code></div>
          <div><span>Unix socket</span><code>{snapshot.runtime_socket}</code></div>
        </section>
        <div className="metric-grid compact-metrics">
          <article><span>Memory items</span><strong>{snapshot.memory_items}</strong></article>
          <article><span>Reusable responses</span><strong>{snapshot.cached_responses}</strong></article>
          <article><span>Unknown cases</span><strong>{snapshot.unknown_cases}</strong></article>
          <article><span>Providers</span><strong>{snapshot.provider_statuses.length}</strong></article>
        </div>
        {snapshot.provider_statuses.length > 0 ? <section className="provider-status-list"><h2>Provider status</h2>{snapshot.provider_statuses.map((provider) => <div key={provider.provider_id}><StatusBadge>{provider.status.replaceAll("_", " ")}</StatusBadge><span>{provider.message}</span></div>)}</section> : null}
        <section className="section-stack">
          <div className="section-heading"><div><h2>Recent diagnostics</h2><p>Logs are centrally redacted and automatically rotated by age and total size.</p></div></div>
          <div className="filter-bar">
            <label className="search-input"><Search size={16} /><Input aria-label="Search diagnostics" placeholder="Search component, code, or correlation" value={search} onChange={(event) => setSearch(event.target.value)} /></label>
            <Select aria-label="Severity" value={severity} onChange={(event) => setSeverity(event.target.value)}><option value="all">All severities</option><option value="error">Error</option><option value="warn">Warning</option><option value="info">Info</option><option value="debug">Debug</option></Select>
          </div>
          {entries.length === 0 ? <EmptyState title="No matching diagnostics" description="Recent sanitized errors and warnings will appear here." /> : <div className="diagnostic-list">{entries.map((entry, index) => <article key={`${entry.timestamp_ms}-${entry.code}-${index}`}><header><StatusBadge tone={severityTone(entry.severity)}>{entry.severity}</StatusBadge><strong>{entry.component}</strong><code>{entry.code}</code><time>{new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "medium" }).format(entry.timestamp_ms)}</time><Button variant="ghost" aria-label="Copy sanitized diagnostic" onClick={() => void copy(entry)}>{copied === entry.code ? <Check size={15} /> : <Copy size={15} />}</Button></header><p>{entry.message}</p>{entry.correlation_id ? <small>Correlation: {entry.correlation_id}</small> : null}</article>)}</div>}
        </section>
      </> : null}
    </div>
  );
}
