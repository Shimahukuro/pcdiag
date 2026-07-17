use serde::{Deserialize, Serialize};

use super::memory::deserialize_required_nullable;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirmwareCollection {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub vendor: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub version: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub release_date: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub interface_type: Option<FirmwareInterfaceType>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub secure_boot_enabled: Option<bool>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub status: Option<FirmwareOperationalStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FirmwareInterfaceType {
    Bios,
    Uefi,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FirmwareOperationalStatus {
    Ok,
    Degraded,
    Error,
    Unknown,
}
