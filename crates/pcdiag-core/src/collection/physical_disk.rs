use serde::{Deserialize, Serialize};

use super::memory::deserialize_required_nullable;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageCollection {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub disks: Option<Vec<PhysicalDisk>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicalDisk {
    pub number: u32,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub model: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub manufacturer: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub firmware_revision: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub bus_type: Option<DiskBusType>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub capacity_bytes: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub logical_sector_size_bytes: Option<u32>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub removable: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskBusType {
    Unknown,
    Scsi,
    Atapi,
    Ata,
    Ieee1394,
    Ssa,
    Fibre,
    Usb,
    Raid,
    Iscsi,
    Sas,
    Sata,
    Sd,
    Mmc,
    Virtual,
    FileBackedVirtual,
    StorageSpaces,
    Nvme,
    StorageClassMemory,
    Ufs,
}
