use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use pcdiag_core::{
    ArtifactFile, ArtifactInput, ArtifactManifest, ArtifactStatus, ArtifactType, ToolInfo,
    diagnose_collection, load_collection_artifact, load_diagnosis_artifact, sha256_hex,
};

const COLLECTION: &[u8] = include_bytes!("fixtures/memory-success-collection.json");
const STATUS: &[u8] = include_bytes!("fixtures/memory-success-status.json");
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

#[test]
fn loads_and_validates_collection_artifact() {
    let directory = create_artifact();

    let loaded = load_collection_artifact(&directory).unwrap();

    assert_eq!(loaded.manifest.artifact_type, ArtifactType::Collection);
    assert_eq!(loaded.collection.memory.physical.load_percent, Some(97.0));
    remove(directory);
}

#[test]
fn rejects_modified_declared_file() {
    let directory = create_artifact();
    fs::write(directory.join("collection.json"), b"{}\n").unwrap();

    let error = load_collection_artifact(&directory).unwrap_err();

    assert!(error.message.contains("size mismatch") || error.message.contains("SHA-256 mismatch"));
    remove(directory);
}

#[test]
fn rejects_unlisted_file() {
    let directory = create_artifact();
    fs::write(directory.join("unexpected.txt"), b"unexpected").unwrap();

    let error = load_collection_artifact(&directory).unwrap_err();

    assert!(error.message.contains("not listed"));
    remove(directory);
}

#[test]
fn rejects_missing_declared_file() {
    let directory = create_artifact();
    fs::remove_file(directory.join("status.json")).unwrap();

    let error = load_collection_artifact(&directory).unwrap_err();

    assert!(error.path.ends_with("status.json"));
    remove(directory);
}

#[test]
fn loads_diagnosis_only_when_it_matches_collection() {
    let collection_directory = create_artifact();
    let collection = load_collection_artifact(&collection_directory).unwrap();
    let diagnosis_directory = collection_directory.parent().unwrap().join(format!(
        "{}-diagnosis",
        collection_directory.file_name().unwrap().to_string_lossy()
    ));
    if diagnosis_directory.exists() {
        fs::remove_dir_all(&diagnosis_directory).unwrap();
    }
    fs::create_dir(&diagnosis_directory).unwrap();
    let diagnosis = diagnose_collection(&collection.collection);
    let diagnosis_bytes = serde_json::to_vec_pretty(&diagnosis).unwrap();
    fs::write(diagnosis_directory.join("diagnosis.json"), &diagnosis_bytes).unwrap();
    let manifest = ArtifactManifest {
        manifest_schema_version: "1.0".into(),
        artifact_schema_version: "2.0".into(),
        session_id: collection.manifest.session_id.clone(),
        artifact_id: "43d39e67-c8f1-4c9b-a20e-a65dbba20295".into(),
        artifact_type: ArtifactType::Diagnosis,
        status: ArtifactStatus::Complete,
        started_at: "2026-07-18T01:31:00.000Z".into(),
        completed_at: "2026-07-18T01:31:01.000Z".into(),
        observed_utc_offset_minutes: 540,
        duration_ms: 1_000,
        tool: ToolInfo {
            name: "pcdiag".into(),
            version: "0.1.0".into(),
        },
        inputs: vec![ArtifactInput {
            artifact_id: collection.manifest.artifact_id.clone(),
            artifact_type: ArtifactType::Collection,
        }],
        files: vec![file("diagnosis.json", &diagnosis_bytes)],
    };
    fs::write(
        diagnosis_directory.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let loaded = load_diagnosis_artifact(&diagnosis_directory, &collection).unwrap();
    assert_eq!(
        loaded.diagnosis.rule_set.version,
        diagnosis.rule_set.version
    );

    remove(collection_directory);
    remove(diagnosis_directory);
}

fn create_artifact() -> PathBuf {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "pcdiag-artifact-loading-{}-{id}",
        std::process::id()
    ));
    if directory.exists() {
        fs::remove_dir_all(&directory).unwrap();
    }
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("collection.json"), COLLECTION).unwrap();
    fs::write(directory.join("status.json"), STATUS).unwrap();
    let manifest = ArtifactManifest {
        manifest_schema_version: "1.0".into(),
        artifact_schema_version: "2.0".into(),
        session_id: "a3f17c92-d604-4be8-9ea7-6ab7b92e41c5".into(),
        artifact_id: "831d1074-1145-4a66-bfa2-169903866adb".into(),
        artifact_type: ArtifactType::Collection,
        status: ArtifactStatus::Complete,
        started_at: "2026-07-18T01:30:15.000Z".into(),
        completed_at: "2026-07-18T01:30:16.000Z".into(),
        observed_utc_offset_minutes: 540,
        duration_ms: 1_000,
        tool: ToolInfo {
            name: "pcdiag".into(),
            version: "0.1.0".into(),
        },
        inputs: vec![],
        files: vec![
            file("collection.json", COLLECTION),
            file("status.json", STATUS),
        ],
    };
    fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    directory
}

fn file(path: &str, bytes: &[u8]) -> ArtifactFile {
    ArtifactFile {
        path: path.into(),
        media_type: "application/json".into(),
        size_bytes: bytes.len() as u64,
        sha256: sha256_hex(bytes),
    }
}

fn remove(directory: PathBuf) {
    fs::remove_dir_all(directory).unwrap();
}
