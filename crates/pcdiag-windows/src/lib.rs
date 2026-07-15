//! Windows-specific collectors for pcdiag.

mod memory;

pub use memory::{MemoryCollectionResult, collect_memory};
