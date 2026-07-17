mod device;
mod gpu;
mod memory;
mod physical_disk;

pub use gpu::{Gpu, GpuAdapterType, GpuDeviceState, GpuDriver, GpuMemory, GpuPciIdentifiers};
pub use memory::{CommitMemory, MemoryCollection, PhysicalMemory, VirtualMemory};
pub use physical_disk::{
    DiskBusType, DiskPartition, PartitionStyle, PhysicalDisk, StorageCollection, StorageVolume,
    VolumeExtent,
};

use serde::{Deserialize, Serialize};

use self::memory::deserialize_required_nullable;

/// Normalized facts collected from a diagnostic target.
///
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub memory: MemoryCollection,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub gpus: Option<Vec<Gpu>>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub devices: Option<Vec<ConnectedDevice>>,
    pub storage: StorageCollection,
}
pub use device::{ConnectedDevice, DeviceDriver, DeviceState};
