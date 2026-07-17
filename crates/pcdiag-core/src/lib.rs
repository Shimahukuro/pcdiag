//! Shared data specifications for pcdiag artifacts.

pub mod collection;
pub mod diagnosis;
pub mod status;
pub mod validation;

pub use collection::{
    BootMode, Collection, CommitMemory, ConnectedDevice, DeviceDriver, DeviceState, DiskBusType,
    DiskPartition, DiskSmart, Gpu, GpuAdapterType, GpuDeviceState, GpuDriver, GpuMemory,
    GpuPciIdentifiers, MemoryCollection, PartitionStyle, PhysicalDisk, PhysicalMemory,
    SmartProtocol, StorageCollection, StorageVolume, SystemArchitecture, VirtualMemory,
    VolumeExtent, WindowsCollection,
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
