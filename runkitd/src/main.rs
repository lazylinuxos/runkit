use clap::{Parser, Subcommand};
use runkit_core::{
    DesiredState, ServiceInfo, ServiceLogEntry, ServiceManager, ServiceRuntimeState,
};
use serde::Serialize;
use serde_json::Value;
use std::os::unix::fs as unix_fs;
use std::path::PathBuf;
use std::process::Command;
use thiserror::Error;

#[derive(Parser, Debug)]
#[command(author, version, about = "Privileged daemon for the Runkit GUI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: HelperCommand,
}

#[derive(Subcommand, Debug)]
enum HelperCommand {
    /// Start a service and ensure it keeps running.
    Start { service: String },
    /// Stop a service and keep it down.
    Stop { service: String },
    /// Restart a service.
    Restart { service: String },
    /// Reload a service's configuration.
    Reload { service: String },
    /// Run the service's check script.
    Check { service: String },
    /// Run a service once and exit.
    Once { service: String },
    /// Enable a service (auto-start on boot).
    Enable { service: String },
    /// Disable a service (stop auto-start).
    Disable { service: String },
    /// List all available services with their current status.
    List,
    /// Tail logs for a service.
    Logs {
        service: String,
        #[arg(long, default_value_t = 200)]
        lines: usize,
    },
}

#[derive(Debug, Serialize)]
struct HelperResponse {
    status: ResponseStatus,
    message: Option<String>,
    data: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum ResponseStatus {
    Ok,
    Error,
}

impl HelperResponse {
    fn ok_with(outcome: CommandOutcome) -> Self {
        Self {
            status: ResponseStatus::Ok,
            message: outcome.message,
            data: outcome.data,
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            status: ResponseStatus::Error,
            message: Some(message.into()),
            data: None,
        }
    }
}

#[derive(Debug)]
struct CommandOutcome {
    message: Option<String>,
    data: Option<Value>,
}

impl CommandOutcome {
    fn message(message: impl Into<String>) -> Self {
        CommandOutcome {
            message: Some(message.into()),
            data: None,
        }
    }

    fn with(message: Option<String>, data: Option<Value>) -> Self {
        CommandOutcome { message, data }
    }
}

#[derive(Debug, Error)]
enum HelperError {
    #[error("invalid service name: {0}")]
    InvalidService(String),
    #[error("service definition missing: {service} ({path})")]
    DefinitionMissing { service: String, path: PathBuf },
    #[error("service already enabled: {0}")]
    AlreadyEnabled(String),
    #[error("service is not enabled: {0}")]
    NotEnabled(String),
    #[error("command `{command}` failed for {service}: {message}")]
    SvFailure {
        command: String,
        service: String,
        message: String,
    },
    #[error("I/O error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Other(String),
}

impl HelperError {
    fn exit_code(&self) -> i32 {
        match self {
            HelperError::InvalidService(_) => 2,
            HelperError::DefinitionMissing { .. } => 3,
            HelperError::AlreadyEnabled(_) => 4,
            HelperError::NotEnabled(_) => 5,
            HelperError::SvFailure { .. } => 6,
            HelperError::Io { .. } => 7,
            HelperError::Other(_) => 1,
        }
    }
}

impl From<runkit_core::ServiceError> for HelperError {
    fn from(value: runkit_core::ServiceError) -> Self {
        match value {
            runkit_core::ServiceError::InvalidServiceName(name) => {
                HelperError::InvalidService(name)
            }
            runkit_core::ServiceError::Io { path, source } => HelperError::Io { path, source },
            runkit_core::ServiceError::SvCommand { service, message } => HelperError::SvFailure {
                command: "status".to_string(),
                service,
                message,
            },
            runkit_core::ServiceError::LogUnavailable(service) => {
                HelperError::Other(format!("log stream unavailable for {service}"))
            }
            runkit_core::ServiceError::Other(err) => HelperError::Other(err.to_string()),
        }
    }
}

fn main() {
    let response = execute();
    match response {
        Ok(outcome) => {
            emit_and_exit(HelperResponse::ok_with(outcome), 0);
        }
        Err(err) => {
            emit_and_exit(HelperResponse::error(err.to_string()), err.exit_code());
        }
    }
}

fn emit_and_exit(response: HelperResponse, exit_code: i32) -> ! {
    let output = serde_json::to_string(&response).unwrap_or_else(|_| {
        "{\"status\":\"error\",\"message\":\"failed to serialize runkitd response\"}".to_string()
    });
    println!("{}", output);
    std::process::exit(exit_code);
}

fn execute() -> Result<CommandOutcome, HelperError> {
    let cli = Cli::parse();
    let context = HelperContext::default();
    context.run(cli.command)
}

#[derive(Debug)]
struct HelperContext {
    manager: ServiceManager,
}

impl Default for HelperContext {
    fn default() -> Self {
        HelperContext {
            manager: ServiceManager::default(),
        }
    }
}

impl HelperContext {
    fn run(&self, command: HelperCommand) -> Result<CommandOutcome, HelperError> {
        match command {
            HelperCommand::Start { service } => self.call_sv("up", &service),
            HelperCommand::Stop { service } => self.call_sv("down", &service),
            HelperCommand::Restart { service } => self.call_sv("restart", &service),
            HelperCommand::Reload { service } => self.call_sv("reload", &service),
            HelperCommand::Check { service } => self.call_sv("check", &service),
            HelperCommand::Once { service } => self.call_sv("once", &service),
            HelperCommand::Enable { service } => self.enable(&service),
            HelperCommand::Disable { service } => self.disable(&service),
            HelperCommand::List => self.list(),
            HelperCommand::Logs { service, lines } => self.logs(&service, lines),
        }
    }

