use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Instant,
};

use pcdiag_core::{
    ArtifactFile, ArtifactManifest, ArtifactStatus, ArtifactType, CollectorStatus, ToolInfo,
    display_id, sha256_hex,
};
use pcdiag_windows::WindowsUpdateCollectionOptions;

use crate::{collector_process::CollectorTimeouts, interrupt};

const MANIFEST_SCHEMA_VERSION: &str = "1.0";
const ARTIFACT_SCHEMA_VERSION: &str = "2.0";

pub fn collect_to_bundle(
    output_root: &Path,
    windows_update_options: WindowsUpdateCollectionOptions,
    collector_timeouts: &CollectorTimeouts,
) -> Result<PathBuf, BundleError> {
    interrupt::check("collect")?;
    let started = Instant::now();
    let started_at = platform::utc_timestamp()?;
    let observed_utc_offset_minutes = platform::utc_offset_minutes()?;
    let local_directory_timestamp = platform::local_directory_timestamp()?;
    fs::create_dir_all(output_root)?;
    for _ in 0..16 {
        let session_id = platform::uuid_v4()?;
        let artifact_id = platform::uuid_v4()?;
        if session_id == artifact_id {
            continue;
        }
        let short_id = display_id(&session_id).expect("generated UUIDv4 must be valid");
        let directory_name = format!("pcdiag-{local_directory_timestamp}-{short_id}");
        let final_directory = output_root.join(&directory_name);
        let incomplete_directory = output_root.join(format!("{directory_name}.incomplete"));
        if final_directory.exists() || incomplete_directory.exists() {
            continue;
        }
        fs::create_dir(&incomplete_directory)?;
        interrupt::check_with_log("collect", &incomplete_directory)?;
        let result = match crate::collector_process::collect_all(
            configured_event_log_days()?,
            windows_update_options,
            collector_timeouts,
            interrupt::is_requested,
        ) {
            Ok(result) => result,
            Err(crate::collector_process::CollectionRunError::Cancelled) => {
                return Err(interrupt::check_with_log("collect", &incomplete_directory)
                    .unwrap_err()
                    .into());
            }
            Err(crate::collector_process::CollectionRunError::Protocol(message)) => {
                return Err(BundleError::CollectorProtocol(message));
            }
        };
        interrupt::check_with_log("collect", &incomplete_directory)?;
        result.collection.validate_with_status(&result.status)?;
        interrupt::check_with_log("collect", &incomplete_directory)?;
        let completed_at = platform::utc_timestamp()?;
        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        return write_bundle(
            &incomplete_directory,
            &final_directory,
            result,
            ManifestTiming {
                session_id,
                artifact_id,
                started_at,
                completed_at,
                observed_utc_offset_minutes,
                duration_ms,
            },
        );
    }
    Err(BundleError::Collision)
}

fn configured_event_log_days() -> Result<u32, BundleError> {
    let Some(value) = std::env::var_os("PCDIAG_EVENT_LOG_DAYS") else {
        return Ok(30);
    };
    let value = value
        .to_str()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|days| (1..=3_650).contains(days))
        .ok_or_else(|| {
            BundleError::Configuration(
                "PCDIAG_EVENT_LOG_DAYSには1から3650の日数を指定してください".into(),
            )
        })?;
    Ok(value)
}

struct ManifestTiming {
    session_id: String,
    artifact_id: String,
    started_at: String,
    completed_at: String,
    observed_utc_offset_minutes: i32,
    duration_ms: u64,
}

