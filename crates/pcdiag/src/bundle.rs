use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Instant,
};

use pcdiag_core::{
    ArtifactFile, ArtifactManifest, ArtifactStatus, ArtifactType, CollectorStatus, ToolInfo,
    display_id,
};
use pcdiag_windows::collect_all;

const MANIFEST_SCHEMA_VERSION: &str = "1.0";
const ARTIFACT_SCHEMA_VERSION: &str = "1.0";

pub fn collect_to_bundle(output_root: &Path) -> Result<PathBuf, BundleError> {
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
        let result = collect_all();
        result.collection.validate_with_status(&result.status)?;
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
    let collection_directory = incomplete_directory.join("collection");
    fs::create_dir(&collection_directory)?;
    let collection_bytes = pretty_json(&result.collection)?;
    let status_bytes = pretty_json(&result.status)?;
    write_new(
        &collection_directory.join("collection.json"),
        &collection_bytes,
    )?;
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
    write_new(&collection_directory.join("manifest.json"), &manifest_bytes)?;
    fs::rename(incomplete_directory, final_directory)?;
    Ok(final_directory.to_owned())
}

fn pretty_json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn artifact_file(path: &str, bytes: &[u8]) -> ArtifactFile {
    ArtifactFile {
        path: path.into(),
        media_type: "application/json".into(),
        size_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        sha256: sha256_hex(bytes),
    }
}

fn sha256_hex(input: &[u8]) -> String {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());
    let mut state = INITIAL;
    for chunk in padded.chunks_exact(64) {
        let mut words = [0u32; 64];
        for (index, word) in words[..16].iter_mut().enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes(chunk[offset..offset + 4].try_into().unwrap());
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ (!e & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
    state.iter().map(|value| format!("{value:08x}")).collect()
}

#[derive(Debug)]
pub enum BundleError {
    Io(io::Error),
    Json(serde_json::Error),
    CollectionValidation(pcdiag_core::ValidationErrors),
    ManifestValidation(pcdiag_core::ManifestValidationErrors),
    Platform(String),
    Collision,
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "ファイル操作に失敗しました: {error}"),
            Self::Json(error) => write!(formatter, "JSON生成に失敗しました: {error}"),
            Self::CollectionValidation(error) => write!(formatter, "収集結果が不正です: {error}"),
            Self::ManifestValidation(error) => write!(formatter, "マニフェストが不正です: {error}"),
            Self::Platform(message) => formatter.write_str(message),
            Self::Collision => {
                formatter.write_str("一意なセッションディレクトリを作成できませんでした")
            }
        }
    }
}

impl std::error::Error for BundleError {}

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
mod platform {
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

    pub(super) fn uuid_v4() -> Result<String, BundleError> {
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

    pub(super) fn utc_timestamp() -> Result<String, BundleError> {
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

    pub(super) fn local_directory_timestamp() -> Result<String, BundleError> {
        let mut time = SYSTEMTIME::default();
        // SAFETY: time points to writable SYSTEMTIME storage.
        unsafe { GetLocalTime(&mut time) };
        Ok(format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
        ))
    }

    pub(super) fn utc_offset_minutes() -> Result<i32, BundleError> {
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
mod platform {
    use super::BundleError;

    fn unsupported<T>() -> Result<T, BundleError> {
        Err(BundleError::Platform(
            "収集バンドル生成はWindowsでのみ利用できます".into(),
        ))
    }

    pub(super) fn uuid_v4() -> Result<String, BundleError> {
        unsupported()
    }
    pub(super) fn utc_timestamp() -> Result<String, BundleError> {
        unsupported()
    }
    pub(super) fn local_directory_timestamp() -> Result<String, BundleError> {
        unsupported()
    }
    pub(super) fn utc_offset_minutes() -> Result<i32, BundleError> {
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
        let result = collect_all();
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
