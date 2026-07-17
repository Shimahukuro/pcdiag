//! Windows-specific collectors for pcdiag.

mod devices;
mod gpu;
mod memory;

pub use devices::{DeviceCollectionResult, collect_devices};
pub use gpu::{GpuCollectionResult, collect_gpus};
pub use memory::{MemoryCollectionResult, collect_memory};
