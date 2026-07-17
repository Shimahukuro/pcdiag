use serde::{Deserialize, Serialize};

use super::{SystemArchitecture, memory::deserialize_required_nullable};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuCollection {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub architecture: Option<SystemArchitecture>,
    pub topology: CpuTopology,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub packages: Option<Vec<CpuPackage>>,
    pub features: CpuFeatures,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuTopology {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub physical_packages: Option<u32>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub physical_cores: Option<u32>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub logical_processors: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuPackage {
    pub package_index: u32,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub manufacturer: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub model: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub physical_cores: Option<u32>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub logical_processors: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuFeatures {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub available_instruction_sets: Option<Vec<CpuInstructionSet>>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub hardware_virtualization_supported: Option<bool>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub virtualization_firmware_enabled: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CpuInstructionSet {
    Sse2,
    Sse3,
    Ssse3,
    Sse4_1,
    Sse4_2,
    Avx,
    Avx2,
    Aes,
    Sha,
    Neon,
    ArmV8Crypto,
}
