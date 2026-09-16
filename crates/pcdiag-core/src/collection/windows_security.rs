use serde::{Deserialize, Serialize};

use super::memory::deserialize_required_nullable;

/// Category health reported by Windows Security Center, not product-specific settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsSecurityCollection {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub firewall: Option<SecurityHealth>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub automatic_updates: Option<SecurityHealth>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub antivirus: Option<SecurityHealth>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub internet_settings: Option<SecurityHealth>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub user_account_control: Option<SecurityHealth>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub security_center_service: Option<SecurityHealth>,
    pub memory_integrity: MemoryIntegrity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityHealth {
    Good,
    Poor,
    Snooze,
    NotMonitored,
}

/// Configuration and runtime observations are independent; neither implies the other.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryIntegrity {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub configured: Option<MemoryIntegrityConfiguration>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub running: Option<MemoryIntegrityRunningState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryIntegrityConfiguration {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryIntegrityRunningState {
    Running,
    NotRunning,
}
