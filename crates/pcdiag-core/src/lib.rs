//! Shared data specifications for pcdiag artifacts.

pub mod collection;
pub mod diagnosis;
pub mod manifest;
pub mod status;
pub mod validation;

pub use collection::{
    BootMode, ClockCollection, Collection, CommitMemory, ConnectedDevice, CpuCollection,
    CpuFeatures, CpuInstructionSet, CpuPackage, CpuTopology, DeviceDriver, DeviceState,
    DiskBusType, DiskPartition, DiskSmart, FirmwareCollection, FirmwareInterfaceType,
    FirmwareOperationalStatus, Gpu, GpuAdapterType, GpuDeviceState, GpuDriver, GpuMemory,
    GpuPciIdentifiers, HardwareClock, MemoryCollection, PartitionStyle, PhysicalDisk,
    PhysicalMemory, SmartProtocol, StorageCollection, StorageVolume, SystemArchitecture,
    VirtualMemory, VolumeExtent, WindowsCollection, WindowsServiceState,
};
pub use diagnosis::{
    Criterion, Diagnosis, DiagnosisSummary, EvaluationCounts, EvaluationReason, Evidence,
    FindingCounts, MeasurementUnit, Recommendation, RuleEvaluation, RuleEvaluationStatus,
    RuleSetInfo, Severity,
};
pub use manifest::{
    ArtifactFile, ArtifactInput, ArtifactManifest, ArtifactStatus, ArtifactType,
    ManifestValidationError, ManifestValidationErrors, ToolInfo, display_id, is_uuid_v4,
};
pub use status::{
    CollectionMessage, CollectionStatus, CollectorName, CollectorResult, CollectorStatus,
    FieldCollectionResult, FieldCollectionStatus,
};
pub use validation::{ValidationError, ValidationErrors};
