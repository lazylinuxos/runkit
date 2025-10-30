use runkit_core::{DesiredState, ServiceInfo, ServiceRuntimeState};
use serde::Deserialize;
use serde_json::Value;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

#[derive(Clone)]
pub struct ActionDispatcher {
    helper_path: PathBuf,
    use_pkexec: bool,
}

impl Default for ActionDispatcher {
    fn default() -> Self {
        let helper_path = env::var("RUNKITD_PATH")
            .or_else(|_| env::var("RUNKIT_HELPER_PATH"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/usr/libexec/runkitd"));
        let use_pkexec = env::var("RUNKITD_NO_PKEXEC")
            .or_else(|_| env::var("RUNKIT_HELPER_NO_PKEXEC"))
            .map(|value| value == "0" || value.eq_ignore_ascii_case("false"))
            .unwrap_or(true);

        ActionDispatcher {
            helper_path,
            use_pkexec,
        }
    }
}

impl ActionDispatcher {
    pub fn run(&self, action: &str, service: &str) -> Result<String, String> {
        let helper_path = self.helper_path.clone();
        let response = execute_helper(helper_path, self.use_pkexec, action, Some(service), &[])?;
        match response.status.as_str() {
            "ok" => Ok(response
                .message
                .unwrap_or_else(|| format!("{action} command completed for {service}"))),
            _ => Err(response
                .message
                .unwrap_or_else(|| format!("runkitd reported failure for {service}"))),
        }
    }

    pub fn fetch_services(&self) -> Result<Vec<ServiceInfo>, String> {
        let response =
            execute_helper(self.helper_path.clone(), self.use_pkexec, "list", None, &[])?;
        if response.status.as_str() != "ok" {
            return Err(
                response
                    .message
                    .unwrap_or_else(|| "runkitd failed to enumerate services".to_string()),
            );
        }

        let data = response
            .data
            .ok_or_else(|| "runkitd returned no service data".to_string())?;

        let snapshots: Vec<ServiceSnapshot> = serde_json::from_value(data)
            .map_err(|err| format!("Failed to decode runkitd response: {err}"))?;

        Ok(snapshots.into_iter().map(ServiceInfo::from).collect())
    }

    pub fn fetch_logs(&self, service: &str, lines: usize) -> Result<Vec<LogEntry>, String> {
        let limit_arg = lines.max(1).to_string();
        let extra_args = ["--lines", limit_arg.as_str()];
        let response = execute_helper(
            self.helper_path.clone(),
            self.use_pkexec,
            "logs",
            Some(service),
            &extra_args,
        )?;

        if response.status.as_str() != "ok" {
            return Err(response
                .message
                .unwrap_or_else(|| format!("runkitd failed to stream logs for {service}")));
        }

        let data = response
            .data
            .ok_or_else(|| "runkitd returned no log data".to_string())?;

        let entries: Vec<LogEntrySnapshot> = serde_json::from_value(data)
            .map_err(|err| format!("Failed to decode runkitd logs response: {err}"))?;

        Ok(entries.into_iter().map(LogEntry::from).collect())
    }
}

fn execute_helper(
    helper_path: PathBuf,
    use_pkexec: bool,
    action: &str,
    service: Option<&str>,
    extra: &[&str],
) -> Result<DaemonProcessResponse, String> {
    let mut command = if use_pkexec {
        let mut cmd = Command::new("pkexec");
        cmd.arg(&helper_path);
        cmd
    } else {
        Command::new(&helper_path)
    };
    command.arg(action);
    if let Some(service) = service {
        command.arg(service);
    }
    for arg in extra {
        command.arg(arg);
    }

    let action_label = match service {
        Some(service) => format!("{action} {service}"),
        None => action.to_string(),
    };

    match command.output() {
        Ok(output) => {
            let stdout = output.stdout;
            let trimmed = String::from_utf8_lossy(&stdout).trim().to_string();
            if trimmed.is_empty() {
                if output.status.success() {
                    return Err(format!(
                        "runkitd returned an empty response for {action_label}"
                    ));
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(if stderr.trim().is_empty() {
                        format!(
                            "runkitd exited with status {} for {action_label}",
                            output.status.code().unwrap_or(-1)
                        )
                    } else {
                        stderr.trim().to_string()
                    });
                }
            }
            match parse_response(&trimmed) {
                Ok(response) => Ok(response),
                Err(parse_err) => {
                    if output.status.success() {
                        Err(format!(
                            "Failed to parse runkitd response for {action_label}: {parse_err} (raw: {trimmed})"
                        ))
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        Err(if stderr.trim().is_empty() {
                            format!(
                                "runkitd exited with status {} for {action_label}",
                                output.status.code().unwrap_or(-1)
                            )
                        } else {
                            stderr.trim().to_string()
                        })
                    }
                }
            }
        }
        Err(err) => Err(format!("Failed to invoke runkitd: {err}")),
    }
}

#[derive(Debug, Deserialize)]
struct DaemonProcessResponse {
    status: String,
    message: Option<String>,
    data: Option<Value>,
}

fn parse_response(data: &str) -> Result<DaemonProcessResponse, serde_json::Error> {
    if data.is_empty() {
        Err(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "empty response",
        )))
    } else {
        serde_json::from_str(data)
    }
}

