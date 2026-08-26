import { useEffect, useState, type FormEvent } from "react";
import { RefreshCw, Save } from "lucide-react";
import { Button, ErrorNotice, Field, Input, PageHeader, Select, TextArea, Toggle } from "../components/Common";
import { commandMessage, daemon } from "../lib/daemon";
import type { ApplicationSettings } from "../lib/types";

export function SettingsScreen({ onAppearanceChange }: { onAppearanceChange: (appearance: ApplicationSettings["appearance"]) => void }) {
  const [settings, setSettings] = useState<ApplicationSettings | null>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function load() {
    setBusy(true);
    setError(null);
    try {
      const loaded = await daemon.settings();
      setSettings(loaded);
      onAppearanceChange(loaded.appearance);
    } catch (reason) {
      setError(commandMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => { void load(); }, []);

  async function save(event: FormEvent) {
    event.preventDefault();
    if (!settings) return;
    setBusy(true);
    setSaved(false);
    setError(null);
    try {
      const updated = await daemon.updateSettings(settings);
      setSettings(updated);
      onAppearanceChange(updated.appearance);
      setSaved(true);
    } catch (reason) {
      setError(commandMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="page">
      <PageHeader title="Settings" description="Conservative defaults keep AI optional, context bounded, and cloud access disabled." actions={<Button disabled={busy} onClick={() => void load()}><RefreshCw size={16} /> Reload</Button>} />
      {error ? <ErrorNotice message={error} onRetry={() => void load()} /> : null}
      {settings ? <form className="settings-sections" onSubmit={save}>
        <section className="settings-group">
          <h2>General</h2>
          <p>Only settings supported by the daemon are shown.</p>
          <Field label="Language" hint="Used as a response preference; interface translation is not yet included.">
            <Input maxLength={32} value={settings.language} onChange={(event) => setSettings({ ...settings, language: event.target.value })} />
          </Field>
        </section>

        <section className="settings-group">
          <h2>Appearance</h2>
          <Field label="Theme">
            <Select value={settings.appearance} onChange={(event) => { const appearance = event.target.value as ApplicationSettings["appearance"]; setSettings({ ...settings, appearance }); onAppearanceChange(appearance); }}>
              <option value="system">System</option><option value="light">Light</option><option value="dark">Dark</option>
            </Select>
          </Field>
        </section>

        <section className="settings-group">
          <h2>AI &amp; routing</h2>
          <Toggle checked={settings.allow_local_ai} onChange={(checked) => setSettings({ ...settings, allow_local_ai: checked })} label="Allow local AI" description="Permits configured local providers after local deterministic routes miss." />
          <Toggle checked={settings.allow_cloud_ai} onChange={(checked) => setSettings({ ...settings, allow_cloud_ai: checked })} label="Allow cloud AI" description="Content can leave this device only when provider and context privacy rules also allow it." />
          <Toggle checked={settings.prefer_cached_local} onChange={(checked) => setSettings({ ...settings, prefer_cached_local: checked })} label="Prefer validated local reuse" />
          <Toggle checked={settings.allow_expensive_fallback} onChange={(checked) => setSettings({ ...settings, allow_expensive_fallback: checked })} label="Allow expensive fallback" />
          <Field label="Optimization goal"><Select value={settings.routing_preset} onChange={(event) => setSettings({ ...settings, routing_preset: event.target.value as ApplicationSettings["routing_preset"] })}><option value="balanced">Balanced</option><option value="minimize_ai_usage">Minimize AI usage</option><option value="minimize_cost">Minimize cost</option><option value="prefer_local">Prefer local</option><option value="prefer_quality">Prefer quality</option><option value="custom">Custom</option></Select></Field>
        </section>

        <section className="settings-group">
          <h2>Memory &amp; learning</h2>
          <Toggle checked={settings.learning_enabled} onChange={(checked) => setSettings({ ...settings, learning_enabled: checked })} label="Enable learning from confirmed interactions" />
          <Toggle checked={settings.ask_before_procedural_rules} onChange={(checked) => setSettings({ ...settings, ask_before_procedural_rules: checked })} label="Ask before creating procedural rules" description="Recommended because procedures can propose future actions." />
          <Toggle checked={settings.auto_save_explicit_preferences} onChange={(checked) => setSettings({ ...settings, auto_save_explicit_preferences: checked })} label="Automatically save explicit preferences" description="Only direct, locally parsed user statements receive UserExplicit evidence." />
          <div className="two-column">
            <Field label="Conversation retention (days)"><Input type="number" min="1" max="3650" value={settings.conversation_retention_days} onChange={(event) => setSettings({ ...settings, conversation_retention_days: Number(event.target.value) })} /></Field>
            <Field label="Episodic retention (days)"><Input type="number" min="1" max="3650" value={settings.episodic_retention_days} onChange={(event) => setSettings({ ...settings, episodic_retention_days: Number(event.target.value) })} /></Field>
          </div>
        </section>

        <section className="settings-group">
          <h2>Privacy &amp; security</h2>
          <Toggle checked={settings.allow_personal_context} onChange={(checked) => setSettings({ ...settings, allow_personal_context: checked })} label="Allow selected personal context in AI requests" description="The daemon still builds a bounded projection; it never sends the entire database or history." />
          <div className="privacy-note"><strong>Secrets</strong><p>Provider credentials are held by the OS credential service. They are not stored in SQLite and cannot be retrieved through the GUI protocol.</p></div>
        </section>

        <section className="settings-group">
          <h2>Logging &amp; diagnostics</h2>
          <Toggle checked={settings.diagnostic_logging} onChange={(checked) => setSettings({ ...settings, diagnostic_logging: checked })} label="Diagnostic logging" description="Prompts, credentials, authorization headers, and raw private files are excluded." />
          <div className="two-column">
            <Field label="Log level"><Select value={settings.log_level} onChange={(event) => setSettings({ ...settings, log_level: event.target.value as ApplicationSettings["log_level"] })}><option value="error">Error</option><option value="warn">Warning</option><option value="info">Info</option><option value="debug">Debug</option></Select></Field>
            <Field label="Retention (days)"><Input type="number" min="1" max="30" value={settings.log_retention_days} onChange={(event) => setSettings({ ...settings, log_retention_days: Number(event.target.value) })} /></Field>
          </div>
          <Field label="Maximum total log size (MiB)"><Input type="number" min="1" max="256" value={settings.log_max_total_mb} onChange={(event) => setSettings({ ...settings, log_max_total_mb: Number(event.target.value) })} /></Field>
          <p className="muted">Log changes are persisted now; the daemon applies its active subscriber level and rotation policy on restart.</p>
        </section>

        <section className="settings-group">
          <h2>Advanced</h2>
          <p>Budgets are validated again by the daemon and have hard upper bounds.</p>
          <div className="two-column">
            <Field label="Model calls per request"><Input type="number" min="0" max="3" value={settings.max_model_calls_per_request} onChange={(event) => setSettings({ ...settings, max_model_calls_per_request: Number(event.target.value) })} /></Field>
            <Field label="Output token budget"><Input type="number" min="64" max="16384" value={settings.max_output_tokens} onChange={(event) => setSettings({ ...settings, max_output_tokens: Number(event.target.value) })} /></Field>
          </div>
          <Field label="Context token budget"><Input type="number" min="256" max="131072" value={settings.max_context_tokens} onChange={(event) => setSettings({ ...settings, max_context_tokens: Number(event.target.value) })} /></Field>
          <Field label="Personal instructions" hint="Cannot override Halquen's managed security contract."><TextArea rows={6} maxLength={8_000} value={settings.personal_instructions} onChange={(event) => setSettings({ ...settings, personal_instructions: event.target.value })} /></Field>
        </section>

        <div className="settings-save"><Button variant="primary" type="submit" disabled={busy}><Save size={16} /> Save settings</Button>{saved ? <span role="status">Settings saved.</span> : null}</div>
      </form> : null}
    </div>
  );
}
