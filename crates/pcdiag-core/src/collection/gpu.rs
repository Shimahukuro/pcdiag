use serde::{Deserialize, Serialize};

use super::memory::deserialize_required_nullable;

/// Normalized facts for one GPU reported by the operating system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gpu {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub name: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub vendor: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub device_instance_id: Option<String>,
    pub pci: GpuPciIdentifiers,
    pub memory: GpuMemory,
    pub driver: GpuDriver,
    pub device_state: GpuDeviceState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuPciIdentifiers {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub vendor_id: Option<u16>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub device_id: Option<u16>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub subsystem_id: Option<u32>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub revision_id: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuMemory {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub dedicated_video_bytes: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub dedicated_system_bytes: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub shared_system_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuDriver {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub version: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuDeviceState {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub present: Option<bool>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub enabled: Option<bool>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub problem_code: Option<u32>,
}
