use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use halquen_capabilities::{inspect_executable, verify_executable};
use halquen_domain::{
    ActionArgumentKind, ActionContext, ActionProposal, ActionRequest, AgentConfiguration,
    AgentExecutionIdentity, AgentTransport, CapabilityId, ExecutableOwnership, RiskClass,
    SandboxBackend,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, Take};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::task::JoinHandle;
use tokio::time::{Instant, timeout};

const AGENT_PROTOCOL_VERSION: u16 = 1;
const MAX_AGENT_INPUT_BYTES: usize = 64 * 1024;
const MAX_AGENT_PROPOSALS: usize = 16;
const MAX_AGENT_MESSAGE_BYTES: usize = 65_536;
const MAX_BROKER_RESULT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilityView {
    pub id: CapabilityId,
    pub version: u32,
    pub description: String,
    pub risk: RiskClass,
    pub arguments: ActionArgumentKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentBrokerDisposition {
    Executed,
    Simulated,
    ConfirmationRequired,
    Denied,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBrokerProposalResult {
    pub index: u16,
    pub capability_id: CapabilityId,
    pub disposition: AgentBrokerDisposition,
    pub execution_id: Option<String>,
    pub confirmation_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AgentBrokerRequest<'a> {
    version: u16,
    kind: &'static str,
    identity: &'a AgentExecutionIdentity,
    input: &'a str,
    capabilities: &'a [AgentCapabilityView],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AgentBrokerResponse<'a> {
    version: u16,
    kind: &'static str,
    results: &'a [AgentBrokerProposalResult],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentInvocationResult {
    pub message: String,
    pub proposals: Vec<ActionProposal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCompletion {
    pub stderr_bytes: usize,
}

#[derive(Debug, Error)]
pub enum AgentHostError {
    #[error("agent configuration is invalid")]
    InvalidConfiguration,
    #[error("agent is disabled")]
    Disabled,
    #[error("agent transport is not implemented")]
    UnsupportedTransport,
    #[error("required sandbox backend is unavailable")]
    SandboxUnavailable,
    #[error("required resource-limit backend is unavailable")]
    ResourceLimitUnavailable,
    #[error("unsafe unsandboxed execution requires explicit host opt-in")]
    UnsafeOptInRequired,
    #[error("configured executable identity is invalid")]
    InvalidExecutable,
    #[error("agent process could not be started")]
    SpawnFailed,
    #[error("agent process exceeded its deadline")]
    TimedOut,
    #[error("agent output exceeded its configured bound")]
    OutputTooLarge,
    #[error("agent output is malformed")]
    MalformedOutput,
    #[error("agent process failed")]
    ProcessFailed,
    #[error("agent I/O failed")]
    Io,
}

#[derive(Debug, Clone, Copy)]
pub struct AgentHost {
    allow_unsafe_unsandboxed: bool,
}

impl AgentHost {
    pub fn new() -> Self {
        Self {
            allow_unsafe_unsandboxed: false,
        }
    }

    pub fn with_unsafe_unsandboxed_opt_in() -> Self {
        Self {
            allow_unsafe_unsandboxed: true,
        }
    }

    pub async fn start(
        &self,
        configuration: &AgentConfiguration,
        identity: AgentExecutionIdentity,
        input: &str,
        capabilities: &[AgentCapabilityView],
    ) -> Result<RunningAgent, AgentHostError> {
        configuration
            .validate()
            .map_err(|_| AgentHostError::InvalidConfiguration)?;
        if !configuration.enabled {
            return Err(AgentHostError::Disabled);
        }
        if configuration.transport != AgentTransport::Cli {
            return Err(AgentHostError::UnsupportedTransport);
        }
        if input.len() > MAX_AGENT_INPUT_BYTES || capabilities.len() > 256 {
            return Err(AgentHostError::InvalidConfiguration);
        }
        let expected = configuration
            .executable_identity
            .as_ref()
            .ok_or(AgentHostError::InvalidExecutable)?;
        let executable = verify_executable(expected, configuration.ownership)
            .map_err(|_| AgentHostError::InvalidExecutable)?;
        let stdout_limit = usize::try_from(configuration.max_stdout_bytes)
            .map_err(|_| AgentHostError::InvalidConfiguration)?;
        let stderr_limit = usize::try_from(configuration.max_stderr_bytes)
            .map_err(|_| AgentHostError::InvalidConfiguration)?;
        let request = AgentBrokerRequest {
            version: AGENT_PROTOCOL_VERSION,
            kind: "broker_request",
            identity: &identity,
            input,
            capabilities,
        };
        let request = encode_frame(&request, MAX_AGENT_INPUT_BYTES)?;
        let mut command = self.command(configuration, &executable)?;
        command
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|_| AgentHostError::SpawnFailed)?;
        let mut stdin = match child.stdin.take() {
            Some(stdin) => stdin,
            None => {
                let _ = child.kill().await;
                return Err(AgentHostError::Io);
            }
        };
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let _ = child.kill().await;
                return Err(AgentHostError::Io);
            }
        };
        let stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                let _ = child.kill().await;
                return Err(AgentHostError::Io);
            }
        };
        match timeout(
            Duration::from_millis(configuration.timeout_ms),
            stdin.write_all(&request),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {
                let _ = child.kill().await;
                return Err(AgentHostError::Io);
            }
            Err(_) => {
                let _ = child.kill().await;
                return Err(AgentHostError::TimedOut);
            }
        }
        let stderr_task = tokio::spawn(read_bounded(stderr, stderr_limit));
        Ok(RunningAgent {
            identity,
            child,
            stdin: Some(stdin),
            stdout: BufReader::new(
                stdout.take(u64::try_from(stdout_limit.saturating_add(1)).unwrap_or(u64::MAX)),
            ),
            stderr_task: Some(stderr_task),
            deadline: Instant::now() + Duration::from_millis(configuration.timeout_ms),
            stdout_limit,
            stdout_read: 0,
        })
    }

    fn command(
        &self,
        configuration: &AgentConfiguration,
        executable: &Path,
    ) -> Result<Command, AgentHostError> {
        let (program, arguments) = match configuration.sandbox {
            SandboxBackend::Bubblewrap => bubblewrap_invocation(configuration, executable)?,
            SandboxBackend::Unavailable => return Err(AgentHostError::SandboxUnavailable),
            SandboxBackend::UnsafeUnsandboxed if !self.allow_unsafe_unsandboxed => {
                return Err(AgentHostError::UnsafeOptInRequired);
            }
            SandboxBackend::UnsafeUnsandboxed => {
                (executable.to_path_buf(), configuration.arguments.clone())
            }
        };
        limited_command(configuration, &program, &arguments)
    }
}