    fn call_sv(&self, subcommand: &str, service: &str) -> Result<CommandOutcome, HelperError> {
        self.manager.validate_service_name(service)?;
        let mut command = Command::new(self.manager.sv_command_path());
        command.arg(subcommand).arg(service);

        let output = command.output().map_err(|err| HelperError::Io {
            path: self.manager.sv_command_path().to_path_buf(),
            source: err,
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(HelperError::SvFailure {
                command: subcommand.to_string(),
                service: service.to_string(),
                message: if stderr.is_empty() {
                    format!("exit status {}", output.status)
                } else {
                    stderr
                },
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(CommandOutcome::message(if stdout.is_empty() {
            format!("{subcommand} command executed for {service}")
        } else {
            stdout
        }))
    }

    fn enable(&self, service: &str) -> Result<CommandOutcome, HelperError> {
        self.manager.validate_service_name(service)?;
        let src = self.manager.definitions_dir().join(service);
        if !src.exists() {
            return Err(HelperError::DefinitionMissing {
                service: service.to_string(),
                path: src,
            });
        }

        let dest = self.manager.enabled_dir().join(service);
        if dest.exists() {
            return Err(HelperError::AlreadyEnabled(service.to_string()));
        }

        unix_fs::symlink(&src, &dest).map_err(|err| HelperError::Io {
            path: dest.clone(),
            source: err,
        })?;

        Ok(CommandOutcome::message(format!(
            "Enabled service {service}"
        )))
    }

    fn disable(&self, service: &str) -> Result<CommandOutcome, HelperError> {
        self.manager.validate_service_name(service)?;
        let dest = self.manager.enabled_dir().join(service);
        if !dest.exists() {
            return Err(HelperError::NotEnabled(service.to_string()));
        }

        std::fs::remove_file(&dest).map_err(|err| HelperError::Io {
            path: dest.clone(),
            source: err,
        })?;

        Ok(CommandOutcome::message(format!(
            "Disabled service {service}"
        )))
    }

    fn list(&self) -> Result<CommandOutcome, HelperError> {
        let services = self.manager.list_services()?;
        let snapshots: Vec<ServiceSnapshot> = services.iter().map(ServiceSnapshot::from).collect();
        let data =
            serde_json::to_value(snapshots).map_err(|err| HelperError::Other(err.to_string()))?;
        Ok(CommandOutcome::with(None, Some(data)))
    }

    fn logs(&self, service: &str, lines: usize) -> Result<CommandOutcome, HelperError> {
        let entries = self.manager.tail_logs(service, lines)?;
        let snapshots: Vec<LogEntrySnapshot> =
            entries.into_iter().map(LogEntrySnapshot::from).collect();
        let data =
            serde_json::to_value(snapshots).map_err(|err| HelperError::Other(err.to_string()))?;
        Ok(CommandOutcome::with(None, Some(data)))
    }
}

#[derive(Debug, Serialize)]
struct ServiceSnapshot {
    name: String,
    definition_path: String,
    enabled: bool,
    desired_state: SnapshotDesiredState,
    runtime_state: SnapshotRuntimeState,
    description: Option<String>,
}

impl From<&ServiceInfo> for ServiceSnapshot {
    fn from(info: &ServiceInfo) -> Self {
        ServiceSnapshot {
            name: info.name.clone(),
            definition_path: info.definition_path.to_string_lossy().to_string(),
            enabled: info.enabled,
            desired_state: SnapshotDesiredState::from(info.desired_state),
            runtime_state: SnapshotRuntimeState::from(&info.runtime_state),
            description: info.description.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotDesiredState {
    AutoStart,
    Manual,
}

impl From<DesiredState> for SnapshotDesiredState {
    fn from(value: DesiredState) -> Self {
        match value {
            DesiredState::AutoStart => SnapshotDesiredState::AutoStart,
            DesiredState::Manual => SnapshotDesiredState::Manual,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum SnapshotRuntimeState {
    Running {
        pid: u32,
        uptime_seconds: u64,
    },
    Down {
        since_seconds: u64,
        normally_up: bool,
    },
    Failed {
        pid: u32,
        uptime_seconds: u64,
        exit_code: i32,
    },
    Unknown {
        raw: String,
    },
}

impl From<&ServiceRuntimeState> for SnapshotRuntimeState {
    fn from(value: &ServiceRuntimeState) -> Self {
        match value {
            ServiceRuntimeState::Running { pid, uptime } => SnapshotRuntimeState::Running {
                pid: *pid,
                uptime_seconds: uptime.as_secs(),
            },
            ServiceRuntimeState::Down { since, normally_up } => SnapshotRuntimeState::Down {
                since_seconds: since.as_secs(),
                normally_up: *normally_up,
            },
            ServiceRuntimeState::Failed {
                pid,
                uptime,
                exit_code,
            } => SnapshotRuntimeState::Failed {
                pid: *pid,
                uptime_seconds: uptime.as_secs(),
                exit_code: *exit_code,
            },
            ServiceRuntimeState::Unknown { raw } => {
                SnapshotRuntimeState::Unknown { raw: raw.clone() }
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct LogEntrySnapshot {
    unix_seconds: Option<i64>,
    nanos: Option<u32>,
    raw: Option<String>,
    message: String,
}

impl From<ServiceLogEntry> for LogEntrySnapshot {
    fn from(entry: ServiceLogEntry) -> Self {
        LogEntrySnapshot {
            unix_seconds: entry.timestamp_unix,
            nanos: entry.timestamp_nanos,
            raw: entry.timestamp_raw,
            message: entry.message,
        }
    }
}
