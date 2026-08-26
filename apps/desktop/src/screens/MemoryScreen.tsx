import { useEffect, useMemo, useState, type FormEvent } from "react";
import { History, Pin, RefreshCw, Search, Shield, Undo2 } from "lucide-react";
import { Button, EmptyState, ErrorNotice, Input, Modal, PageHeader, StatusBadge, Tabs } from "../components/Common";
import { commandMessage, daemon } from "../lib/daemon";
import type { MemoryRevision, MemoryRevisionView, MemoryValue, MemoryView, TrustClass } from "../lib/types";

type MemoryFilter = "all" | "semantic" | "procedural";

function valueLabel(value: MemoryValue): string {
  switch (value.kind) {
    case "fact":
      return `${value.predicate}: ${value.object}`;
    case "relation":
      return `${value.relation}: ${value.to}`;
    case "preference":
      return `${value.key} → ${value.value}`;
    case "procedure":
      return `${value.name} · ${value.capability_ids.length} capabilities`;
  }
}

function trustLabel(trust: TrustClass): string {
  return trust.replaceAll("_", " ");
}

function MemoryDetails({
  item,
  onClose,
  onChanged,
}: {
  item: MemoryView;
  onClose: () => void;
  onChanged: () => Promise<void>;
}) {
  const [history, setHistory] = useState<MemoryRevisionView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let active = true;
    daemon.memoryHistory(item.item.id)
      .then((revisions) => active && setHistory(revisions))
      .catch((reason: unknown) => active && setError(commandMessage(reason)));
    return () => { active = false; };
  }, [item.item.id]);

  async function update(update: { pinned?: boolean; disabled?: boolean; priority_permille?: number }) {
    setBusy(true);
    setError(null);
    try {
      await daemon.updateMemoryState(item.item.id, update);
      await onChanged();
      onClose();
    } catch (reason) {
      setError(commandMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  async function restore(revision: MemoryRevision) {
    setBusy(true);
    setError(null);
    try {
      await daemon.restoreMemory(item.item.id, revision.id);
      await onChanged();
      onClose();
    } catch (reason) {
      setError(commandMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title="Memory details" onClose={onClose}>
      <div className="memory-detail-title">
        <div>
          <span className="eyebrow">{item.item.kind}</span>
          <h3>{valueLabel(item.current.value)}</h3>
        </div>
        {item.pinned ? <StatusBadge tone="info">Pinned</StatusBadge> : null}
        {item.disabled ? <StatusBadge tone="warn">Disabled</StatusBadge> : null}
      </div>
      <dl className="detail-list">
        <div><dt>Priority</dt><dd>{item.priority_permille / 10}%</dd></div>
        <div><dt>Confidence</dt><dd>{item.confidence_permille / 10}%</dd></div>
        <div><dt>Evidence</dt><dd>{item.evidence_count}</dd></div>
        <div><dt>Updated</dt><dd>{new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(item.item.updated_at_ms)}</dd></div>
        <div><dt>Last used</dt><dd>{item.last_used_at_ms ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(item.last_used_at_ms) : "Never"}</dd></div>
      </dl>
      <div className="trust-row"><Shield size={16} />{item.trust_classes.map((trust) => <StatusBadge key={trust}>{trustLabel(trust)}</StatusBadge>)}</div>
      <div className="button-row">
        <Button disabled={busy} onClick={() => void update({ pinned: !item.pinned })}><Pin size={16} />{item.pinned ? "Unpin" : "Pin"}</Button>
        <Button disabled={busy} variant={item.disabled ? "secondary" : "danger"} onClick={() => void update({ disabled: !item.disabled })}>
          {item.disabled ? "Enable" : "Disable"}
        </Button>
      </div>
      <section className="history-section">
        <h3><History size={17} /> Revision history</h3>
        {history.map(({ revision, trust_classes }, index) => {
          const current = revision.id === item.item.current_revision_id;
          return (
            <article className="history-entry" key={revision.id}>
              <div>
                <strong>Revision {history.length - index}{current ? " · current" : ""}</strong>
                <span>{new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(revision.created_at_ms)}</span>
                <p>{valueLabel(revision.value)}</p>
                <small>{trust_classes.map(trustLabel).join(", ") || "No trust evidence"}</small>
              </div>
              {!current ? <Button variant="ghost" disabled={busy} onClick={() => void restore(revision)}><Undo2 size={15} /> Restore</Button> : null}
            </article>
          );
        })}
      </section>
      {error ? <ErrorNotice message={error} /> : null}
    </Modal>
  );
}

export function MemoryScreen() {
  const [filter, setFilter] = useState<MemoryFilter>("all");
  const [search, setSearch] = useState("");
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<MemoryView[]>([]);
  const [selected, setSelected] = useState<MemoryView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function load(nextFilter = filter, nextQuery = query) {
    setBusy(true);
    setError(null);
    try {
      setItems(await daemon.memory(nextFilter === "all" ? null : nextFilter, nextQuery));
    } catch (reason) {
      setError(commandMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => { void load("all", ""); }, []);

  function submit(event: FormEvent) {
    event.preventDefault();
    const next = search.trim();
    setQuery(next);
    void load(filter, next);
  }

  function changeFilter(next: MemoryFilter) {
    setFilter(next);
    void load(next, query);
  }

  const enabledCount = useMemo(() => items.filter((item) => !item.disabled).length, [items]);

  return (
    <div className="page">
      <PageHeader
        title="Memory"
        description="Typed, evidence-backed knowledge. Changes go through daemon validation and versioned transactions."
        actions={<Button disabled={busy} onClick={() => void load()}><RefreshCw size={16} /> Refresh</Button>}
      />
      <Tabs value={filter} onChange={changeFilter} items={[
        { value: "all", label: "All" },
        { value: "semantic", label: "Semantic" },
        { value: "procedural", label: "Procedural" },
      ]} />
      <form className="filter-bar" onSubmit={submit}>
        <label className="search-input"><Search size={16} /><Input aria-label="Search memory" placeholder="Search canonical values" value={search} onChange={(event) => setSearch(event.target.value)} /></label>
        <Button type="submit">Search</Button>
        <span className="muted">{enabledCount} enabled · {items.length} shown</span>
      </form>
      {error ? <ErrorNotice message={error} onRetry={() => void load()} /> : null}
      {items.length === 0 && !busy ? (
        <EmptyState title="No matching memory" description="Teach Halquen a stable preference in chat, for example: “Remember that my editor is Zed.”" />
      ) : (
        <div className="memory-list">
          {items.map((item) => (
            <button className={item.disabled ? "memory-row disabled" : "memory-row"} key={item.item.id} onClick={() => setSelected(item)}>
              <div className="memory-kind"><span>{item.item.kind}</span>{item.pinned ? <Pin size={14} /> : null}</div>
              <div className="memory-value"><strong>{valueLabel(item.current.value)}</strong><span>{item.trust_classes.map(trustLabel).join(", ") || "No trust evidence"}</span></div>
              <div className="memory-score"><strong>{item.priority_permille / 10}%</strong><span>priority</span></div>
              <div className="memory-score"><strong>{item.confidence_permille / 10}%</strong><span>confidence</span></div>
              <div className="memory-updated"><span>{new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(item.item.updated_at_ms)}</span>{item.disabled ? <StatusBadge tone="warn">Disabled</StatusBadge> : null}</div>
            </button>
          ))}
        </div>
      )}
      {selected ? <MemoryDetails item={selected} onClose={() => setSelected(null)} onChanged={() => load()} /> : null}
    </div>
  );
}
