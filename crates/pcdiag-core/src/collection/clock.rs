use serde::{Deserialize, Serialize};

use super::memory::deserialize_required_nullable;

/// Clock and time-synchronization facts reported by the diagnostic target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockCollection {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub system_time_utc: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub utc_offset_minutes: Option<i16>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub windows_time_service: Option<WindowsServiceState>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub hardware_clock: Option<HardwareClock>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareClock {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub time_utc: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowsServiceState {
    Stopped,
    StartPending,
    StopPending,
    Running,
    ContinuePending,
    PausePending,
    Paused,
    Unknown,
}
