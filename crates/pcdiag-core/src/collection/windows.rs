use serde::{Deserialize, Serialize};

use super::memory::deserialize_required_nullable;

/// Basic facts about the running Windows installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsCollection {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub edition: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub version: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub build_number: Option<u32>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub architecture: Option<SystemArchitecture>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub booted_at: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub uptime_ms: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub boot_mode: Option<BootMode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemArchitecture {
    X86,
    X86_64,
    Arm,
    Arm64,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootMode {
    Bios,
    Uefi,
    Unknown,
}
