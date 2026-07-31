mod clock;
mod cpu;
mod device;
mod event_log;
mod firmware;
mod gpu;
mod memory;
mod physical_disk;
mod windows;
mod windows_update;

pub use cpu::{CpuCollection, CpuFeatures, CpuInstructionSet, CpuPackage, CpuTopology};
pub use event_log::{EventLogCollection, EventLogEntry, EventLogLevel};
pub use firmware::{FirmwareCollection, FirmwareInterfaceType, FirmwareOperationalStatus};
pub use gpu::{Gpu, GpuAdapterType, GpuDeviceState, GpuDriver, GpuMemory, GpuPciIdentifiers};
pub use memory::{CommitMemory, MemoryCollection, PhysicalMemory, VirtualMemory};
pub use physical_disk::{
    DiskBusType, DiskPartition, DiskSmart, PartitionStyle, PhysicalDisk, SmartProtocol,
    StorageCollection, StorageVolume, VolumeExtent,
};
pub use windows::{BootMode, SystemArchitecture, WindowsCollection};
pub use windows_update::{
    WindowsUpdateCollection, WindowsUpdateHistoryEntry, WindowsUpdateOperation, WindowsUpdateResult,
};

use serde::{Deserialize, Serialize};

use self::memory::deserialize_required_nullable;

/// Normalized facts collected from a diagnostic target.
///
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub windows: WindowsCollection,
    pub windows_updates: WindowsUpdateCollection,
    pub clock: ClockCollection,
    pub cpu: CpuCollection,
    pub firmware: FirmwareCollection,
    pub memory: MemoryCollection,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub gpus: Option<Vec<Gpu>>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub devices: Option<Vec<ConnectedDevice>>,
    pub event_logs: EventLogCollection,
    pub storage: StorageCollection,
}
pub use clock::{ClockCollection, HardwareClock, WindowsServiceState};
pub use device::{ConnectedDevice, DeviceDriver, DeviceState};
