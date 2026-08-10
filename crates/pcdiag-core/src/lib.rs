//! Shared data specifications for pcdiag artifacts.

pub mod ai_guide;
pub mod artifact;
pub mod collection;
pub mod diagnosis;
pub mod hash;
pub mod manifest;
pub mod rules;
pub mod status;
pub mod validation;

pub use ai_guide::{
    AI_DIAGNOSIS_GUIDE, AI_DIAGNOSIS_GUIDE_FILE_NAME, AI_DIAGNOSIS_GUIDE_MEDIA_TYPE,
    validate_ai_diagnosis_guide,
};
pub use artifact::{
    ArtifactLoadError, LoadedCollectionArtifact, LoadedDiagnosisArtifact, load_collection_artifact,
    load_diagnosis_artifact,
};
pub use collection::{
    BootMode, ClockCollection, Collection, CommitMemory, ConnectedDevice, CpuCollection,
    CpuFeatures, CpuInstructionSet, CpuPackage, CpuTopology, DeviceDriver, DeviceState,
    DiskBusType, DiskPartition, DiskSmart, EventLogCollection, EventLogEntry, EventLogLevel,
    FirmwareCollection, FirmwareInterfaceType, FirmwareOperationalStatus, Gpu, GpuAdapterType,
    GpuDeviceState, GpuDriver, GpuMemory, GpuPciIdentifiers, HardwareClock, MemoryCollection,
    PartitionStyle, PhysicalDisk, PhysicalMemory, SmartProtocol, StorageCollection, StorageVolume,
    SystemArchitecture, VirtualMemory, VolumeExtent, WindowsCollection, WindowsServiceState,
    WindowsUpdateCollection, WindowsUpdateHistoryEntry, WindowsUpdateOperation,
    WindowsUpdateResult,
};
pub use diagnosis::{
    Criterion, Diagnosis, DiagnosisSummary, EvaluationCounts, EvaluationReason, Evidence,
    FindingCounts, MeasurementUnit, Recommendation, RuleEvaluation, RuleEvaluationStatus,
    RuleSetInfo, Severity,
};
pub use hash::sha256_hex;
pub use manifest::{
    ArtifactFile, ArtifactInput, ArtifactManifest, ArtifactStatus, ArtifactType,
    CURRENT_ARTIFACT_SCHEMA_VERSION, CURRENT_MANIFEST_SCHEMA_VERSION, ManifestValidationError,
    ManifestValidationErrors, ToolInfo, display_id, is_uuid_v4,
};
pub use rules::{
    BUILTIN_RECOMMENDATION_CODES, BUILTIN_RULE_IDS, BUILTIN_RULE_SET_NAME,
    BUILTIN_RULE_SET_VERSION, diagnose_collection,
};
pub use status::{
    CollectionMessage, CollectionStatus, CollectorName, CollectorResult, CollectorStatus,
    FieldCollectionResult, FieldCollectionStatus,
};
pub use validation::{ValidationError, ValidationErrors};