impl Default for AgentHost {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RunningAgent {
    identity: AgentExecutionIdentity,
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<Take<ChildStdout>>,
    stderr_task: Option<JoinHandle<Result<Vec<u8>, AgentHostError>>>,
    deadline: Instant,
    stdout_limit: usize,
    stdout_read: usize,
}

impl RunningAgent {
    pub fn identity(&self) -> &AgentExecutionIdentity {
        &self.identity
    }

    pub async fn receive_proposals(&mut self) -> Result<AgentInvocationResult, AgentHostError> {
        let remaining = match self.remaining() {
            Ok(remaining) => remaining,
            Err(error) => return self.fail(error).await,
        };
        let mut line = Vec::new();
        let read = match timeout(remaining, self.stdout.read_until(b'\n', &mut line)).await {
            Ok(Ok(read)) => read,
            Ok(Err(_)) => return self.fail(AgentHostError::Io).await,
            Err(_) => return self.fail(AgentHostError::TimedOut).await,
        };
        self.stdout_read = self.stdout_read.saturating_add(read);
        if read == 0 || self.stdout_read > self.stdout_limit {
            return self
                .fail(if read == 0 {
                    AgentHostError::MalformedOutput
                } else {
                    AgentHostError::OutputTooLarge
                })
                .await;
        }
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        match parse_agent_output(&line, &self.identity) {
            Ok(result) => Ok(result),
            Err(error) => self.fail(error).await,
        }
    }

