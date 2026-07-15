//! Windows-specific collectors for pcdiag.

mod gpu;
mod memory;

pub use gpu::{GpuCollectionResult, collect_gpus};
pub use memory::{MemoryCollectionResult, collect_memory};