fn write_bundle(
    incomplete_directory: &Path,
    final_directory: &Path,
    result: pcdiag_windows::CompleteCollectionResult,
    timing: ManifestTiming,
) -> Result<PathBuf, BundleError> {
    interrupt::check_with_log("collect", incomplete_directory)?;
    let collection_directory = incomplete_directory.join("collection");
    fs::create_dir(&collection_directory)?;
    let collection_bytes = pretty_json(&result.collection)?;
    let status_bytes = pretty_json(&result.status)?;
    interrupt::check_with_log("collect", incomplete_directory)?;
    write_new(
        &collection_directory.join("collection.json"),
        &collection_bytes,
    )?;
    interrupt::check_with_log("collect", incomplete_directory)?;
    write_new(&collection_directory.join("status.json"), &status_bytes)?;

    let artifact_status = if result
        .status
        .collectors
        .iter()
        .all(|collector| collector.status == CollectorStatus::Success)
    {
        ArtifactStatus::Complete
    } else {
        ArtifactStatus::Partial
    };
    let manifest = ArtifactManifest {
        manifest_schema_version: MANIFEST_SCHEMA_VERSION.into(),
        artifact_schema_version: ARTIFACT_SCHEMA_VERSION.into(),
        session_id: timing.session_id,
        artifact_id: timing.artifact_id,
        artifact_type: ArtifactType::Collection,
        status: artifact_status,
        started_at: timing.started_at,
        completed_at: timing.completed_at,
        observed_utc_offset_minutes: timing.observed_utc_offset_minutes,
        duration_ms: timing.duration_ms,
        tool: ToolInfo {
            name: "pcdiag".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        inputs: vec![],
        files: vec![
            artifact_file("collection.json", &collection_bytes),
            artifact_file("status.json", &status_bytes),
        ],
    };
    manifest.validate()?;
    let manifest_bytes = pretty_json(&manifest)?;
    interrupt::check_with_log("collect", incomplete_directory)?;
    write_new(&collection_directory.join("manifest.json"), &manifest_bytes)?;
    interrupt::check_with_log("collect", incomplete_directory)?;
    fs::rename(incomplete_directory, final_directory)?;
    Ok(final_directory.to_owned())
}

pub(crate) fn pretty_json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

pub(crate) fn artifact_file(path: &str, bytes: &[u8]) -> ArtifactFile {
    ArtifactFile {
        path: path.into(),
        media_type: "application/json".into(),
        size_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        sha256: sha256_hex(bytes),
    }
}

#[derive(Debug)]
pub enum BundleError {
    Interrupted(interrupt::Interrupted),
    Io(io::Error),
    Json(serde_json::Error),
    CollectionValidation(pcdiag_core::ValidationErrors),
    ManifestValidation(pcdiag_core::ManifestValidationErrors),
    Platform(String),
    Configuration(String),
    CollectorProtocol(String),
    Collision,
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Interrupted(error) => error.fmt(formatter),
            Self::Io(error) => write!(formatter, "ファイル操作に失敗しました: {error}"),
            Self::Json(error) => write!(formatter, "JSON生成に失敗しました: {error}"),
            Self::CollectionValidation(error) => write!(formatter, "収集結果が不正です: {error}"),
            Self::ManifestValidation(error) => write!(formatter, "マニフェストが不正です: {error}"),
            Self::Platform(message) => formatter.write_str(message),
            Self::Configuration(message) => formatter.write_str(message),
            Self::CollectorProtocol(message) => {
                write!(formatter, "コレクター結果を統合できませんでした: {message}")
            }
            Self::Collision => {
                formatter.write_str("一意なセッションディレクトリを作成できませんでした")
            }
        }
    }
}

impl std::error::Error for BundleError {}

impl BundleError {
    pub(crate) fn is_interrupted(&self) -> bool {
        matches!(self, Self::Interrupted(_))
    }
}

impl From<interrupt::Interrupted> for BundleError {
    fn from(value: interrupt::Interrupted) -> Self {
        Self::Interrupted(value)
    }
}

impl From<io::Error> for BundleError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for BundleError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<pcdiag_core::ValidationErrors> for BundleError {
    fn from(value: pcdiag_core::ValidationErrors) -> Self {
        Self::CollectionValidation(value)
    }
}

impl From<pcdiag_core::ManifestValidationErrors> for BundleError {
    fn from(value: pcdiag_core::ManifestValidationErrors) -> Self {
        Self::ManifestValidation(value)
    }
}

