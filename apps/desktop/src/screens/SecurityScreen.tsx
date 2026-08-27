import { useEffect, useState, type FormEvent } from "react";
import { LockKeyhole, Play, Plus, RefreshCw, ShieldAlert, Trash2 } from "lucide-react";
import {
  Button,
  EmptyState,
  ErrorNotice,
  Field,
  Input,
  PageHeader,
  Select,
  StatusBadge,
  Tabs,
  Toggle,
} from "../components/Common";
import { commandMessage, daemon } from "../lib/daemon";
import type {
  AgentConfiguration,
  AgentRunResult,
  AgentSession,
  PermissionEffect,
  PermissionGrant,
  PermissionLifetime,
  ResourceClassification,
  ResourceKind,
  ResourceLabel,
  RegisteredApplication,
  SecurityOverview,
  SecurityProfile,
} from "../lib/types";

type Tab = "overview" | "permissions" | "resources" | "agents" | "applications";

function target(grant: PermissionGrant): string {
  return grant.scope.arguments.kind === "open_app" ? grant.scope.arguments.app : "exact arguments";
}

export function SecurityScreen() {
  const [tab, setTab] = useState<Tab>("overview");
  const [overview, setOverview] = useState<SecurityOverview | null>(null);
  const [permissions, setPermissions] = useState<PermissionGrant[]>([]);
  const [labels, setLabels] = useState<ResourceLabel[]>([]);
  const [agents, setAgents] = useState<AgentConfiguration[]>([]);
  const [agentSessions, setAgentSessions] = useState<AgentSession[]>([]);
  const [applications, setApplications] = useState<RegisteredApplication[]>([]);
  const [agentRun, setAgentRun] = useState<AgentRunResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [permissionApp, setPermissionApp] = useState("app:telegram");
  const [permissionEffect, setPermissionEffect] = useState<PermissionEffect>("allow");
  const [permissionLifetime, setPermissionLifetime] = useState<PermissionLifetime>("always");
  const [permissionAgentId, setPermissionAgentId] = useState("");

  const [labelName, setLabelName] = useState("");
  const [labelPattern, setLabelPattern] = useState("");
  const [labelKind, setLabelKind] = useState<ResourceKind>("file");
  const [labelClass, setLabelClass] = useState<ResourceClassification>("sensitive");

  const [agentName, setAgentName] = useState("");
  const [agentExecutable, setAgentExecutable] = useState("");
  const [agentArguments, setAgentArguments] = useState("");
  const [agentSandbox, setAgentSandbox] = useState<AgentConfiguration["sandbox"]>("bubblewrap");
  const [agentEnabled, setAgentEnabled] = useState(false);
  const [agentInput, setAgentInput] = useState("Propose a safe action");

  const [applicationEntity, setApplicationEntity] = useState("app:");
  const [applicationName, setApplicationName] = useState("");
  const [applicationExecutable, setApplicationExecutable] = useState("");

  async function load() {
    setBusy(true);
    setError(null);
    try {
      const [nextOverview, nextPermissions, nextLabels, nextAgents, nextSessions, nextApplications] = await Promise.all([
        daemon.securityOverview(),
        daemon.permissions(),
        daemon.resourceLabels(),
        daemon.agents(),
        daemon.agentSessions(),
        daemon.applications(),
      ]);
      setOverview(nextOverview);
      setPermissions(nextPermissions);
      setLabels(nextLabels);
      setAgents(nextAgents);
      setAgentSessions(nextSessions);
      setApplications(nextApplications);
    } catch (reason) {
      setError(commandMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => { void load(); }, []);

  async function changeProfile(profile: SecurityProfile) {
    setBusy(true);
    setError(null);
    try {
      const saved = await daemon.updateSecurityProfile(profile);
      setOverview((current) => current ? { ...current, profile: saved } : current);
    } catch (reason) {
      setError(commandMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  async function addPermission(event: FormEvent) {
    event.preventDefault();
    const app = permissionApp.trim().toLowerCase();
    if (!app.startsWith("app:")) return;
    setBusy(true);
    setError(null);
    try {
      await daemon.upsertPermission({
        id: null,
        effect: permissionEffect,
        lifetime: permissionLifetime,
        capability_id: "system.open_app",
        arguments: { kind: "open_app", app },
        resources: [{ kind: "application", identifier: app, classification: "local" }],
        destination: null,
        session: null,
        agent_id: permissionAgentId || null,
        expires_at_ms: permissionLifetime === "until" ? Date.now() + 24 * 60 * 60 * 1_000 : null,
      });
      await load();
    } catch (reason) {
      setError(commandMessage(reason));
      setBusy(false);
    }
  }

  async function revokePermission(id: string) {
    setBusy(true);
    setError(null);
    try {
      await daemon.revokePermission(id);
      await load();
    } catch (reason) {
      setError(commandMessage(reason));
      setBusy(false);
    }
  }

  async function addLabel(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await daemon.upsertResourceLabel({
        id: null,
        name: labelName.trim(),
        resource_kind: labelKind,
        match_kind: labelKind === "file" ? "path_prefix" : labelKind === "network_endpoint" ? "host" : "exact",
        pattern: labelPattern.trim(),
        classification: labelClass,
        data_classification: labelClass === "system_critical" ? "sensitive" : labelClass === "local" ? "personal" : labelClass,
      });
      setLabelName("");
      setLabelPattern("");
      await load();
    } catch (reason) {
      setError(commandMessage(reason));
      setBusy(false);
    }
  }

  async function removeLabel(id: string) {
    setBusy(true);
    setError(null);
    try {
      await daemon.removeResourceLabel(id);
      await load();
    } catch (reason) {
      setError(commandMessage(reason));
      setBusy(false);
    }
  }

  async function addAgent(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await daemon.upsertAgent({
        id: null,
        name: agentName.trim(),
        transport: "cli",
        executable: agentExecutable.trim(),
        arguments: agentArguments.split("\n").map((value) => value.trim()).filter(Boolean),
        socket_path: null,
        sandbox: agentSandbox,
        ownership: "root_or_current_user",
        sha256_hex: null,
        resource_limits: {
          cpu_seconds: 30,
          memory_bytes: 536_870_912,
          process_count: 64,
          file_size_bytes: 16_777_216,
          open_files: 128,
          temp_bytes: 67_108_864,
        },
        enabled: agentEnabled,
        timeout_ms: 30_000,
        max_stdout_bytes: 65_536,
        max_stderr_bytes: 16_384,
      });
      setAgentName("");
      setAgentExecutable("");
      setAgentArguments("");
      setAgentEnabled(false);
      await load();
    } catch (reason) {
      setError(commandMessage(reason));
      setBusy(false);
    }
  }

  async function removeAgent(id: string) {
    setBusy(true);
    setError(null);
    try {
      await daemon.removeAgent(id);
      await load();
    } catch (reason) {
      setError(commandMessage(reason));
      setBusy(false);
    }
  }

  async function runAgent(id: string) {
    if (!agentInput.trim()) return;
    setBusy(true);
    setError(null);
    try {
      setAgentRun(await daemon.runAgent(id, agentInput.trim()));
      await load();
    } catch (reason) {
      setError(commandMessage(reason));
      setBusy(false);
    }
  }

  async function addApplication(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await daemon.upsertApplication({
        entity_id: applicationEntity.trim().toLowerCase(),
        display_name: applicationName.trim(),
        executable: applicationExecutable.trim(),
        arguments: [],
        ownership: "root_or_current_user",
        sha256_hex: null,
        enabled: true,
      });
      setApplicationEntity("app:");
      setApplicationName("");
      setApplicationExecutable("");
      await load();
    } catch (reason) {
      setError(commandMessage(reason));
      setBusy(false);
    }
  }

  async function removeApplication(entityId: string) {
    setBusy(true);
    setError(null);
    try {
      await daemon.removeApplication(entityId);
      await load();
    } catch (reason) {
      setError(commandMessage(reason));
      setBusy(false);
    }
  }

  return (
    <div className="page">
      <PageHeader
        title="Security"
        description="Permissions are exact, revocable and subordinate to immutable hard-deny rules. AI and agents can propose actions but cannot grant authority."
        actions={<Button disabled={busy} onClick={() => void load()}><RefreshCw size={16} /> Reload</Button>}
      />
      {error ? <ErrorNotice message={error} onRetry={() => void load()} /> : null}
      <Tabs value={tab} onChange={setTab} items={[
        { value: "overview", label: "Overview" },
        { value: "permissions", label: "Permissions" },
        { value: "resources", label: "Resources" },
        { value: "agents", label: "Agents" },
        { value: "applications", label: "Applications" },
      ]} />

      {tab === "overview" && overview ? (
        <div className="security-grid">
          <section className="settings-group">
            <h2>Security profile</h2>
            <p>Profiles tune defaults. Immutable rules remain active in every profile.</p>
            <Select value={overview.profile} disabled={busy} onChange={(event) => void changeProfile(event.target.value as SecurityProfile)}>
              <option value="strict">Strict</option>
              <option value="balanced">Balanced</option>
              <option value="developer">Developer</option>
              <option value="custom">Custom</option>
            </Select>
          </section>
          <section className="settings-group">
            <h2>Authority state</h2>
            <dl className="security-counts">
              <div><dt>Active permissions</dt><dd>{overview.active_permissions}</dd></div>
              <div><dt>Resource labels</dt><dd>{overview.resource_labels}</dd></div>
              <div><dt>Configured agents</dt><dd>{overview.configured_agents}</dd></div>
              <div><dt>Active agent sessions</dt><dd>{overview.active_agent_sessions}</dd></div>
              <div><dt>Registered applications</dt><dd>{overview.registered_applications}</dd></div>
            </dl>
          </section>
          <section className="settings-group security-wide">
            <h2>Immutable core rules</h2>
            <p>Confirmation and persistent grants cannot override these rules.</p>
            <div className="rule-list">{overview.immutable_rule_ids.map((rule) => <code key={rule}>{rule}</code>)}</div>
          </section>
        </div>
      ) : null}

      {tab === "permissions" ? (
        <div className="security-split">
          <form className="settings-group form-stack" onSubmit={addPermission}>
            <h2>Create exact application permission</h2>
            <Field label="Application entity" hint="Exact typed scope, for example app:telegram."><Input required pattern="app:[a-z0-9_:-]+" value={permissionApp} onChange={(event) => setPermissionApp(event.target.value)} /></Field>
            <div className="two-column">
              <Field label="Decision"><Select value={permissionEffect} onChange={(event) => setPermissionEffect(event.target.value as PermissionEffect)}><option value="allow">Allow</option><option value="deny">Deny</option></Select></Field>
              <Field label="Lifetime"><Select value={permissionLifetime} onChange={(event) => setPermissionLifetime(event.target.value as PermissionLifetime)}><option value="once">Once</option><option value="until">24 hours</option><option value="always">Always exact</option></Select></Field>
            </div>
            <Field label="Agent binding" hint="Agent proposals never match an unbound user permission.">
              <Select value={permissionAgentId} onChange={(event) => setPermissionAgentId(event.target.value)}>
                <option value="">User/local actions only</option>
                {agents.map((agent) => <option key={agent.id} value={agent.id}>{agent.name}</option>)}
              </Select>
            </Field>
            <Button type="submit" variant="primary" disabled={busy}><Plus size={16} /> Create permission</Button>
          </form>
          <section className="settings-group">
            <h2>Permission grants</h2>
            {permissions.length === 0 ? <EmptyState title="No grants" description="Baseline policy and hard rules are still active." /> : (
              <div className="security-list">{permissions.map((grant) => (
                <div className={grant.revoked_at_ms ? "security-row revoked" : "security-row"} key={grant.id}>
                  <div><strong>{grant.scope.capability_id}</strong><span>{target(grant)}{grant.agent_id ? ` · agent ${grant.agent_id}` : " · user/local only"}</span></div>
                  <StatusBadge tone={grant.effect === "allow" ? "good" : "bad"}>{grant.effect}</StatusBadge>
                  <span>{grant.lifetime}</span>
                  <Button variant="ghost" disabled={busy || grant.revoked_at_ms !== null} aria-label="Revoke" onClick={() => void revokePermission(grant.id)}><Trash2 size={15} /></Button>
                </div>
              ))}</div>
            )}
          </section>
        </div>
      ) : null}

      {tab === "resources" ? (
        <div className="security-split">
          <form className="settings-group form-stack" onSubmit={addLabel}>
            <h2>Classify resource</h2>
            <Field label="Label"><Input required maxLength={128} value={labelName} onChange={(event) => setLabelName(event.target.value)} /></Field>
            <Field label="Resource type"><Select value={labelKind} onChange={(event) => setLabelKind(event.target.value as ResourceKind)}><option value="file">Filesystem</option><option value="database">Database</option><option value="network_endpoint">Network endpoint</option><option value="application">Application</option><option value="system">System</option></Select></Field>
            <Field label="Exact value or prefix"><Input required maxLength={1024} value={labelPattern} onChange={(event) => setLabelPattern(event.target.value)} /></Field>
            <Field label="Classification"><Select value={labelClass} onChange={(event) => setLabelClass(event.target.value as ResourceClassification)}><option value="public">Public</option><option value="local">Local</option><option value="personal">Personal</option><option value="sensitive">Sensitive</option><option value="secret">Secret</option><option value="production">Production</option><option value="system_critical">System critical</option></Select></Field>
            <Button type="submit" variant="primary" disabled={busy}><Plus size={16} /> Add label</Button>
          </form>
          <section className="settings-group"><h2>Resource labels</h2><div className="security-list">{labels.map((label) => <div className="security-row" key={label.id}><div><strong>{label.name}</strong><span>{label.pattern}</span></div><StatusBadge tone={label.classification === "secret" || label.classification === "system_critical" ? "bad" : "warn"}>{label.classification}</StatusBadge><span>{label.resource_kind}</span><Button variant="ghost" disabled={busy} aria-label="Remove" onClick={() => void removeLabel(label.id)}><Trash2 size={15} /></Button></div>)}</div></section>
        </div>
      ) : null}

      {tab === "agents" ? (
        <div className="security-split">
          <form className="settings-group form-stack" onSubmit={addAgent}>
            <h2>Configure CLI agent</h2>
            <p>The executable receives bounded stdin and must return bounded typed JSON. No shell command strings are used.</p>
            <Field label="Name"><Input required maxLength={128} value={agentName} onChange={(event) => setAgentName(event.target.value)} /></Field>
            <Field label="Absolute executable path"><Input required maxLength={1024} placeholder="/usr/local/bin/my-agent" value={agentExecutable} onChange={(event) => setAgentExecutable(event.target.value)} /></Field>
            <Field label="Arguments" hint="One argument per line. No shell interpolation."><textarea className="textarea" value={agentArguments} onChange={(event) => setAgentArguments(event.target.value)} /></Field>
            <Field label="Sandbox"><Select value={agentSandbox} onChange={(event) => setAgentSandbox(event.target.value as AgentConfiguration["sandbox"])}><option value="bubblewrap">Bubblewrap (required backend)</option><option value="unavailable">Unavailable / fail closed</option><option value="unsafe_unsandboxed">Unsafe explicit opt-in</option></Select></Field>
            {agentSandbox === "unsafe_unsandboxed" ? <div className="danger-note"><ShieldAlert size={18} /><span>This agent is not isolated and can bypass Halquen. Keep it disabled unless you intentionally accept that risk.</span></div> : null}
            <Toggle checked={agentEnabled} onChange={setAgentEnabled} label="Enabled" description="Configuration does not automatically start the agent." />
            <Button type="submit" variant="primary" disabled={busy}><Plus size={16} /> Save agent</Button>
            <Field label="Broker request" hint="The agent receives only safe capability metadata. All proposals return to daemon policy.">
              <textarea className="textarea" value={agentInput} onChange={(event) => setAgentInput(event.target.value)} />
            </Field>
            {agentRun ? <div className="danger-note"><span>Last session: {agentRun.session.status}; {agentRun.proposals.length} proposal result(s).</span></div> : null}
          </form>
          <div className="form-stack">
            <section className="settings-group">
              <h2>Agent configurations</h2>
              {agents.length === 0 ? <EmptyState title="No agents" description="Built-in provider adapters remain available separately under AI." /> : (
                <div className="security-list">{agents.map((agent) => (
                  <div className="security-row" key={agent.id}>
                    <div><strong>{agent.name}</strong><span>{agent.executable}</span></div>
                    <StatusBadge tone={agent.sandbox === "bubblewrap" ? "good" : agent.sandbox === "unsafe_unsandboxed" ? "bad" : "warn"}>{agent.sandbox}</StatusBadge>
                    <Button variant="ghost" disabled={busy || !agent.enabled} aria-label={`Run ${agent.name}`} onClick={() => void runAgent(agent.id)}><Play size={15} /></Button>
                    <Button variant="ghost" disabled={busy} aria-label="Remove" onClick={() => void removeAgent(agent.id)}><Trash2 size={15} /></Button>
                  </div>
                ))}</div>
              )}
            </section>
            <section className="settings-group">
              <h2>Recent sessions</h2>
              {agentSessions.length === 0 ? <EmptyState title="No agent sessions" description="Sessions appear after a brokered run." /> : (
                <div className="security-list">{agentSessions.map((session) => (
                  <div className="security-row" key={session.id}>
                    <div><strong>{session.agent_id}</strong><span>{session.instance_id}</span></div>
                    <StatusBadge tone={session.status === "completed" ? "good" : session.status === "running" ? "warn" : "bad"}>{session.status}</StatusBadge>
                  </div>
                ))}</div>
              )}
            </section>
          </div>
        </div>
      ) : null}

      {tab === "applications" ? (
        <div className="security-split">
          <form className="settings-group form-stack" onSubmit={addApplication}>
            <h2>Register trusted application</h2>
            <p>The daemon records the executable identity. Action proposals reference only the typed application entity.</p>
            <Field label="Application entity"><Input required pattern="app:[a-z0-9_:-]+" value={applicationEntity} onChange={(event) => setApplicationEntity(event.target.value)} /></Field>
            <Field label="Display name"><Input required maxLength={128} value={applicationName} onChange={(event) => setApplicationName(event.target.value)} /></Field>
            <Field label="Canonical executable path"><Input required maxLength={1024} placeholder="/usr/bin/example" value={applicationExecutable} onChange={(event) => setApplicationExecutable(event.target.value)} /></Field>
            <Button type="submit" variant="primary" disabled={busy}><Plus size={16} /> Register application</Button>
          </form>
          <section className="settings-group">
            <h2>Trusted application registry</h2>
            {applications.length === 0 ? <EmptyState title="No applications" description="Real system.open_app execution fails closed until an application is registered." /> : (
              <div className="security-list">{applications.map((application) => (
                <div className="security-row" key={application.entity_id}>
                  <div><strong>{application.display_name}</strong><span>{application.entity_id} · {application.executable}</span></div>
                  <StatusBadge tone={application.enabled ? "good" : "warn"}>{application.enabled ? "enabled" : "disabled"}</StatusBadge>
                  <Button variant="ghost" disabled={busy} aria-label="Remove" onClick={() => void removeApplication(application.entity_id)}><Trash2 size={15} /></Button>
                </div>
              ))}</div>
            )}
          </section>
        </div>
      ) : null}

      {!overview && !error ? <EmptyState title="Loading security state" description="Reading daemon-owned policy, grants, resource labels and agent configuration." action={<LockKeyhole size={24} />} /> : null}
    </div>
  );
}
