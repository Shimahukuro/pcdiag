//! Shared data contracts for pcdiag artifacts.

pub mod collection;
pub mod diagnosis;
pub mod status;
pub mod validation;

pub use collection::{
    Collection, CommitMemory, ConnectedDevice, DeviceDriver, DeviceState, Gpu, GpuAdapterType,
    GpuDeviceState, GpuDriver, GpuMemory, GpuPciIdentifiers, MemoryCollection, PhysicalMemory,
    VirtualMemory,
};
pub use diagnosis::{
    Criterion, Diagnosis, DiagnosisSummary, EvaluationCounts, EvaluationReason, Evidence,
    FindingCounts, MeasurementUnit, Recommendation, RuleEvaluation, RuleEvaluationStatus,
    RuleSetInfo, Severity,
};
pub use status::{
    CollectionMessage, CollectionStatus, CollectorName, CollectorResult, CollectorStatus,
    FieldCollectionResult, FieldCollectionStatus,
};
pub use validation::{ValidationError, ValidationErrors};