#[cfg(windows)]
pub(crate) mod platform {
    use windows_sys::Win32::{
        Foundation::SYSTEMTIME,
        Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom},
        System::{
            SystemInformation::{GetLocalTime, GetSystemTime},
            SystemServices::{TIME_ZONE_ID_DAYLIGHT, TIME_ZONE_ID_STANDARD},
            Time::{GetTimeZoneInformation, TIME_ZONE_ID_INVALID, TIME_ZONE_INFORMATION},
        },
    };

    use super::BundleError;

    pub(crate) fn uuid_v4() -> Result<String, BundleError> {
        let mut bytes = [0u8; 16];
        // SAFETY: bytes is a valid writable buffer and the system-preferred RNG needs no handle.
        let status = unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        };
        if status < 0 {
            return Err(BundleError::Platform(format!(
                "UUIDv4用の乱数を生成できませんでした: {status}"
            )));
        }
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Ok(format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[0],
            bytes[1],
            bytes[2],
            bytes[3],
            bytes[4],
            bytes[5],
            bytes[6],
            bytes[7],
            bytes[8],
            bytes[9],
            bytes[10],
            bytes[11],
            bytes[12],
            bytes[13],
            bytes[14],
            bytes[15]
        ))
    }

    pub(crate) fn utc_timestamp() -> Result<String, BundleError> {
        let mut time = SYSTEMTIME::default();
        // SAFETY: time points to writable SYSTEMTIME storage.
        unsafe { GetSystemTime(&mut time) };
        Ok(format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            time.wYear,
            time.wMonth,
            time.wDay,
            time.wHour,
            time.wMinute,
            time.wSecond,
            time.wMilliseconds
        ))
    }

    pub(crate) fn local_directory_timestamp() -> Result<String, BundleError> {
        let mut time = SYSTEMTIME::default();
        // SAFETY: time points to writable SYSTEMTIME storage.
        unsafe { GetLocalTime(&mut time) };
        Ok(format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
        ))
    }

    pub(crate) fn utc_offset_minutes() -> Result<i32, BundleError> {
        let mut information = TIME_ZONE_INFORMATION::default();
        // SAFETY: information points to writable TIME_ZONE_INFORMATION storage.
        let state = unsafe { GetTimeZoneInformation(&mut information) };
        if state == TIME_ZONE_ID_INVALID {
            return Err(BundleError::Platform(
                "UTCオフセットを取得できませんでした".into(),
            ));
        }
        let extra_bias = if state == TIME_ZONE_ID_DAYLIGHT {
            information.DaylightBias
        } else if state == TIME_ZONE_ID_STANDARD {
            information.StandardBias
        } else {
            0
        };
        Ok(-(information.Bias + extra_bias))
    }
}

#[cfg(not(windows))]
pub(crate) mod platform {
    use super::BundleError;

    fn unsupported<T>() -> Result<T, BundleError> {
        Err(BundleError::Platform(
            "収集バンドル生成はWindowsでのみ利用できます".into(),
        ))
    }

    pub(crate) fn uuid_v4() -> Result<String, BundleError> {
        unsupported()
    }
    pub(crate) fn utc_timestamp() -> Result<String, BundleError> {
        unsupported()
    }
    pub(crate) fn local_directory_timestamp() -> Result<String, BundleError> {
        unsupported()
    }
    pub(crate) fn utc_offset_minutes() -> Result<i32, BundleError> {
        unsupported()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_standard_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn writes_files_manifest_and_finalizes_directory() {
        let root = std::env::temp_dir().join(format!("pcdiag-bundle-test-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir(&root).unwrap();
        let incomplete = root.join("session.incomplete");
        let final_directory = root.join("session");
        fs::create_dir(&incomplete).unwrap();
        let result = pcdiag_windows::collect_all(30, WindowsUpdateCollectionOptions::default());
        let written = write_bundle(
            &incomplete,
            &final_directory,
            result,
            ManifestTiming {
                session_id: "a3f17c92-d604-4be8-9ea7-6ab7b92e41c5".into(),
                artifact_id: "831d1074-1145-4a66-bfa2-169903866adb".into(),
                started_at: "2026-07-18T01:30:15.000Z".into(),
                completed_at: "2026-07-18T01:30:28.000Z".into(),
                observed_utc_offset_minutes: 540,
                duration_ms: 13_000,
            },
        )
        .unwrap();

        assert_eq!(written, final_directory);
        assert!(!incomplete.exists());
        let collection_directory = final_directory.join("collection");
        let manifest: ArtifactManifest =
            serde_json::from_slice(&fs::read(collection_directory.join("manifest.json")).unwrap())
                .unwrap();
        manifest.validate().unwrap();
        for file in &manifest.files {
            let bytes = fs::read(collection_directory.join(&file.path)).unwrap();
            assert_eq!(file.size_bytes, bytes.len() as u64);
            assert_eq!(file.sha256, sha256_hex(&bytes));
        }
        fs::remove_dir_all(root).unwrap();
    }
}