    pub async fn complete(
        mut self,
        results: &[AgentBrokerProposalResult],
    ) -> Result<AgentCompletion, AgentHostError> {
        let response = AgentBrokerResponse {
            version: AGENT_PROTOCOL_VERSION,
            kind: "broker_result",
            results,
        };
        let frame = match encode_frame(&response, MAX_BROKER_RESULT_BYTES) {
            Ok(frame) => frame,
            Err(error) => return self.fail(error).await,
        };
        let remaining = match self.remaining() {
            Ok(remaining) => remaining,
            Err(error) => return self.fail(error).await,
        };
        let mut stdin = match self.stdin.take() {
            Some(stdin) => stdin,
            None => return self.fail(AgentHostError::Io).await,
        };
        let operation = async {
            stdin
                .write_all(&frame)
                .await
                .map_err(|_| AgentHostError::Io)?;
            stdin.shutdown().await.map_err(|_| AgentHostError::Io)?;
            let mut trailing = Vec::new();
            self.stdout
                .read_to_end(&mut trailing)
                .await
                .map_err(|_| AgentHostError::Io)?;
            if self.stdout_read.saturating_add(trailing.len()) > self.stdout_limit {
                return Err(AgentHostError::OutputTooLarge);
            }
            let status = self.child.wait().await.map_err(|_| AgentHostError::Io)?;
            if !status.success() {
                return Err(AgentHostError::ProcessFailed);
            }
            Ok(())
        };
        match timeout(remaining, operation).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                let _ = self.child.kill().await;
                return Err(error);
            }
            Err(_) => {
                let _ = self.child.kill().await;
                return Err(AgentHostError::TimedOut);
            }
        }
        let stderr = self.join_stderr().await?;
        Ok(AgentCompletion {
            stderr_bytes: stderr.len(),
        })
    }

    pub async fn terminate(mut self) {
        let _ = self.child.kill().await;
        let _ = self.join_stderr().await;
    }

    fn remaining(&self) -> Result<Duration, AgentHostError> {
        self.deadline
            .checked_duration_since(Instant::now())
            .ok_or(AgentHostError::TimedOut)
    }

    async fn fail<T>(&mut self, error: AgentHostError) -> Result<T, AgentHostError> {
        let _ = self.child.kill().await;
        let _ = self.join_stderr().await;
        Err(error)
    }

    async fn join_stderr(&mut self) -> Result<Vec<u8>, AgentHostError> {
        match self.stderr_task.take() {
            Some(task) => task.await.map_err(|_| AgentHostError::Io)?,
            None => Ok(Vec::new()),
        }
    }
}

fn bubblewrap_invocation(
    configuration: &AgentConfiguration,
    executable: &Path,
) -> Result<(PathBuf, Vec<String>), AgentHostError> {
    let bubblewrap =
        trusted_system_tool(&["/usr/bin/bwrap"]).ok_or(AgentHostError::SandboxUnavailable)?;
    let mut arguments = [
        "--unshare-all",
        "--new-session",
        "--die-with-parent",
        "--clearenv",
        "--tmpfs",
        "/tmp",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--chdir",
        "/tmp",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    for system_path in ["/usr", "/bin", "/lib", "/lib64"] {
        if Path::new(system_path).exists() {
            arguments.extend([
                "--ro-bind".to_owned(),
                system_path.to_owned(),
                system_path.to_owned(),
            ]);
        }
    }
    arguments.push("--".to_owned());
    arguments.push(executable.to_string_lossy().into_owned());
    arguments.extend(configuration.arguments.iter().cloned());
    Ok((bubblewrap, arguments))
}

fn limited_command(
    configuration: &AgentConfiguration,
    program: &Path,
    arguments: &[String],
) -> Result<Command, AgentHostError> {
    let prlimit = trusted_system_tool(&["/usr/bin/prlimit"])
        .ok_or(AgentHostError::ResourceLimitUnavailable)?;
    let limits = &configuration.resource_limits;
    let mut command = Command::new(prlimit);
    command.args([
        format!("--cpu={}", limits.cpu_seconds),
        format!("--as={}", limits.memory_bytes),
        format!("--nproc={}", limits.process_count),
        format!("--fsize={}", limits.file_size_bytes),
        format!("--nofile={}", limits.open_files),
        "--".to_owned(),
        program.to_string_lossy().into_owned(),
    ]);
    command.args(arguments);
    Ok(command)
}

fn trusted_system_tool(candidates: &[&str]) -> Option<PathBuf> {
    candidates.iter().find_map(|candidate| {
        let identity = inspect_executable(candidate, ExecutableOwnership::RootOnly, None).ok()?;
        verify_executable(&identity, ExecutableOwnership::RootOnly).ok()
    })
}

async fn read_bounded(
    reader: impl AsyncRead + Unpin,
    limit: usize,
) -> Result<Vec<u8>, AgentHostError> {
    let max = u64::try_from(limit.saturating_add(1)).unwrap_or(u64::MAX);
    let mut reader = reader.take(max);
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    reader
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| AgentHostError::Io)?;
    if bytes.len() > limit {
        return Err(AgentHostError::OutputTooLarge);
    }
    Ok(bytes)
}

