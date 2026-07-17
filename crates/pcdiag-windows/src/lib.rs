//! Windows-specific collectors for pcdiag.

mod devices;
mod gpu;
mod memory;
mod physical_disks;

pub use devices::{DeviceCollectionResult, collect_devices};
pub use gpu::{GpuCollectionResult, collect_gpus};
pub use memory::{MemoryCollectionResult, collect_memory};
pub use physical_disks::{PhysicalDiskCollectionResult, collect_physical_disks};
