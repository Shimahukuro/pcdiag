use std::{collections::HashMap, fmt, fs, path::Path};

use crate::{ArtifactManifest, ArtifactType, Collection, CollectionStatus, Diagnosis, sha256_hex};

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedCollectionArtifact {
    pub manifest: ArtifactManifest,
    pub collection: Collection,
    pub status: CollectionStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedDiagnosisArtifact {
    pub manifest: ArtifactManifest,
    pub diagnosis: Diagnosis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactLoadError {
    pub path: String,
    pub message: String,
}

impl fmt::Display for ArtifactLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for ArtifactLoadError {}

pub fn load_collection_artifact(
    artifact_directory: &Path,
) -> Result<LoadedCollectionArtifact, ArtifactLoadError> {
    let (manifest, verified) = load_verified_files(artifact_directory, ArtifactType::Collection)?;

    let collection_path = artifact_directory.join("collection.json");
    let status_path = artifact_directory.join("status.json");
    let collection: Collection = parse_json(
        &collection_path,
        verified
            .get("collection.json")
            .expect("validated collection manifest contains collection.json"),
    )?;
    let status: CollectionStatus = parse_json(
        &status_path,
        verified
            .get("status.json")
            .expect("validated collection manifest contains status.json"),
    )?;
    collection
        .validate_with_status(&status)
        .map_err(|error| ArtifactLoadError {
            path: display(artifact_directory),
            message: format!("collection/status validation failed: {error}"),
        })?;

    Ok(LoadedCollectionArtifact {
        manifest,
        collection,
        status,
    })
}

pub fn load_diagnosis_artifact(
    artifact_directory: &Path,
    collection: &LoadedCollectionArtifact,
) -> Result<LoadedDiagnosisArtifact, ArtifactLoadError> {
    let (manifest, verified) = load_verified_files(artifact_directory, ArtifactType::Diagnosis)?;
    if manifest.session_id != collection.manifest.session_id {
        return Err(ArtifactLoadError {
            path: display(&artifact_directory.join("manifest.json")),
            message: "session_id does not match collection artifact".into(),
        });
    }
    let input = manifest
        .inputs
        .iter()
        .find(|input| input.artifact_type == ArtifactType::Collection)
        .expect("validated diagnosis manifest contains collection input");
    if input.artifact_id != collection.manifest.artifact_id {
        return Err(ArtifactLoadError {
            path: display(&artifact_directory.join("manifest.json")),
            message: "collection input artifact_id does not match collection artifact".into(),
        });
    }
    let diagnosis_path = artifact_directory.join("diagnosis.json");
    let diagnosis: Diagnosis = parse_json(
        &diagnosis_path,
        verified
            .get("diagnosis.json")
            .expect("validated diagnosis manifest contains diagnosis.json"),
    )?;
    diagnosis
        .validate_against(&collection.collection)
        .map_err(|error| ArtifactLoadError {
            path: display(&diagnosis_path),
            message: format!("diagnosis/collection validation failed: {error}"),
        })?;
    Ok(LoadedDiagnosisArtifact {
        manifest,
        diagnosis,
    })
}

fn load_verified_files(
    artifact_directory: &Path,
    expected_type: ArtifactType,
) -> Result<(ArtifactManifest, HashMap<String, Vec<u8>>), ArtifactLoadError> {
    let manifest_path = artifact_directory.join("manifest.json");
    let manifest_bytes = read(&manifest_path)?;
    let manifest: ArtifactManifest = parse_json(&manifest_path, &manifest_bytes)?;
    manifest.validate().map_err(|error| ArtifactLoadError {
        path: display(&manifest_path),
        message: error.to_string(),
    })?;
    if manifest.artifact_type != expected_type {
        return Err(ArtifactLoadError {
            path: display(&manifest_path),
            message: format!("artifact_type must be {expected_type:?}").to_lowercase(),
        });
    }
    reject_unlisted_files(artifact_directory, &manifest)?;
    let mut verified = HashMap::new();
    for declared in &manifest.files {
        let path = artifact_directory.join(&declared.path);
        let bytes = read(&path)?;
        let actual_size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if actual_size != declared.size_bytes {
            return Err(ArtifactLoadError {
                path: display(&path),
                message: format!(
                    "size mismatch: manifest={}, actual={actual_size}",
                    declared.size_bytes
                ),
            });
        }
        let actual_hash = sha256_hex(&bytes);
        if actual_hash != declared.sha256 {
            return Err(ArtifactLoadError {
                path: display(&path),
                message: format!(
                    "SHA-256 mismatch: manifest={}, actual={actual_hash}",
                    declared.sha256
                ),
            });
        }
        verified.insert(declared.path.clone(), bytes);
    }
    Ok((manifest, verified))
}

fn reject_unlisted_files(
    artifact_directory: &Path,
    manifest: &ArtifactManifest,
) -> Result<(), ArtifactLoadError> {
    let declared: std::collections::HashSet<_> = manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    inspect_directory(artifact_directory, artifact_directory, "", &declared)
}

fn inspect_directory(
    artifact_directory: &Path,
    directory: &Path,
    relative_directory: &str,
    declared: &std::collections::HashSet<&str>,
) -> Result<(), ArtifactLoadError> {
    let entries = fs::read_dir(directory).map_err(|error| ArtifactLoadError {
        path: display(directory),
        message: error.to_string(),
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| ArtifactLoadError {
            path: display(directory),
            message: error.to_string(),
        })?;
        let file_type = entry.file_type().map_err(|error| ArtifactLoadError {
            path: display(&entry.path()),
            message: error.to_string(),
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(ArtifactLoadError {
                path: display(&entry.path()),
                message: "artifact filename must be valid UTF-8".into(),
            });
        };
        let relative = if relative_directory.is_empty() {
            name.to_owned()
        } else {
            format!("{relative_directory}/{name}")
        };
        if file_type.is_dir() {
            let prefix = format!("{relative}/");
            if !declared.iter().any(|path| path.starts_with(&prefix)) {
                return Err(ArtifactLoadError {
                    path: display(&entry.path()),
                    message: "directory does not contain a file listed in manifest.json".into(),
                });
            }
            inspect_directory(artifact_directory, &entry.path(), &relative, declared)?;
        } else if !file_type.is_file() {
            return Err(ArtifactLoadError {
                path: display(&entry.path()),
                message: "artifact directory must not contain special files".into(),
            });
        } else if !(relative == "manifest.json" && directory == artifact_directory)
            && !declared.contains(relative.as_str())
        {
            return Err(ArtifactLoadError {
                path: display(&entry.path()),
                message: "file is not listed in manifest.json".into(),
            });
        }
    }
    Ok(())
}

fn read(path: &Path) -> Result<Vec<u8>, ArtifactLoadError> {
    fs::read(path).map_err(|error| ArtifactLoadError {
        path: display(path),
        message: error.to_string(),
    })
}

fn parse_json<T: serde::de::DeserializeOwned>(
    path: &Path,
    bytes: &[u8],
) -> Result<T, ArtifactLoadError> {
    serde_json::from_slice(bytes).map_err(|error| ArtifactLoadError {
        path: display(path),
        message: error.to_string(),
    })
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