fn encode_frame(value: &impl Serialize, max_bytes: usize) -> Result<Vec<u8>, AgentHostError> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| AgentHostError::Io)?;
    if bytes.len().saturating_add(1) > max_bytes {
        return Err(AgentHostError::InvalidConfiguration);
    }
    bytes.push(b'\n');
    Ok(bytes)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentWireOutput {
    version: u16,
    kind: String,
    message: String,
    #[serde(default)]
    proposals: Vec<AgentWireProposal>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentWireProposal {
    action: ActionRequest,
    explanation: String,
}

fn parse_agent_output(
    bytes: &[u8],
    identity: &AgentExecutionIdentity,
) -> Result<AgentInvocationResult, AgentHostError> {
    let output: AgentWireOutput =
        serde_json::from_slice(bytes).map_err(|_| AgentHostError::MalformedOutput)?;
    if output.version != AGENT_PROTOCOL_VERSION
        || output.kind != "proposals"
        || output.message.len() > MAX_AGENT_MESSAGE_BYTES
        || output.proposals.len() > MAX_AGENT_PROPOSALS
        || output
            .proposals
            .iter()
            .any(|proposal| proposal.explanation.len() > 2_048)
    {
        return Err(AgentHostError::MalformedOutput);
    }
    let proposals = output
        .proposals
        .into_iter()
        .map(|proposal| {
            ActionProposal::new(proposal.action, ActionContext::agent(identity.clone()))
                .map_err(|_| AgentHostError::MalformedOutput)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AgentInvocationResult {
        message: output.message,
        proposals,
    })
}

#[cfg(test)]
mod tests {
    use halquen_capabilities::inspect_executable;
    use halquen_domain::{
        AgentId, AgentInstanceId, AgentResourceLimits, AgentSessionId, ExecutableOwnership,
    };

    use super::*;

    fn python_agent(source: &str, max_stdout_bytes: u32) -> Option<AgentConfiguration> {
        let executable = Path::new("/usr/bin/python3").canonicalize().ok()?;
        let executable = executable.to_str()?.to_owned();
        let identity = inspect_executable(&executable, ExecutableOwnership::RootOnly, None).ok()?;
        Some(AgentConfiguration {
            id: AgentId::generate(),
            name: "fixture agent".to_owned(),
            transport: AgentTransport::Cli,
            executable,
            arguments: vec!["-c".to_owned(), source.to_owned()],
            socket_path: None,
            sandbox: SandboxBackend::UnsafeUnsandboxed,
            ownership: ExecutableOwnership::RootOnly,
            executable_identity: Some(identity),
            resource_limits: AgentResourceLimits::default(),
            enabled: true,
            timeout_ms: 2_000,
            max_stdout_bytes,
            max_stderr_bytes: 1_024,
            created_at_ms: 1,
            updated_at_ms: 1,
        })
    }

    fn execution_identity(agent_id: AgentId) -> AgentExecutionIdentity {
        AgentExecutionIdentity {
            agent_id,
            instance_id: AgentInstanceId::generate(),
            session_id: AgentSessionId::generate(),
        }
    }

    #[tokio::test]
    async fn broker_round_trip_keeps_agent_proposals_untrusted() {
        let source = r#"import sys,json
json.loads(sys.stdin.readline())
print(json.dumps({"version":1,"kind":"proposals","message":"proposal","proposals":[{"action":{"capability_id":"system.open_app","arguments":{"kind":"open_app","app":"app:telegram"}},"explanation":"requested"}]}), flush=True)
result=json.loads(sys.stdin.readline())
assert result["kind"] == "broker_result"
"#;
        let Some(configuration) = python_agent(source, 4_096) else {
            return;
        };
        let identity = execution_identity(configuration.id.clone());
        let mut running = AgentHost::with_unsafe_unsandboxed_opt_in()
            .start(&configuration, identity.clone(), "request", &[])
            .await
            .unwrap();
        let result = running.receive_proposals().await.unwrap();
        assert_eq!(result.proposals.len(), 1);
        assert_eq!(result.proposals[0].context.agent, Some(identity));
        assert_eq!(
            result.proposals[0].context.authority,
            halquen_domain::AuthorityClass::None
        );
        running.complete(&[]).await.unwrap();
    }

    #[tokio::test]
    async fn oversized_subprocess_output_is_blocked() {
        let source = r#"import sys
sys.stdin.readline()
print('x' * 2048, flush=True)
"#;
        let Some(configuration) = python_agent(source, 1_024) else {
            return;
        };
        let identity = execution_identity(configuration.id.clone());
        let mut running = AgentHost::with_unsafe_unsandboxed_opt_in()
            .start(&configuration, identity, "request", &[])
            .await
            .unwrap();
        assert!(matches!(
            running.receive_proposals().await,
            Err(AgentHostError::OutputTooLarge)
        ));
    }

    #[tokio::test]
    async fn trailing_output_is_counted_against_the_total_stdout_limit() {
        let source = r#"import sys,json
json.loads(sys.stdin.readline())
print(json.dumps({"version":1,"kind":"proposals","message":"","proposals":[]}), flush=True)
json.loads(sys.stdin.readline())
sys.stdout.write('x' * 980)
sys.stdout.flush()
"#;
        let Some(configuration) = python_agent(source, 1_024) else {
            return;
        };
        let identity = execution_identity(configuration.id.clone());
        let mut running = AgentHost::with_unsafe_unsandboxed_opt_in()
            .start(&configuration, identity, "request", &[])
            .await
            .unwrap();
        running.receive_proposals().await.unwrap();
        assert!(matches!(
            running.complete(&[]).await,
            Err(AgentHostError::OutputTooLarge)
        ));
    }

    #[tokio::test]
    async fn timed_out_agent_is_explicitly_killed_and_reaped() {
        let source = r#"import sys,time
sys.stdin.readline()
time.sleep(5)
"#;
        let Some(mut configuration) = python_agent(source, 1_024) else {
            return;
        };
        configuration.timeout_ms = 100;
        let identity = execution_identity(configuration.id.clone());
        let mut running = AgentHost::with_unsafe_unsandboxed_opt_in()
            .start(&configuration, identity, "request", &[])
            .await
            .unwrap();
        assert!(matches!(
            running.receive_proposals().await,
            Err(AgentHostError::TimedOut)
        ));
        assert!(running.child.try_wait().unwrap().is_some());
    }

    #[tokio::test]
    async fn unsafe_subprocess_fails_without_explicit_opt_in() {
        let Some(configuration) = python_agent("", 1_024) else {
            return;
        };
        let identity = execution_identity(configuration.id.clone());
        assert!(matches!(
            AgentHost::new()
                .start(&configuration, identity, "request", &[])
                .await,
            Err(AgentHostError::UnsafeOptInRequired)
        ));
    }

    #[test]
    fn agent_wire_format_cannot_inject_context_or_authority() {
        let identity = execution_identity(AgentId::generate());
        let payload = br#"{"version":1,"kind":"proposals","message":"x","proposals":[{"action":{"capability_id":"system.open_app","arguments":{"kind":"open_app","app":"app:telegram"}},"explanation":"x","authority":"user"}]}"#;
        assert!(matches!(
            parse_agent_output(payload, &identity),
            Err(AgentHostError::MalformedOutput)
        ));
    }

    #[test]
    fn bubblewrap_invocation_does_not_expose_home_or_daemon_socket() {
        let Some(mut configuration) = python_agent("", 1_024) else {
            return;
        };
        configuration.sandbox = SandboxBackend::Bubblewrap;
        let Ok((_program, arguments)) =
            bubblewrap_invocation(&configuration, Path::new(&configuration.executable))
        else {
            return;
        };
        let joined = arguments.join(" ");
        assert!(joined.contains("--unshare-all"));
        assert!(joined.contains("--clearenv"));
        assert!(!joined.contains("HOME"));
        assert!(!joined.contains("halquen.sock"));
    }

    #[test]
    fn resource_limit_wrapper_contains_all_supported_rlimits() {
        let Some(configuration) = python_agent("", 1_024) else {
            return;
        };
        let Ok(command) = limited_command(
            &configuration,
            Path::new(&configuration.executable),
            &configuration.arguments,
        ) else {
            return;
        };
        let arguments = command
            .as_std()
            .get_args()
            .map(|argument| argument.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        for limit in ["--cpu=", "--as=", "--nproc=", "--fsize=", "--nofile="] {
            assert!(arguments.contains(limit), "missing {limit} in {arguments}");
        }
    }
}
