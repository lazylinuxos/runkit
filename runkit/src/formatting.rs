use humantime::format_duration;
use runkit_core::{DesiredState, ServiceInfo, ServiceRuntimeState};

pub fn runtime_state_short(service: &ServiceInfo) -> String {
    if matches!(&service.runtime_state, ServiceRuntimeState::Running { .. }) {
        return "Running".to_string();
    }

    if !service.enabled {
        return "Stopped".to_string();
    }

    match &service.runtime_state {
        ServiceRuntimeState::Down { normally_up, .. } => {
            if *normally_up {
                "Stopped".to_string()
            } else {
                "Idle".to_string()
            }
        }
        ServiceRuntimeState::Failed { .. } => "Error".to_string(),
        ServiceRuntimeState::Unknown { .. } => "Unavailable".to_string(),
        ServiceRuntimeState::Running { .. } => unreachable!(),
    }
}

pub fn runtime_state_detail(service: &ServiceInfo) -> String {
    match &service.runtime_state {
        ServiceRuntimeState::Running { pid, uptime } => format!(
            "Running (PID {pid}) for {}",
            format_duration(*uptime).to_string()
        ),
        ServiceRuntimeState::Down { since, normally_up } => {
            let downtime = format_duration(*since).to_string();
            if !service.enabled {
                format!("Stopped (disabled); last ran {downtime} ago")
            } else if *normally_up {
                format!("Stopped {downtime} ago; runit will restart automatically")
            } else {
                format!("Stopped {downtime} ago; waiting for manual start")
            }
        }
        ServiceRuntimeState::Failed {
            exit_code, uptime, ..
        } => {
            let runtime = format_duration(*uptime);
            if service.enabled {
                format!("Stopped due to error; exited with code {exit_code} after {runtime}")
            } else {
                format!(
                    "Stopped (disabled); last start attempt exited with code {exit_code} after {}",
                    runtime
                )
            }
        }
        ServiceRuntimeState::Unknown { .. } => {
            if service.enabled {
                "Status unavailable; runit did not report details".to_string()
            } else {
                "Stopped (disabled); service directory is not linked to /var/service".to_string()
            }
        }
    }
}

pub fn list_row_subtitle(service: &ServiceInfo) -> String {
    match service.description.as_ref() {
        Some(description) if !description.is_empty() => {
            format!("{} — {}", runtime_state_short(service), description)
        }
        _ => runtime_state_short(service),
    }
}

pub fn detail_description_text(service: &ServiceInfo) -> String {
    service
        .description
        .clone()
        .unwrap_or_else(|| "This service has no description yet.".to_string())
}

pub fn is_running(state: &ServiceRuntimeState) -> bool {
    matches!(state, ServiceRuntimeState::Running { .. })
}

pub fn is_auto_start(desired: DesiredState) -> bool {
    matches!(desired, DesiredState::AutoStart)
}

pub fn status_level(service: &ServiceInfo) -> StatusLevel {
    if matches!(&service.runtime_state, ServiceRuntimeState::Running { .. }) {
        return StatusLevel::Good;
    }

    if !service.enabled {
        return StatusLevel::Neutral;
    }

    match &service.runtime_state {
        ServiceRuntimeState::Down { normally_up, .. } => {
            if *normally_up {
                StatusLevel::Warning
            } else {
                StatusLevel::Neutral
            }
        }
        ServiceRuntimeState::Failed { .. } => StatusLevel::Critical,
        ServiceRuntimeState::Unknown { .. } => StatusLevel::Warning,
        ServiceRuntimeState::Running { .. } => unreachable!(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusLevel {
    Good,
    Warning,
    Critical,
    Neutral,
}