#[derive(Debug, Deserialize)]
struct ServiceSnapshot {
    name: String,
    definition_path: String,
    enabled: bool,
    desired_state: SnapshotDesiredState,
    runtime_state: SnapshotRuntimeState,
    description: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub unix_seconds: Option<i64>,
    pub nanos: Option<u32>,
    pub raw: Option<String>,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct LogEntrySnapshot {
    unix_seconds: Option<i64>,
    nanos: Option<u32>,
    raw: Option<String>,
    message: String,
}

impl From<LogEntrySnapshot> for LogEntry {
    fn from(snapshot: LogEntrySnapshot) -> Self {
        LogEntry {
            unix_seconds: snapshot.unix_seconds,
            nanos: snapshot.nanos,
            raw: snapshot.raw,
            message: snapshot.message,
        }
    }
}

impl From<ServiceSnapshot> for ServiceInfo {
    fn from(snapshot: ServiceSnapshot) -> Self {
        ServiceInfo {
            name: snapshot.name,
            definition_path: PathBuf::from(snapshot.definition_path),
            enabled: snapshot.enabled,
            desired_state: snapshot.desired_state.into(),
            runtime_state: snapshot.runtime_state.into(),
            description: snapshot.description,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotDesiredState {
    AutoStart,
    Manual,
}

impl From<SnapshotDesiredState> for DesiredState {
    fn from(value: SnapshotDesiredState) -> Self {
        match value {
            SnapshotDesiredState::AutoStart => DesiredState::AutoStart,
            SnapshotDesiredState::Manual => DesiredState::Manual,
        }
    }
}

#[derive(Debug, Deserialize)]
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

impl From<SnapshotRuntimeState> for ServiceRuntimeState {
    fn from(value: SnapshotRuntimeState) -> Self {
        match value {
            SnapshotRuntimeState::Running {
                pid,
                uptime_seconds,
            } => ServiceRuntimeState::Running {
                pid,
                uptime: Duration::from_secs(uptime_seconds),
            },
            SnapshotRuntimeState::Down {
                since_seconds,
                normally_up,
            } => ServiceRuntimeState::Down {
                since: Duration::from_secs(since_seconds),
                normally_up,
            },
            SnapshotRuntimeState::Failed {
                pid,
                uptime_seconds,
                exit_code,
            } => ServiceRuntimeState::Failed {
                pid,
                uptime: Duration::from_secs(uptime_seconds),
                exit_code,
            },
            SnapshotRuntimeState::Unknown { raw } => ServiceRuntimeState::Unknown { raw },
        }
    }
}
