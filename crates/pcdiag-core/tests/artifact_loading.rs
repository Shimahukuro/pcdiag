use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use pcdiag_core::{
    ArtifactFile, ArtifactManifest, ArtifactStatus, ArtifactType, ToolInfo,
    load_collection_artifact, sha256_hex,
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
