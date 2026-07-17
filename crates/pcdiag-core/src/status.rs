use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectorName {
    Windows,
    Clock,
    Memory,
    Gpu,
    Devices,
    PhysicalDisks,
    Partitions,
    Volumes,
    Smart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectorStatus {
    Success,
    Partial,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldCollectionStatus {
    SourceNull,
    Unsupported,
    NotApplicable,
    PermissionDenied,
    Timeout,
    Failed,
    NotCollected,
    InvalidValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionStatus {
    pub collectors: Vec<CollectorResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectorResult {
    pub name: CollectorName,
    pub status: CollectorStatus,
    pub duration_ms: u64,
    pub messages: Vec<CollectionMessage>,
    pub fields: Vec<FieldCollectionResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionMessage {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldCollectionResult {
    pub path: String,
    pub status: FieldCollectionStatus,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_code: Option<i64>,
}
