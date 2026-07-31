use serde::{Deserialize, Serialize};

use super::memory::deserialize_required_nullable;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsUpdateCollection {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub lookback_days: Option<u32>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub max_entries: Option<u32>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub history: Option<Vec<WindowsUpdateHistoryEntry>>,
}

impl Default for WindowsUpdateCollection {
    fn default() -> Self {
        Self {
            lookback_days: Some(180),
            max_entries: Some(1_000),
            history: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsUpdateHistoryEntry {
    pub occurred_at: String,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub title: Option<String>,
    pub kb_ids: Vec<String>,
    pub operation: WindowsUpdateOperation,
    pub operation_code: i32,
    pub result: WindowsUpdateResult,
    pub result_code: i32,
    pub hresult: i64,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub update_id: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub revision_number: Option<i32>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub support_url: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub client_application_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowsUpdateOperation {
    Installation,
    Uninstallation,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowsUpdateResult {
    NotStarted,
    InProgress,
    Succeeded,
    SucceededWithErrors,
    Failed,
    Aborted,
    Unknown,
}
