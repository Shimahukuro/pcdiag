use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventLogCollection {
    pub lookback_days: u32,
    pub system: Option<Vec<EventLogEntry>>,
    pub application: Option<Vec<EventLogEntry>>,
    pub security: Option<Vec<EventLogEntry>>,
}

impl Default for EventLogCollection {
    fn default() -> Self {
        Self {
            lookback_days: 30,
            system: Some(Vec::new()),
            application: Some(Vec::new()),
            security: Some(Vec::new()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventLogEntry {
    pub occurred_at: String,
    pub log_name: String,
    pub provider: String,
    pub event_id: u32,
    pub level: EventLogLevel,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventLogLevel {
    Critical,
    Error,
    Warning,
    Information,
}
