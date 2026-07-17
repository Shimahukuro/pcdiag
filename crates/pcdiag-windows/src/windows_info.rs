use std::time::Instant;

use pcdiag_core::{
    BootMode, CollectionMessage, CollectorName, CollectorResult, CollectorStatus,
    FieldCollectionResult, FieldCollectionStatus, SystemArchitecture, WindowsCollection,
};

const VERSION_PATHS: [&str; 3] = [
    "/windows/edition",
    "/windows/version",
    "/windows/build_number",
];
const ARCHITECTURE_PATH: &str = "/windows/architecture";
const UPTIME_PATHS: [&str; 2] = ["/windows/booted_at", "/windows/uptime_ms"];
const BOOT_MODE_PATH: &str = "/windows/boot_mode";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsCollectionResult {
    pub collection: WindowsCollection,
    pub status: CollectorResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VersionSnapshot {
    edition: Option<String>,
    version: String,
    build_number: u32,
    edition_failure: Option<CollectionFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UptimeSnapshot {
    booted_at: String,
    uptime_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CollectionFailure {
    code: &'static str,
    native_code: Option<i64>,
    message: &'static str,
    field_status: FieldCollectionStatus,
}

/// Collects facts about the running Windows installation.
pub fn collect_windows_info() -> WindowsCollectionResult {
    let started = Instant::now();
    build_result(
        platform::version_snapshot(),
        platform::architecture(),
        platform::uptime_snapshot(),
        platform::boot_mode(),
        elapsed_ms(started),
    )
}

fn build_result(
    version: Result<VersionSnapshot, CollectionFailure>,
    architecture: Result<SystemArchitecture, CollectionFailure>,
    uptime: Result<UptimeSnapshot, CollectionFailure>,
    boot_mode: Result<BootMode, CollectionFailure>,
    duration_ms: u64,
) -> WindowsCollectionResult {
    let mut messages = Vec::new();
    let mut fields = Vec::new();

    let (edition, version, build_number) = match version {
        Ok(snapshot) => {
            if let Some(failure) = &snapshot.edition_failure {
                record_failure(&mut messages, &mut fields, &["/windows/edition"], failure);
            }
            (
                snapshot.edition,
                Some(snapshot.version),
                Some(snapshot.build_number),
            )
        }
        Err(failure) => {
            record_failure(&mut messages, &mut fields, &VERSION_PATHS, &failure);
            (None, None, None)
        }
    };
    let architecture = match architecture {
        Ok(value) => Some(value),
        Err(failure) => {
            record_failure(&mut messages, &mut fields, &[ARCHITECTURE_PATH], &failure);
            None
        }
    };
    let (booted_at, uptime_ms) = match uptime {
        Ok(snapshot) => (Some(snapshot.booted_at), Some(snapshot.uptime_ms)),
        Err(failure) => {
            record_failure(&mut messages, &mut fields, &UPTIME_PATHS, &failure);
            (None, None)
        }
    };
    let boot_mode = match boot_mode {
        Ok(value) => Some(value),
        Err(failure) => {
            record_failure(&mut messages, &mut fields, &[BOOT_MODE_PATH], &failure);
            None
        }
    };

    let failed_fields = fields.len();
    let total_fields = VERSION_PATHS.len() + 1 + UPTIME_PATHS.len() + 1;
    let status = if failed_fields == 0 {
        CollectorStatus::Success
    } else if failed_fields == total_fields {
        fields.clear();
        CollectorStatus::Failed
    } else {
        CollectorStatus::Partial
    };

    WindowsCollectionResult {
        collection: WindowsCollection {
            edition,
            version,
            build_number,
            architecture,
            booted_at,
            uptime_ms,
            boot_mode,
        },
        status: CollectorResult {
            name: CollectorName::Windows,
            status,
            duration_ms,
            messages,
            fields,
        },
    }
}

fn record_failure(
    messages: &mut Vec<CollectionMessage>,
    fields: &mut Vec<FieldCollectionResult>,
    paths: &[&str],
    failure: &CollectionFailure,
) {
    messages.push(CollectionMessage {
        code: failure.code.into(),
        native_code: failure.native_code,
        message: Some(failure.message.into()),
    });
    fields.extend(paths.iter().map(|path| FieldCollectionResult {
        path: (*path).into(),
        status: failure.field_status,
        code: failure.code.into(),
        native_code: failure.native_code,
    }));
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(windows)]
mod platform {
    use std::{io, mem::size_of};

    use windows_sys::Win32::{
        Foundation::{FILETIME, SYSTEMTIME},
        System::{
            SystemInformation::{
                FirmwareTypeBios, FirmwareTypeUefi, FirmwareTypeUnknown, GetFirmwareType,
                GetNativeSystemInfo, GetProductInfo, GetSystemTimePreciseAsFileTime,
                GetTickCount64, OSVERSIONINFOW, PROCESSOR_ARCHITECTURE_AMD64,
                PROCESSOR_ARCHITECTURE_ARM, PROCESSOR_ARCHITECTURE_ARM64,
                PROCESSOR_ARCHITECTURE_INTEL, SYSTEM_INFO,
            },
            Time::FileTimeToSystemTime,
        },
    };

    use super::{
        BootMode, CollectionFailure, FieldCollectionStatus, SystemArchitecture, UptimeSnapshot,
        VersionSnapshot,
    };

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn RtlGetVersion(version_information: *mut OSVERSIONINFOW) -> i32;
    }

    pub(super) fn version_snapshot() -> Result<VersionSnapshot, CollectionFailure> {
        let mut info = OSVERSIONINFOW {
            dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
            ..Default::default()
        };
        // SAFETY: `info` is writable and its size field is initialized.
        let status = unsafe { RtlGetVersion(&mut info) };
        if status < 0 {
            return Err(failure(
                "windows_version_failed",
                Some(i64::from(status)),
                "Windowsのバージョン情報を取得できませんでした",
            ));
        }

        let mut product = 0;
        // SAFETY: `product` points to writable storage and version values came
        // from RtlGetVersion for the running system.
        let edition_failure = if unsafe {
            GetProductInfo(info.dwMajorVersion, info.dwMinorVersion, 0, 0, &mut product)
        } == 0
        {
            Some(last_error_failure(
                "windows_edition_failed",
                "Windowsエディションを取得できませんでした",
            ))
        } else {
            None
        };

        Ok(VersionSnapshot {
            edition: edition_failure.is_none().then(|| edition_name(product)),
            version: format!(
                "{}.{}.{}",
                info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber
            ),
            build_number: info.dwBuildNumber,
            edition_failure,
        })
    }

    pub(super) fn architecture() -> Result<SystemArchitecture, CollectionFailure> {
        let mut info = SYSTEM_INFO::default();
        // SAFETY: `info` points to writable SYSTEM_INFO storage.
        unsafe { GetNativeSystemInfo(&mut info) };
        // SAFETY: GetNativeSystemInfo initializes this union member.
        let architecture = unsafe { info.Anonymous.Anonymous.wProcessorArchitecture };
        Ok(match architecture {
            PROCESSOR_ARCHITECTURE_INTEL => SystemArchitecture::X86,
            PROCESSOR_ARCHITECTURE_AMD64 => SystemArchitecture::X86_64,
            PROCESSOR_ARCHITECTURE_ARM => SystemArchitecture::Arm,
            PROCESSOR_ARCHITECTURE_ARM64 => SystemArchitecture::Arm64,
            _ => SystemArchitecture::Unknown,
        })
    }

    pub(super) fn uptime_snapshot() -> Result<UptimeSnapshot, CollectionFailure> {
        // SAFETY: GetTickCount64 has no preconditions.
        let uptime_ms = unsafe { GetTickCount64() };
        let mut now = FILETIME::default();
        // SAFETY: `now` points to writable FILETIME storage.
        unsafe { GetSystemTimePreciseAsFileTime(&mut now) };
        let now_ticks = (u64::from(now.dwHighDateTime) << 32) | u64::from(now.dwLowDateTime);
        let boot_ticks = now_ticks
            .checked_sub(uptime_ms.saturating_mul(10_000))
            .ok_or_else(|| {
                failure(
                    "boot_time_invalid",
                    None,
                    "Windowsの時刻と稼働時間から起動時刻を算出できませんでした",
                )
            })?;
        let boot_filetime = FILETIME {
            dwLowDateTime: boot_ticks as u32,
            dwHighDateTime: (boot_ticks >> 32) as u32,
        };
        let mut boot = SYSTEMTIME::default();
        // SAFETY: both pointers refer to initialized readable/writable structures.
        if unsafe { FileTimeToSystemTime(&boot_filetime, &mut boot) } == 0 {
            return Err(last_error_failure(
                "boot_time_conversion_failed",
                "Windowsの起動時刻をUTC日時へ変換できませんでした",
            ));
        }

        Ok(UptimeSnapshot {
            booted_at: format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
                boot.wYear,
                boot.wMonth,
                boot.wDay,
                boot.wHour,
                boot.wMinute,
                boot.wSecond,
                boot.wMilliseconds
            ),
            uptime_ms,
        })
    }

    pub(super) fn boot_mode() -> Result<BootMode, CollectionFailure> {
        let mut firmware = FirmwareTypeUnknown;
        // SAFETY: `firmware` points to writable FIRMWARE_TYPE storage.
        if unsafe { GetFirmwareType(&mut firmware) } == 0 {
            return Err(last_error_failure(
                "firmware_type_failed",
                "WindowsからBIOS・UEFI起動方式を取得できませんでした",
            ));
        }
        Ok(if firmware == FirmwareTypeBios {
            BootMode::Bios
        } else if firmware == FirmwareTypeUefi {
            BootMode::Uefi
        } else {
            BootMode::Unknown
        })
    }

    fn edition_name(product: u32) -> String {
        match product {
            4 => "Enterprise",
            6 => "Business",
            7 => "Server Standard",
            8 => "Server Datacenter",
            48 => "Professional",
            49 => "Professional N",
            98 => "Home N",
            99 => "Home China",
            100 => "Home Single Language",
            101 => "Home",
            121 => "Education",
            122 => "Education N",
            125 => "Enterprise LTSC",
            126 => "Enterprise LTSC N",
            161 => "Professional for Workstations",
            162 => "Professional for Workstations N",
            164 => "Professional Education",
            165 => "Professional Education N",
            _ => return format!("product_{product}"),
        }
        .into()
    }

    fn last_error_failure(code: &'static str, message: &'static str) -> CollectionFailure {
        failure(
            code,
            io::Error::last_os_error().raw_os_error().map(i64::from),
            message,
        )
    }

    fn failure(
        code: &'static str,
        native_code: Option<i64>,
        message: &'static str,
    ) -> CollectionFailure {
        CollectionFailure {
            code,
            native_code,
            message,
            field_status: FieldCollectionStatus::Failed,
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{
        BootMode, CollectionFailure, FieldCollectionStatus, SystemArchitecture, UptimeSnapshot,
        VersionSnapshot,
    };

    pub(super) fn version_snapshot() -> Result<VersionSnapshot, CollectionFailure> {
        Err(unsupported())
    }

    pub(super) fn architecture() -> Result<SystemArchitecture, CollectionFailure> {
        Err(unsupported())
    }

    pub(super) fn uptime_snapshot() -> Result<UptimeSnapshot, CollectionFailure> {
        Err(unsupported())
    }

    pub(super) fn boot_mode() -> Result<BootMode, CollectionFailure> {
        Err(unsupported())
    }

    fn unsupported() -> CollectionFailure {
        CollectionFailure {
            code: "platform_not_supported",
            native_code: None,
            message: "Windows以外の環境ではWindows基本情報を収集できません",
            field_status: FieldCollectionStatus::Unsupported,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_all_windows_information() {
        let result = build_result(
            Ok(VersionSnapshot {
                edition: Some("Professional".into()),
                version: "10.0.26100".into(),
                build_number: 26_100,
                edition_failure: None,
            }),
            Ok(SystemArchitecture::X86_64),
            Ok(UptimeSnapshot {
                booted_at: "2026-07-17T00:00:00.000Z".into(),
                uptime_ms: 123_000,
            }),
            Ok(BootMode::Uefi),
            2,
        );

        assert_eq!(result.status.status, CollectorStatus::Success);
        assert_eq!(result.collection.edition.as_deref(), Some("Professional"));
        assert_eq!(result.collection.build_number, Some(26_100));
        assert_eq!(result.collection.boot_mode, Some(BootMode::Uefi));
    }

    #[test]
    fn preserves_successful_groups_when_one_source_fails() {
        let result = build_result(
            Err(CollectionFailure {
                code: "windows_version_failed",
                native_code: Some(-1),
                message: "failed",
                field_status: FieldCollectionStatus::Failed,
            }),
            Ok(SystemArchitecture::Arm64),
            Ok(UptimeSnapshot {
                booted_at: "2026-07-17T00:00:00.000Z".into(),
                uptime_ms: 1,
            }),
            Ok(BootMode::Uefi),
            1,
        );

        assert_eq!(result.status.status, CollectorStatus::Partial);
        assert_eq!(result.status.fields.len(), 3);
        assert_eq!(result.collection.version, None);
        assert_eq!(
            result.collection.architecture,
            Some(SystemArchitecture::Arm64)
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_collection_returns_failed() {
        let result = collect_windows_info();

        assert_eq!(result.status.status, CollectorStatus::Failed);
        assert!(result.collection.version.is_none());
    }
}
