use serde::{Deserialize, Deserializer, Serialize};

/// System-wide memory facts reported by Windows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryCollection {
    pub physical: PhysicalMemory,
    pub commit: CommitMemory,
    #[serde(rename = "virtual")]
    pub virtual_memory: VirtualMemory,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicalMemory {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub total_bytes: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub available_bytes: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub load_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitMemory {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub limit_bytes: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub available_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualMemory {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub total_bytes: Option<u64>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub available_bytes: Option<u64>,
}

/// Deserializes an explicitly present value that may be JSON null.
///
/// Applying `deserialize_with` makes the object key mandatory while still
/// mapping an explicit null to `None`. Serde's default `Option<T>` behavior
/// would otherwise treat a missing key and an explicit null identically.
pub(crate) fn deserialize_required_nullable<'de, D, T>(
    deserializer: D,
) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
