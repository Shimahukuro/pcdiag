mod memory;

pub use memory::{CommitMemory, MemoryCollection, PhysicalMemory, VirtualMemory};

use serde::{Deserialize, Serialize};

/// Normalized facts collected from a diagnostic target.
///
/// This first vertical slice contains only the memory category. Other
/// categories will be added after their three-file contracts are designed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub memory: MemoryCollection,
}
