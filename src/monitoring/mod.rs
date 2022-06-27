pub mod alerts;

pub mod monitor;

pub mod query;

mod qtype;
pub use qtype::*;

pub mod analysis;

/// Enum representing the current Status of the Incidents
pub enum IncidentStatus {
    Active,
    Resolved,
}

impl std::fmt::Display for IncidentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            IncidentStatus::Active => {
                write!(f, "Active")
            }
            IncidentStatus::Resolved => {
                write!(f, "Resolved")
            }
        }
    }
}

impl From<i32> for IncidentStatus {
    fn from(v: i32) -> Self {
        match v {
            0 => IncidentStatus::Active,
            _ => IncidentStatus::Resolved,
        }
    }
}

/// Enum representing the Severity of the Incidents
#[derive(Clone)]
pub enum Severity {
    Warning,
    Critical,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Severity::Warning => {
                write!(f, "Warning")
            }
            Severity::Critical => {
                write!(f, "Critical")
            }
        }
    }
}

impl From<i32> for Severity {
    fn from(v: i32) -> Self {
        match v {
            0 => Severity::Warning,
            _ => Severity::Critical,
        }
    }
}
