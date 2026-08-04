use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Instant,
};

use pcdiag_core::{
    ArtifactInput, ArtifactManifest, ArtifactStatus, ArtifactType, LoadedCollectionArtifact,
    ToolInfo, diagnose_collection, load_collection_artifact,
};

use crate::{
    bundle::{self, artifact_file, pretty_json, write_new},
    interrupt,
};

pub fn diagnose_bundle(session_directory: &Path) -> Result<PathBuf, DiagnoseError> {
    interrupt::check("diagnose")?;
    let started = Instant::now();
    let started_at = bundle::platform::utc_timestamp()?;
    let observed_utc_offset_minutes = bundle::platform::utc_offset_minutes()?;
    let collection = load_collection_artifact(&session_directory.join("collection"))?;
    interrupt::check("diagnose")?;
    let artifact_id = unique_artifact_id(&collection)?;
    let diagnosis = diagnose_collection(&collection.collection);
    interrupt::check("diagnose")?;
    diagnosis.validate_against(&collection.collection)?;
    interrupt::check("diagnose")?;
    let completed_at = bundle::platform::utc_timestamp()?;
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    write_diagnosis(
        session_directory,
        collection,
        diagnosis,
        DiagnosisTiming {
            artifact_id,
            started_at,
            completed_at,
            observed_utc_offset_minutes,
            duration_ms,
        },
    )
}

fn unique_artifact_id(collection: &LoadedCollectionArtifact) -> Result<String, DiagnoseError> {
    for _ in 0..16 {
        let artifact_id = bundle::platform::uuid_v4()?;
        if artifact_id != collection.manifest.session_id
            && artifact_id != collection.manifest.artifact_id
        {
            return Ok(artifact_id);
        }
    }
    Err(DiagnoseError::ArtifactIdCollision)
}

struct DiagnosisTiming {
    artifact_id: String,
    started_at: String,
    completed_at: String,
    observed_utc_offset_minutes: i32,
    duration_ms: u64,
}

fn write_diagnosis(
    session_directory: &Path,
    collection: LoadedCollectionArtifact,
    diagnosis: pcdiag_core::Diagnosis,
    timing: DiagnosisTiming,
) -> Result<PathBuf, DiagnoseError> {
    let final_directory = session_directory.join("diagnosis");
    let incomplete_directory = session_directory.join("diagnosis.incomplete");
    if final_directory.exists() {
        return Err(DiagnoseError::AlreadyExists(final_directory));
    }
    if incomplete_directory.exists() {
        return Err(DiagnoseError::IncompleteExists(incomplete_directory));
    }
    fs::create_dir(&incomplete_directory)?;
    interrupt::check_with_log("diagnose", &incomplete_directory)?;
    let diagnosis_bytes = pretty_json(&diagnosis)?;
    interrupt::check_with_log("diagnose", &incomplete_directory)?;
    write_new(
        &incomplete_directory.join("diagnosis.json"),
        &diagnosis_bytes,
    )?;
    let artifact_status = if diagnosis
        .evaluations
        .iter()
        .any(|evaluation| evaluation.status == pcdiag_core::RuleEvaluationStatus::Failed)
    {
        ArtifactStatus::Partial
    } else {
        ArtifactStatus::Complete
    };
    let manifest = ArtifactManifest {
        manifest_schema_version: "1.0".into(),
        artifact_schema_version: "2.0".into(),
        session_id: collection.manifest.session_id,
        artifact_id: timing.artifact_id,
        artifact_type: ArtifactType::Diagnosis,
        status: artifact_status,
        started_at: timing.started_at,
        completed_at: timing.completed_at,
        observed_utc_offset_minutes: timing.observed_utc_offset_minutes,
        duration_ms: timing.duration_ms,
        tool: ToolInfo {
            name: "pcdiag".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        inputs: vec![ArtifactInput {
            artifact_id: collection.manifest.artifact_id,
            artifact_type: ArtifactType::Collection,
        }],
        files: vec![artifact_file("diagnosis.json", &diagnosis_bytes)],
    };
    manifest.validate()?;
    interrupt::check_with_log("diagnose", &incomplete_directory)?;
    write_new(
        &incomplete_directory.join("manifest.json"),
        &pretty_json(&manifest)?,
    )?;
    interrupt::check_with_log("diagnose", &incomplete_directory)?;
    fs::rename(&incomplete_directory, &final_directory)?;
    Ok(final_directory)
}

#[derive(Debug)]
pub enum DiagnoseError {
    Interrupted(interrupt::Interrupted),
    Io(io::Error),
    Json(serde_json::Error),
    Bundle(bundle::BundleError),
    Artifact(pcdiag_core::ArtifactLoadError),
    Validation(pcdiag_core::ValidationErrors),
    Manifest(pcdiag_core::ManifestValidationErrors),
    AlreadyExists(PathBuf),
    IncompleteExists(PathBuf),
    ArtifactIdCollision,
}

impl std::fmt::Display for DiagnoseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Interrupted(error) => error.fmt(formatter),
            Self::Io(error) => write!(formatter, "ファイル操作に失敗しました: {error}"),
            Self::Json(error) => write!(formatter, "JSON生成に失敗しました: {error}"),
            Self::Bundle(error) => write!(formatter, "実行環境情報を取得できませんでした: {error}"),
            Self::Artifact(error) => write!(formatter, "収集成果物が不正です: {error}"),
            Self::Validation(error) => write!(formatter, "診断結果が不正です: {error}"),
            Self::Manifest(error) => write!(formatter, "診断マニフェストが不正です: {error}"),
            Self::AlreadyExists(path) => write!(
                formatter,
                "診断成果物は既に存在します。上書きしません: {}",
                path.display()
            ),
            Self::IncompleteExists(path) => write!(
                formatter,
                "未完了の診断ディレクトリが存在します: {}",
                path.display()
            ),
            Self::ArtifactIdCollision => {
                formatter.write_str("一意な診断成果物IDを生成できませんでした")
            }
        }
    }
}

