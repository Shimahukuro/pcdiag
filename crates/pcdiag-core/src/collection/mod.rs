mod gpu;
mod memory;

pub use gpu::{Gpu, GpuAdapterType, GpuDeviceState, GpuDriver, GpuMemory, GpuPciIdentifiers};
pub use memory::{CommitMemory, MemoryCollection, PhysicalMemory, VirtualMemory};

use serde::{Deserialize, Serialize};

use self::memory::deserialize_required_nullable;

/// Normalized facts collected from a diagnostic target.
///
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub memory: MemoryCollection,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub gpus: Option<Vec<Gpu>>,
}
