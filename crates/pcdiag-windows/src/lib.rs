//! Windows-specific collectors for pcdiag.

mod clock;
mod devices;
mod gpu;
mod memory;
mod partitions;
mod physical_disks;
mod smart;
mod volumes;
mod windows_info;

pub use clock::{ClockCollectionResult, collect_clock};
pub use devices::{DeviceCollectionResult, collect_devices};
pub use gpu::{GpuCollectionResult, collect_gpus};
pub use memory::{MemoryCollectionResult, collect_memory};
pub use partitions::{PartitionCollectionResult, collect_partitions};
pub use physical_disks::{PhysicalDiskCollectionResult, collect_physical_disks};
pub use smart::{SmartCollectionResult, collect_smart};
pub use volumes::{VolumeCollectionResult, collect_volumes};
pub use windows_info::{WindowsCollectionResult, collect_windows_info};