impl std::error::Error for DiagnoseError {}

impl DiagnoseError {
    pub(crate) fn is_interrupted(&self) -> bool {
        matches!(self, Self::Interrupted(_))
            || matches!(self, Self::Bundle(error) if error.is_interrupted())
    }
}

impl From<interrupt::Interrupted> for DiagnoseError {
    fn from(value: interrupt::Interrupted) -> Self {
        Self::Interrupted(value)
    }
}

impl From<io::Error> for DiagnoseError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<serde_json::Error> for DiagnoseError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}
impl From<bundle::BundleError> for DiagnoseError {
    fn from(value: bundle::BundleError) -> Self {
        Self::Bundle(value)
    }
}
impl From<pcdiag_core::ArtifactLoadError> for DiagnoseError {
    fn from(value: pcdiag_core::ArtifactLoadError) -> Self {
        Self::Artifact(value)
    }
}
impl From<pcdiag_core::ValidationErrors> for DiagnoseError {
    fn from(value: pcdiag_core::ValidationErrors) -> Self {
        Self::Validation(value)
    }
}
impl From<pcdiag_core::ManifestValidationErrors> for DiagnoseError {
    fn from(value: pcdiag_core::ManifestValidationErrors) -> Self {
        Self::Manifest(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcdiag_core::{ArtifactFile, Collection, CollectionStatus};

    fn loaded_collection() -> LoadedCollectionArtifact {
        let collection: Collection = serde_json::from_str(include_str!(
            "../../pcdiag-core/tests/fixtures/memory-success-collection.json"
        ))
        .unwrap();
        let status: CollectionStatus = serde_json::from_str(include_str!(
            "../../pcdiag-core/tests/fixtures/memory-success-status.json"
        ))
        .unwrap();
        LoadedCollectionArtifact {
            manifest: ArtifactManifest {
                manifest_schema_version: "1.0".into(),
                artifact_schema_version: "2.0".into(),
                session_id: "a3f17c92-d604-4be8-9ea7-6ab7b92e41c5".into(),
                artifact_id: "831d1074-1145-4a66-bfa2-169903866adb".into(),
                artifact_type: ArtifactType::Collection,
                status: ArtifactStatus::Complete,
                started_at: "2026-07-18T01:00:00.000Z".into(),
                completed_at: "2026-07-18T01:00:01.000Z".into(),
                observed_utc_offset_minutes: 540,
                duration_ms: 1_000,
                tool: ToolInfo {
                    name: "pcdiag".into(),
                    version: "0.1.0".into(),
                },
                inputs: vec![],
                files: vec![
                    ArtifactFile {
                        path: "collection.json".into(),
                        media_type: "application/json".into(),
                        size_bytes: 1,
                        sha256: "0".repeat(64),
                    },
                    ArtifactFile {
                        path: "status.json".into(),
                        media_type: "application/json".into(),
                        size_bytes: 1,
                        sha256: "1".repeat(64),
                    },
                ],
            },
            collection,
            status,
        }
    }

    fn timing(artifact_id: &str) -> DiagnosisTiming {
        DiagnosisTiming {
            artifact_id: artifact_id.into(),
            started_at: "2026-07-18T01:01:00.000Z".into(),
            completed_at: "2026-07-18T01:01:00.010Z".into(),
            observed_utc_offset_minutes: 540,
            duration_ms: 10,
        }
    }

    #[test]
    fn writes_diagnosis_and_refuses_to_overwrite_it() {
        let root =
            std::env::temp_dir().join(format!("pcdiag-diagnosis-test-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir(&root).unwrap();
        let loaded = loaded_collection();
        let diagnosis = diagnose_collection(&loaded.collection);
        diagnosis.validate_against(&loaded.collection).unwrap();
        let written = write_diagnosis(
            &root,
            loaded,
            diagnosis,
            timing("74719796-03c1-475d-bc40-d81522811d0d"),
        )
        .unwrap();

        let manifest: ArtifactManifest =
            serde_json::from_slice(&fs::read(written.join("manifest.json")).unwrap()).unwrap();
        manifest.validate().unwrap();
        assert_eq!(manifest.inputs[0].artifact_type, ArtifactType::Collection);
        let diagnosis: pcdiag_core::Diagnosis =
            serde_json::from_slice(&fs::read(written.join("diagnosis.json")).unwrap()).unwrap();
        assert_eq!(
            diagnosis.evaluations[0].status,
            pcdiag_core::RuleEvaluationStatus::Triggered
        );

        let second_collection = loaded_collection();
        let second_diagnosis = diagnose_collection(&second_collection.collection);
        let error = write_diagnosis(
            &root,
            second_collection,
            second_diagnosis,
            timing("d16c15e8-477d-47f5-8f2d-6da44a8bc82b"),
        )
        .unwrap_err();
        assert!(matches!(error, DiagnoseError::AlreadyExists(_)));
        fs::remove_dir_all(root).unwrap();
    }
}
