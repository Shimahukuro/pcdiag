use std::{collections::HashSet, fmt};

use serde::{Deserialize, Serialize};

pub const CURRENT_MANIFEST_SCHEMA_VERSION: &str = "1.0";
pub const CURRENT_ARTIFACT_SCHEMA_VERSION: &str = "2.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SchemaVersion {
    major: u64,
    minor: u64,
}

impl SchemaVersion {
    fn parse(value: &str) -> Option<Self> {
        let (major, minor) = value.split_once('.')?;
        if major.is_empty()
            || minor.is_empty()
            || minor.contains('.')
            || !canonical_component(major)
            || !canonical_component(minor)
        {
            return None;
        }
        Some(Self {
            major: major.parse().ok()?,
            minor: minor.parse().ok()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactManifest {
    pub manifest_schema_version: String,
    pub artifact_schema_version: String,
    pub session_id: String,
    pub artifact_id: String,
    pub artifact_type: ArtifactType,
    pub status: ArtifactStatus,
    pub started_at: String,
    pub completed_at: String,
    pub observed_utc_offset_minutes: i32,
    pub duration_ms: u64,
    pub tool: ToolInfo,
    pub inputs: Vec<ArtifactInput>,
    pub files: Vec<ArtifactFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    Collection,
    Diagnosis,
    Report,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactStatus {
    Complete,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactInput {
    pub artifact_id: String,
    pub artifact_type: ArtifactType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactFile {
    pub path: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestValidationError {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestValidationErrors(Vec<ManifestValidationError>);

impl ManifestValidationErrors {
    pub fn errors(&self) -> &[ManifestValidationError] {
        &self.0
    }
}

impl fmt::Display for ManifestValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} manifest validation error(s)", self.0.len())?;
        for error in &self.0 {
            write!(formatter, "; {}: {}", error.path, error.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for ManifestValidationErrors {}

impl ArtifactManifest {
    pub fn validate(&self) -> Result<(), ManifestValidationErrors> {
        let mut errors = Vec::new();
        validate_supported_schema_version(
            &mut errors,
            "/manifest_schema_version",
            &self.manifest_schema_version,
            CURRENT_MANIFEST_SCHEMA_VERSION,
        );
        validate_supported_schema_version(
            &mut errors,
            "/artifact_schema_version",
            &self.artifact_schema_version,
            CURRENT_ARTIFACT_SCHEMA_VERSION,
        );
        for (path, value) in [
            ("/session_id", self.session_id.as_str()),
            ("/artifact_id", self.artifact_id.as_str()),
        ] {
            if !is_uuid_v4(value) {
                push(&mut errors, path, "must be a canonical lowercase UUIDv4");
            }
        }
        if self.session_id == self.artifact_id {
            push(&mut errors, "/artifact_id", "must differ from session_id");
        }
        for (path, value) in [
            ("/started_at", self.started_at.as_str()),
            ("/completed_at", self.completed_at.as_str()),
        ] {
            if !is_utc_timestamp(value) {
                push(&mut errors, path, "must be a UTC RFC 3339 timestamp");
            }
        }
        if !(-1_440..=1_440).contains(&self.observed_utc_offset_minutes) {
            push(
                &mut errors,
                "/observed_utc_offset_minutes",
                "must be between -1440 and 1440",
            );
        }
        if self.tool.name != "pcdiag" {
            push(&mut errors, "/tool/name", "must be pcdiag");
        }
        if self.tool.version.is_empty() {
            push(&mut errors, "/tool/version", "must not be empty");
        }
        if self.artifact_type == ArtifactType::Collection && !self.inputs.is_empty() {
            push(
                &mut errors,
                "/inputs",
                "collection artifact must not have inputs",
            );
        }
        if self.artifact_type == ArtifactType::Diagnosis
            && (self.inputs.len() != 1 || self.inputs[0].artifact_type != ArtifactType::Collection)
        {
            push(
                &mut errors,
                "/inputs",
                "diagnosis artifact must have exactly one collection input",
            );
        }
        if self.artifact_type == ArtifactType::Report
            && (self.inputs.len() != 2
                || self
                    .inputs
                    .iter()
                    .filter(|input| input.artifact_type == ArtifactType::Collection)
                    .count()
                    != 1
                || self
                    .inputs
                    .iter()
                    .filter(|input| input.artifact_type == ArtifactType::Diagnosis)
                    .count()
                    != 1)
        {
            push(
                &mut errors,
                "/inputs",
                "report artifact must have exactly one collection and one diagnosis input",
            );
        }
        for (index, input) in self.inputs.iter().enumerate() {
            if !is_uuid_v4(&input.artifact_id) {
                push(
                    &mut errors,
                    format!("/inputs/{index}/artifact_id"),
                    "must be a canonical lowercase UUIDv4",
                );
            }
        }
        let mut paths = HashSet::new();
        for (index, file) in self.files.iter().enumerate() {
            let base = format!("/files/{index}");
            if !is_safe_relative_path(&file.path) {
                push(
                    &mut errors,
                    format!("{base}/path"),
                    "must be a safe relative path",
                );
            }
            if file.path == "manifest.json" {
                push(
                    &mut errors,
                    format!("{base}/path"),
                    "manifest.json must not list itself",
                );
            }
            if !paths.insert(&file.path) {
                push(&mut errors, format!("{base}/path"), "must be unique");
            }
            if file.media_type.is_empty() {
                push(
                    &mut errors,
                    format!("{base}/media_type"),
                    "must not be empty",
                );
            }
            if !is_lower_hex_sha256(&file.sha256) {
                push(
                    &mut errors,
                    format!("{base}/sha256"),
                    "must be 64 lowercase hexadecimal characters",
                );
            }
        }
        if self.artifact_type == ArtifactType::Collection {
            for required in ["collection.json", "status.json"] {
                if !self.files.iter().any(|file| file.path == required) {
                    push(&mut errors, "/files", format!("must contain {required}"));
                }
            }
        }
        if self.artifact_type == ArtifactType::Diagnosis
            && !self.files.iter().any(|file| file.path == "diagnosis.json")
        {
            push(&mut errors, "/files", "must contain diagnosis.json");
        }
        if self.artifact_type == ArtifactType::Report
            && !self.files.iter().any(|file| file.path == "report.html")
        {
            push(&mut errors, "/files", "must contain report.html");
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ManifestValidationErrors(errors))
        }
    }
}

pub(crate) fn validate_artifact_version_dependency(
    input: &str,
    derived: &str,
) -> Result<(), String> {
    let input_version = SchemaVersion::parse(input)
        .ok_or_else(|| format!("input artifact schema version {input:?} is malformed"))?;
    let derived_version = SchemaVersion::parse(derived)
        .ok_or_else(|| format!("derived artifact schema version {derived:?} is malformed"))?;
    if input_version.major != derived_version.major {
        return Err(format!(
            "input artifact schema version {input:?} and derived artifact schema version {derived:?} have different major versions"
        ));
    }
    if input_version.minor > derived_version.minor {
        return Err(format!(
            "input artifact schema version {input:?} is newer than derived artifact schema version {derived:?}"
        ));
    }
    Ok(())
}

fn validate_supported_schema_version(
    errors: &mut Vec<ManifestValidationError>,
    path: &str,
    value: &str,
    current: &str,
) {
    let Some(version) = SchemaVersion::parse(value) else {
        push(
            errors,
            path,
            format!("must use canonical MAJOR.MINOR format (for example {current}); got {value:?}"),
        );
        return;
    };
    let current_version =
        SchemaVersion::parse(current).expect("current schema version constant must be valid");
    if version.major != current_version.major || version.minor > current_version.minor {
        push(
            errors,
            path,
            format!(
                "{value:?} is unsupported; supported versions are {}.0 through {current}",
                current_version.major
            ),
        );
    }
}

fn canonical_component(value: &str) -> bool {
    value.bytes().all(|byte| byte.is_ascii_digit()) && (value == "0" || !value.starts_with('0'))
}

pub fn is_uuid_v4(value: &str) -> bool {
    if value.len() != 36 {
        return false;
    }
    let bytes = value.as_bytes();
    if bytes[8] != b'-' || bytes[13] != b'-' || bytes[18] != b'-' || bytes[23] != b'-' {
        return false;
    }
    if bytes[14] != b'4' || !matches!(bytes[19], b'8' | b'9' | b'a' | b'b') {
        return false;
    }
    bytes.iter().enumerate().all(|(index, byte)| {
        matches!(index, 8 | 13 | 18 | 23) || byte.is_ascii_digit() || (b'a'..=b'f').contains(byte)
    })
}

pub fn display_id(uuid: &str) -> Option<String> {
    is_uuid_v4(uuid).then(|| {
        uuid.chars()
            .filter(|character| *character != '-')
            .take(12)
            .collect()
    })
}

fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_utc_timestamp(value: &str) -> bool {
    value.len() >= 20
        && value.ends_with('Z')
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
        && value.as_bytes().get(10) == Some(&b'T')
        && value.as_bytes().get(13) == Some(&b':')
        && value.as_bytes().get(16) == Some(&b':')
}

fn push(
    errors: &mut Vec<ManifestValidationError>,
    path: impl Into<String>,
    message: impl Into<String>,
) {
    errors.push(ManifestValidationError {
        path: path.into(),
        message: message.into(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> ArtifactManifest {
        ArtifactManifest {
            manifest_schema_version: "1.0".into(),
            artifact_schema_version: "2.0".into(),
            session_id: "a3f17c92-d604-4be8-9ea7-6ab7b92e41c5".into(),
            artifact_id: "831d1074-1145-4a66-bfa2-169903866adb".into(),
            artifact_type: ArtifactType::Collection,
            status: ArtifactStatus::Complete,
            started_at: "2026-07-18T01:30:15Z".into(),
            completed_at: "2026-07-18T01:30:28Z".into(),
            observed_utc_offset_minutes: 540,
            duration_ms: 13_254,
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
                    sha256: "a".repeat(64),
                },
            ],
        }
    }

    #[test]
    fn validates_collection_manifest() {
        manifest().validate().unwrap();
    }

    #[test]
    fn rejects_artifact_schema_version_1() {
        let mut value = manifest();
        value.artifact_schema_version = "1.0".into();

        let errors = value.validate().unwrap_err();
        assert!(errors.errors().iter().any(|error| {
            error.path == "/artifact_schema_version"
                && error
                    .message
                    .contains("supported versions are 2.0 through 2.0")
        }));
    }

    #[test]
    fn rejects_future_minor_and_malformed_schema_versions_with_details() {
        let mut future = manifest();
        future.artifact_schema_version = "2.1".into();
        let error = future.validate().unwrap_err().to_string();
        assert!(error.contains("\"2.1\" is unsupported"));
        assert!(error.contains("2.0 through 2.0"));

        for malformed in ["2", "2.0.1", "v2.0", "02.0", "2.00", ""] {
            let mut value = manifest();
            value.artifact_schema_version = malformed.into();
            let error = value.validate().unwrap_err().to_string();
            assert!(error.contains("canonical MAJOR.MINOR"), "{malformed:?}");
            assert!(error.contains(malformed), "{malformed:?}");
        }
    }

    #[test]
    fn backward_compatibility_accepts_only_older_minors_of_the_same_major() {
        let mut errors = Vec::new();
        validate_supported_schema_version(&mut errors, "/version", "2.0", "2.3");
        validate_supported_schema_version(&mut errors, "/version", "2.3", "2.3");
        assert!(errors.is_empty());

        validate_supported_schema_version(&mut errors, "/version", "2.4", "2.3");
        validate_supported_schema_version(&mut errors, "/version", "1.9", "2.3");
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn validates_derived_artifact_version_order() {
        validate_artifact_version_dependency("2.0", "2.1").unwrap();
        validate_artifact_version_dependency("2.1", "2.1").unwrap();
        assert!(validate_artifact_version_dependency("2.1", "2.0").is_err());
        assert!(validate_artifact_version_dependency("1.0", "2.0").is_err());
    }

    #[test]
    fn ignores_unknown_fields_but_rejects_unknown_enums_and_missing_required_fields() {
        let mut value = serde_json::to_value(manifest()).unwrap();
        value["future_optional_field"] = serde_json::json!(true);
        serde_json::from_value::<ArtifactManifest>(value.clone()).unwrap();

        value["artifact_type"] = serde_json::json!("future_type");
        assert!(serde_json::from_value::<ArtifactManifest>(value).is_err());

        let mut missing = serde_json::to_value(manifest()).unwrap();
        missing.as_object_mut().unwrap().remove("artifact_id");
        assert!(serde_json::from_value::<ArtifactManifest>(missing).is_err());
    }

    #[test]
    fn rejects_unsafe_paths_and_non_v4_ids() {
        let mut value = manifest();
        value.session_id = "a3f17c92-d604-7be8-9ea7-6ab7b92e41c5".into();
        value.files[0].path = "../collection.json".into();

        let errors = value.validate().unwrap_err();
        assert_eq!(errors.errors().len(), 3);
    }

    #[test]
    fn derives_display_id() {
        assert_eq!(
            display_id("a3f17c92-d604-4be8-9ea7-6ab7b92e41c5").as_deref(),
            Some("a3f17c92d604")
        );
    }

    #[test]
    fn validates_report_manifest_inputs_and_file() {
        let mut value = manifest();
        value.artifact_type = ArtifactType::Report;
        value.inputs = vec![
            ArtifactInput {
                artifact_id: "211444ae-9a5c-4bf7-9349-80af85af3c04".into(),
                artifact_type: ArtifactType::Collection,
            },
            ArtifactInput {
                artifact_id: "43d39e67-c8f1-4c9b-a20e-a65dbba20295".into(),
                artifact_type: ArtifactType::Diagnosis,
            },
        ];
        value.files = vec![ArtifactFile {
            path: "report.html".into(),
            media_type: "text/html; charset=utf-8".into(),
            size_bytes: 1,
            sha256: "0".repeat(64),
        }];

        value.validate().unwrap();
        value.inputs.pop();
        assert!(value.validate().is_err());
    }
}
