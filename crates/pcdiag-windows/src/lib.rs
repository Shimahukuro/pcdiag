//! Windows-specific collectors for pcdiag.

mod clock;
mod collect;
mod cpu;
mod devices;
mod event_logs;
mod firmware;
mod gpu;
mod memory;
mod partitions;
mod physical_disks;
mod smart;
mod volumes;
mod windows_info;
mod windows_updates;

pub use clock::{ClockCollectionResult, collect_clock};
pub use collect::{CompleteCollectionResult, collect_all};
pub use cpu::{CpuCollectionResult, collect_cpu};
pub use devices::{DeviceCollectionResult, collect_devices};
pub use event_logs::{EventLogCollectionResult, collect_event_logs};
pub use firmware::{FirmwareCollectionResult, collect_firmware};
pub use gpu::{GpuCollectionResult, collect_gpus};
pub use memory::{MemoryCollectionResult, collect_memory};
pub use partitions::{PartitionCollectionResult, collect_partitions};
pub use physical_disks::{PhysicalDiskCollectionResult, collect_physical_disks};
pub use smart::{SmartCollectionResult, collect_smart};
pub use volumes::{VolumeCollectionResult, collect_volumes};
pub use windows_info::{WindowsCollectionResult, collect_windows_info};
pub use windows_updates::{
    WindowsUpdateCollectionOptions, WindowsUpdateCollectionResult, collect_windows_updates,
};
