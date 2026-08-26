import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Bot, Plus, RefreshCw, ShieldCheck, Trash2, Zap } from "lucide-react";
import {
  Button,
  EmptyState,
  ErrorNotice,
  Field,
  Input,
  Modal,
  PageHeader,
  Select,
  StatusBadge,
  Tabs,
  TextArea,
  Toggle,
} from "../components/Common";
import { commandMessage, daemon } from "../lib/daemon";
import type {
  AiModel,
  ApplicationSettings,
  ModelUpsert,
  PrivacyClass,
  PromptPreview,
  Provider,
  ProviderKind,
  ProviderUpsert,
  UsageStats,
} from "../lib/types";

type AiTab = "providers" | "models" | "routing" | "usage" | "prompts";

const providerDefaults: Record<Exclude<ProviderKind, "anthropic" | "gemini">, string> = {
  open_ai_compatible: "https://api.example.com/v1",
  open_ai: "https://api.openai.com/v1",
  ollama: "http://127.0.0.1:11434/v1",
  lm_studio: "http://127.0.0.1:1234/v1",
};

function statusTone(status: Provider["status"]): "good" | "warn" | "bad" | "neutral" | "info" {
  if (status === "connected" || status === "configured") return "good";
  if (status === "rate_limited" || status === "unsupported") return "warn";
  if (status === "unavailable") return "neutral";
  return "bad";
}

function ProviderForm({ provider, onClose, onSaved }: { provider: Provider | null; onClose: () => void; onSaved: () => Promise<void> }) {
  const initialKind = provider?.kind ?? "open_ai";
  const [kind, setKind] = useState<ProviderKind>(initialKind);
  const [name, setName] = useState(provider?.name ?? "OpenAI");
  const [baseUrl, setBaseUrl] = useState(provider?.base_url ?? providerDefaults.open_ai);
  const [privacy, setPrivacy] = useState<PrivacyClass>(provider?.privacy ?? "cloud");
  const [enabled, setEnabled] = useState(provider?.enabled ?? true);
  const [apiKey, setApiKey] = useState("");
  const [clearApiKey, setClearApiKey] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function changeKind(next: ProviderKind) {
    setKind(next);
    if (next === "anthropic" || next === "gemini") return;
    setBaseUrl(providerDefaults[next]);
    setPrivacy(next === "ollama" || next === "lm_studio" ? "local" : "cloud");
    const labels: Record<Exclude<ProviderKind, "anthropic" | "gemini">, string> = {
      open_ai_compatible: "OpenAI-compatible",
      open_ai: "OpenAI",
      ollama: "Ollama",
      lm_studio: "LM Studio",
    };
    setName(labels[next]);
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);

    // JavaScript strings cannot be zeroized. Clear the controlled input immediately
    // after invoke has serialized the transient replacement secret.
    let transientSecret = apiKey.trim();
    const request: ProviderUpsert = {
      id: provider?.id ?? null,
      kind,
      name: name.trim(),
      base_url: baseUrl.trim(),
      enabled,
      privacy,
      api_key: transientSecret || null,
      clear_api_key: clearApiKey,
    };
    setApiKey("");
    const pending = daemon.upsertProvider(request);
    transientSecret = "";
    request.api_key = null;
    try {
      await pending;
      await onSaved();
      onClose();
    } catch (reason) {
      setError(commandMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title={provider ? "Configure provider" : "Add provider"} onClose={onClose}>
      <form className="form-stack" onSubmit={submit} autoComplete="off">
        <Field label="Provider type">
          <Select value={kind} onChange={(event) => changeKind(event.target.value as ProviderKind)}>
            <option value="open_ai">OpenAI</option>
            <option value="open_ai_compatible">Custom OpenAI-compatible</option>
            <option value="ollama">Ollama</option>
            <option value="lm_studio">LM Studio</option>
            <option value="anthropic" disabled>Anthropic — adapter not available yet</option>
            <option value="gemini" disabled>Gemini — adapter not available yet</option>
          </Select>
        </Field>
        <Field label="Display name"><Input required maxLength={80} value={name} onChange={(event) => setName(event.target.value)} /></Field>
        <Field label="Base URL" hint="HTTPS is required except for explicit loopback local providers.">
          <Input required inputMode="url" spellCheck={false} value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} />
        </Field>
        <Field label={provider?.configured ? "Replace API key" : "API key"} hint="Sent once to the daemon and stored in the OS credential service. It is never returned.">
          <Input type="password" autoComplete="new-password" spellCheck={false} value={apiKey} onChange={(event) => setApiKey(event.target.value)} />
        </Field>
        {provider?.configured ? <Toggle checked={clearApiKey} onChange={setClearApiKey} label="Remove stored credential" description="The provider remains configured without a key." /> : null}
        <Field label="Privacy class">
          <Select value={privacy} onChange={(event) => setPrivacy(event.target.value as PrivacyClass)}>
            <option value="cloud">Cloud</option>
            <option value="local">Local</option>
          </Select>
        </Field>
        <Toggle checked={enabled} onChange={setEnabled} label="Provider enabled" />
        {error ? <ErrorNotice message={error} /> : null}
        <div className="modal-actions"><Button type="button" onClick={onClose}>Cancel</Button><Button variant="primary" type="submit" disabled={busy}>Save provider</Button></div>
      </form>
    </Modal>
  );
}

