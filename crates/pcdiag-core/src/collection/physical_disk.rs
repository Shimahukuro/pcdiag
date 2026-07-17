use serde::{Deserialize, Serialize};

use super::memory::deserialize_required_nullable;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageCollection {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub disks: Option<Vec<PhysicalDisk>>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub partitions: Option<Vec<DiskPartition>>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub volumes: Option<Vec<StorageVolume>>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub smart: Option<Vec<DiskSmart>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskSmart {
    pub disk_number: u32,
    pub protocol: SmartProtocol,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub predict_failure: Option<bool>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub critical_warning: Option<u8>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub temperature_celsius: Option<i16>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub available_spare_percent: Option<u8>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub percentage_used: Option<u8>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub power_on_hours: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub unsafe_shutdowns: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub media_errors: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartProtocol {
    Nvme,
    FailurePrediction,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskPartition {
    pub disk_number: u32,
    pub partition_number: u32,
    pub offset_bytes: u64,
    pub length_bytes: u64,
    pub style: PartitionStyle,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub type_id: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub bootable: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionStyle {
    Mbr,
    Gpt,
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageVolume {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub mount_points: Option<Vec<String>>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub file_system: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub capacity_bytes: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub free_bytes: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub extents: Option<Vec<VolumeExtent>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeExtent {
    pub disk_number: u32,
    pub offset_bytes: u64,
    pub length_bytes: u64,
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
