use serde::{Deserialize, Serialize};

use super::memory::deserialize_required_nullable;

/// A category is `None` when collection was unavailable and `Some([])` when it succeeded with no entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct CategoryCollection<T> {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub items: Option<Vec<T>>,
    #[serde(default)]
    pub truncated: bool,
}

impl<T> Default for CategoryCollection<T> {
    fn default() -> Self {
        Self {
            items: None,
            truncated: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeEnvironmentCollection {
    pub services: CategoryCollection<Service>,
    pub startup_applications: CategoryCollection<StartupApplication>,
    pub installed_applications: CategoryCollection<InstalledApplication>,
    pub running_processes: CategoryCollection<RunningProcess>,
    pub scheduled_tasks: CategoryCollection<ScheduledTask>,
}

impl RuntimeEnvironmentCollection {
    pub fn is_unavailable(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BinaryIdentity {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub executable_path: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub publisher: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub signature_status: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub sha256: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub file_exists: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Service {
    pub name: String,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub display_name: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub description: Option<String>,
    pub state: String,
    pub start_mode: String,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub command_line: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub account: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub process_id: Option<u32>,
    pub dependencies: Vec<String>,
    pub binary: BinaryIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupApplication {
    pub name: String,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub enabled: Option<bool>,
    pub source: String,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub command_line: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub target_user: Option<String>,
    pub binary: BinaryIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstalledApplicationKind {
    Desktop,
    Store,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledApplication {
    pub name: String,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub version: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub publisher: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub installed_on: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub install_location: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub architecture: Option<String>,
    pub kind: InstalledApplicationKind,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub uninstall_command: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessAccess {
    Complete,
    Partial,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningProcess {
    pub name: String,
    pub process_id: u32,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub parent_process_id: Option<u32>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub command_line: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub started_at: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub user: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub cpu_time_ms: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub working_set_bytes: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub io_bytes: Option<u64>,
    pub access: ProcessAccess,
    pub binary: BinaryIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledTaskAction {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub executable: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub arguments: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub working_directory: Option<String>,
    pub binary: BinaryIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub name: String,
    pub folder: String,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub description: Option<String>,
    pub enabled: bool,
    pub state: String,
    pub triggers: Vec<String>,
    pub actions: Vec<ScheduledTaskAction>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub account: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub run_level: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub last_run_at: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub next_run_at: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub last_result: Option<i64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub repetition_interval: Option<String>,
    pub hidden: bool,
}