function ModelForm({ providers, onClose, onSaved }: { providers: Provider[]; onClose: () => void; onSaved: () => Promise<void> }) {
  const [providerId, setProviderId] = useState(providers[0]?.id ?? "");
  const [displayName, setDisplayName] = useState("");
  const [modelId, setModelId] = useState("");
  const [privacy, setPrivacy] = useState<PrivacyClass>(providers[0]?.privacy ?? "cloud");
  const [priority, setPriority] = useState(100);
  const [contextLimit, setContextLimit] = useState("");
  const [isDefault, setIsDefault] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    const parsedContext = contextLimit ? Number(contextLimit) : null;
    const input: ModelUpsert = {
      id: null,
      provider_id: providerId,
      display_name: displayName.trim(),
      provider_model_id: modelId.trim(),
      enabled: true,
      context_limit: parsedContext,
      privacy,
      priority,
      task_eligibility: ["conversation"],
      is_default: isDefault,
    };
    setBusy(true);
    setError(null);
    try {
      await daemon.upsertModel(input);
      await onSaved();
      onClose();
    } catch (reason) {
      setError(commandMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title="Add model" onClose={onClose}>
      <form className="form-stack" onSubmit={submit}>
        <Field label="Provider"><Select required value={providerId} onChange={(event) => { const next = event.target.value; setProviderId(next); setPrivacy(providers.find((item) => item.id === next)?.privacy ?? "cloud"); }}>{providers.map((provider) => <option key={provider.id} value={provider.id}>{provider.name}</option>)}</Select></Field>
        <Field label="Display name"><Input required maxLength={80} value={displayName} onChange={(event) => setDisplayName(event.target.value)} /></Field>
        <Field label="Provider model ID"><Input required maxLength={160} spellCheck={false} value={modelId} onChange={(event) => setModelId(event.target.value)} /></Field>
        <div className="two-column">
          <Field label="Context limit" hint="Optional"><Input type="number" min="256" max="2000000" value={contextLimit} onChange={(event) => setContextLimit(event.target.value)} /></Field>
          <Field label="Routing priority"><Input type="number" min="0" max="1000" value={priority} onChange={(event) => setPriority(Number(event.target.value))} /></Field>
        </div>
        <Field label="Privacy class"><Select value={privacy} onChange={(event) => setPrivacy(event.target.value as PrivacyClass)}><option value="cloud">Cloud</option><option value="local">Local</option></Select></Field>
        <Toggle checked={isDefault} onChange={setIsDefault} label="Default conversation model" />
        {error ? <ErrorNotice message={error} /> : null}
        <div className="modal-actions"><Button type="button" onClick={onClose}>Cancel</Button><Button variant="primary" type="submit" disabled={busy || !providerId}>Add model</Button></div>
      </form>
    </Modal>
  );
}

export function AiScreen() {
  const [tab, setTab] = useState<AiTab>("providers");
  const [providers, setProviders] = useState<Provider[]>([]);
  const [models, setModels] = useState<AiModel[]>([]);
  const [settings, setSettings] = useState<ApplicationSettings | null>(null);
  const [usage, setUsage] = useState<UsageStats | null>(null);
  const [providerForm, setProviderForm] = useState<Provider | "new" | null>(null);
  const [modelForm, setModelForm] = useState(false);
  const [testing, setTesting] = useState<string | null>(null);
  const [testMessage, setTestMessage] = useState<Record<string, string>>({});
  const [previewRequest, setPreviewRequest] = useState("");
  const [preview, setPreview] = useState<PromptPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function load() {
    setBusy(true);
    setError(null);
    try {
      const [nextProviders, nextModels, nextSettings, nextUsage] = await Promise.all([
        daemon.providers(), daemon.models(), daemon.settings(), daemon.usage(),
      ]);
      setProviders(nextProviders);
      setModels(nextModels);
      setSettings(nextSettings);
      setUsage(nextUsage);
    } catch (reason) {
      setError(commandMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => { void load(); }, []);

  const modelCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const model of models) counts.set(model.provider_id, (counts.get(model.provider_id) ?? 0) + 1);
    return counts;
  }, [models]);

  async function testProvider(provider: Provider) {
    setTesting(provider.id);
    try {
      const result = await daemon.testProvider(provider.id);
      setTestMessage((current) => ({ ...current, [provider.id]: result.message }));
      await load();
    } catch (reason) {
      setTestMessage((current) => ({ ...current, [provider.id]: commandMessage(reason) }));
    } finally {
      setTesting(null);
    }
  }

  async function removeProvider(provider: Provider) {
    if (!window.confirm(`Remove ${provider.name}? Its stored credential will also be deleted.`)) return;
    try {
      await daemon.removeProvider(provider.id);
      await load();
    } catch (reason) {
      setError(commandMessage(reason));
    }
  }

  async function updateModel(model: AiModel, changes: Partial<AiModel>) {
    try {
      await daemon.upsertModel({ ...model, ...changes });
      await load();
    } catch (reason) {
      setError(commandMessage(reason));
    }
  }

  async function saveSettings(next: ApplicationSettings) {
    setBusy(true);
    try {
      setSettings(await daemon.updateSettings(next));
    } catch (reason) {
      setError(commandMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  async function showPreview() {
    const message = previewRequest.trim();
    if (!message) return;
    try {
      setPreview(await daemon.preview({ session_id: null, message, model_selection: { kind: "automatic" } }));
    } catch (reason) {
      setError(commandMessage(reason));
    }
  }

  const totalResolved = usage ? usage.local_resolutions + usage.ai_fallbacks : 0;
  const fallbackRate = usage && totalResolved > 0 ? Math.round((usage.ai_fallbacks / totalResolved) * 100) : 0;

  return (
    <div className="page">
      <PageHeader title="AI" description="Optional reasoning providers. Local deterministic routes always run first." actions={<Button disabled={busy} onClick={() => void load()}><RefreshCw size={16} /> Refresh</Button>} />
      <Tabs value={tab} onChange={setTab} items={[
        { value: "providers", label: "Providers" }, { value: "models", label: "Models" },
        { value: "routing", label: "Routing" }, { value: "usage", label: "Usage" }, { value: "prompts", label: "Prompts" },
      ]} />
      {error ? <ErrorNotice message={error} onRetry={() => void load()} /> : null}

      {tab === "providers" ? <section className="section-stack">
        <div className="section-heading"><div><h2>Providers</h2><p>Credentials are stored by the daemon in the OS keyring.</p></div><Button variant="primary" onClick={() => setProviderForm("new")}><Plus size={16} /> Add provider</Button></div>
        {providers.length === 0 ? <EmptyState title="No AI provider configured" description="Halquen remains usable for local commands and memory. Cloud access is off by default." action={<Button onClick={() => setProviderForm("new")}>Add provider</Button>} /> :
          <div className="provider-grid">{providers.map((provider) => <article className="provider-card" key={provider.id}>
            <header><div className="provider-icon"><Bot size={19} /></div><div><strong>{provider.name}</strong><span>{provider.kind.replaceAll("_", " ")}</span></div><StatusBadge tone={statusTone(provider.status)}>{provider.status.replaceAll("_", " ")}</StatusBadge></header>
            <dl><div><dt>Route</dt><dd>{provider.privacy}</dd></div><div><dt>Models</dt><dd>{modelCounts.get(provider.id) ?? 0}</dd></div><div><dt>Credential</dt><dd>{provider.configured ? "Stored in keyring" : "Not configured"}</dd></div></dl>
            <p className="endpoint" title={provider.base_url}>{provider.base_url}</p>
            {testMessage[provider.id] ? <small>{testMessage[provider.id]}</small> : null}
            <footer><Button onClick={() => setProviderForm(provider)}>Configure</Button><Button disabled={testing === provider.id} onClick={() => void testProvider(provider)}><Zap size={15} /> Test</Button><Button variant="ghost" aria-label={`Remove ${provider.name}`} onClick={() => void removeProvider(provider)}><Trash2 size={16} /></Button></footer>
          </article>)}</div>}
      </section> : null}

      {tab === "models" ? <section className="section-stack">
        <div className="section-heading"><div><h2>Models</h2><p>Automatic routing is preferred. Manual selection cannot bypass privacy policy.</p></div><Button variant="primary" disabled={providers.length === 0} onClick={() => setModelForm(true)}><Plus size={16} /> Add model</Button></div>
        {models.length === 0 ? <EmptyState title="No models configured" description="Add a provider first, then enter an exact provider model ID." /> : <div className="data-table model-table">
          <div className="data-row data-head"><span>Model</span><span>Provider</span><span>Privacy</span><span>Priority</span><span>Status</span></div>
          {models.map((model) => <div className="data-row" key={model.id}><span><strong>{model.display_name}</strong><small>{model.provider_model_id}</small></span><span>{providers.find((provider) => provider.id === model.provider_id)?.name ?? "Unknown"}</span><span><StatusBadge>{model.privacy}</StatusBadge></span><span>{model.priority}</span><span><label className="compact-switch"><input type="checkbox" checked={model.enabled} onChange={(event) => void updateModel(model, { enabled: event.target.checked })} /> Enabled</label>{!model.is_default ? <Button variant="ghost" onClick={() => void updateModel(model, { is_default: true })}>Make default</Button> : <StatusBadge tone="good">Default</StatusBadge>}</span></div>)}
        </div>}
      </section> : null}

      {tab === "routing" && settings ? <section className="settings-sections narrow-section">
        <div className="settings-group"><h2>Optimization goal</h2><p>High-level presets map to deterministic router policy.</p><Field label="Routing preset"><Select value={settings.routing_preset} onChange={(event) => void saveSettings({ ...settings, routing_preset: event.target.value as ApplicationSettings["routing_preset"] })}><option value="balanced">Balanced</option><option value="minimize_ai_usage">Minimize AI usage</option><option value="minimize_cost">Minimize cost</option><option value="prefer_local">Prefer local</option><option value="prefer_quality">Prefer quality</option><option value="custom">Custom</option></Select></Field></div>
        <div className="settings-group"><h2>Eligible routes</h2><Toggle checked={settings.allow_local_ai} onChange={(checked) => void saveSettings({ ...settings, allow_local_ai: checked })} label="Allow local AI providers" description="External local providers such as Ollama remain independently managed." /><Toggle checked={settings.allow_cloud_ai} onChange={(checked) => void saveSettings({ ...settings, allow_cloud_ai: checked })} label="Allow cloud AI" description="Off by default. Provider and context privacy rules still apply." /></div>
        <div className="settings-group"><h2>Request budgets</h2><div className="two-column"><Field label="Maximum model calls"><Input type="number" min="0" max="3" value={settings.max_model_calls_per_request} onChange={(event) => setSettings({ ...settings, max_model_calls_per_request: Number(event.target.value) })} onBlur={() => void saveSettings(settings)} /></Field><Field label="Maximum output tokens"><Input type="number" min="64" max="16384" value={settings.max_output_tokens} onChange={(event) => setSettings({ ...settings, max_output_tokens: Number(event.target.value) })} onBlur={() => void saveSettings(settings)} /></Field></div><Field label="Maximum AI context tokens"><Input type="number" min="256" max="131072" value={settings.max_context_tokens} onChange={(event) => setSettings({ ...settings, max_context_tokens: Number(event.target.value) })} onBlur={() => void saveSettings(settings)} /></Field><Toggle checked={settings.prefer_cached_local} onChange={(checked) => void saveSettings({ ...settings, prefer_cached_local: checked })} label="Prefer validated local reuse" /><Toggle checked={settings.allow_expensive_fallback} onChange={(checked) => void saveSettings({ ...settings, allow_expensive_fallback: checked })} label="Allow expensive fallback" /></div>
      </section> : null}

      {tab === "usage" ? <section className="section-stack"><div className="section-heading"><div><h2>Efficiency</h2><p>Token savings are estimates; provider token counts are recorded when available.</p></div></div>{usage ? <><div className="metric-grid"><article><span>AI fallback rate</span><strong>{fallbackRate}%</strong><small>{usage.ai_fallbacks.toLocaleString()} fallback requests</small></article><article><span>Resolved without AI</span><strong>{usage.local_resolutions.toLocaleString()}</strong><small>{usage.response_cache_hits.toLocaleString()} validated cache hits</small></article><article><span>Provider tokens</span><strong>{(usage.input_tokens + usage.output_tokens).toLocaleString()}</strong><small>{usage.input_tokens.toLocaleString()} in · {usage.output_tokens.toLocaleString()} out</small></article><article><span>Estimated tokens avoided</span><strong>≈{usage.estimated_tokens_avoided.toLocaleString()}</strong><small>Estimated baseline, not billing data</small></article></div><div className="usage-list"><div><span>Model requests</span><strong>{usage.model_requests}</strong></div><div><span>Failed provider calls</span><strong>{usage.failed_provider_calls}</strong></div><div><span>Clarifications</span><strong>{usage.clarifications}</strong></div><div><span>Provider cached tokens</span><strong>{usage.cached_tokens}</strong></div></div></> : null}</section> : null}

      {tab === "prompts" && settings ? <section className="settings-sections narrow-section"><div className="settings-group"><div className="managed-contract"><ShieldCheck size={20} /><div><h2>Halquen core instructions</h2><p>Managed by Halquen. They define the non-removable security boundary for every model call.</p></div><StatusBadge tone="good">Protected</StatusBadge></div></div><div className="settings-group"><h2>Personal instructions</h2><p>Style and preference guidance. It cannot override capability, policy, privacy, or memory validation.</p><TextArea rows={7} maxLength={8_000} value={settings.personal_instructions} onChange={(event) => setSettings({ ...settings, personal_instructions: event.target.value })} /><div className="button-row"><Button variant="primary" onClick={() => void saveSettings(settings)}>Save instructions</Button></div></div><div className="settings-group"><h2>Preview AI request</h2><p>Inspect sanitized route/context metadata without secrets or hidden reasoning.</p><TextArea rows={3} placeholder="Enter a representative request…" value={previewRequest} onChange={(event) => setPreviewRequest(event.target.value)} /><Button onClick={() => void showPreview()}>Preview request</Button>{preview ? <dl className="detail-list preview-result"><div><dt>Task</dt><dd>{preview.task.replaceAll("_", " ")}</dd></div><div><dt>Estimated context</dt><dd>{preview.estimated_context_tokens} tokens</dd></div><div><dt>Categories</dt><dd>{preview.context_categories.join(", ") || "Current request only"}</dd></div><div><dt>Core contract</dt><dd>Managed</dd></div></dl> : null}</div></section> : null}

      {providerForm ? <ProviderForm provider={providerForm === "new" ? null : providerForm} onClose={() => setProviderForm(null)} onSaved={load} /> : null}
      {modelForm ? <ModelForm providers={providers} onClose={() => setModelForm(false)} onSaved={load} /> : null}
    </div>
  );
}
