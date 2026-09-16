use std::time::Instant;

use pcdiag_core::{
    CollectionMessage, CollectorName, CollectorResult, CollectorStatus, FieldCollectionResult,
    FieldCollectionStatus, MemoryIntegrity, MemoryIntegrityConfiguration,
    MemoryIntegrityRunningState, SecurityHealth, WindowsSecurityCollection,
};
use serde::Deserialize;

// WSC_SECURITY_PROVIDER flags; query separately because combined flags return only the worst health.
const PROVIDERS: [u32; 6] = [0x1, 0x2, 0x4, 0x10, 0x20, 0x40];
const PATHS: [&str; 8] = [
    "/windows_security/firewall",
    "/windows_security/automatic_updates",
    "/windows_security/antivirus",
    "/windows_security/internet_settings",
    "/windows_security/user_account_control",
    "/windows_security/security_center_service",
    "/windows_security/memory_integrity/configured",
    "/windows_security/memory_integrity/running",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsSecurityCollectionResult {
    pub collection: WindowsSecurityCollection,
    pub status: CollectorResult,
}

#[derive(Debug, Clone)]
struct Failure {
    status: FieldCollectionStatus,
    code: &'static str,
    native_code: Option<i64>,
}

impl Failure {
    fn new(status: FieldCollectionStatus, code: &'static str, native_code: Option<i64>) -> Self {
        Self {
            status,
            code,
            native_code,
        }
    }
}

#[derive(Debug, Deserialize)]
struct DeviceGuard {
    configured: Option<Vec<u32>>,
    running: Option<Vec<u32>>,
    error: Option<DeviceGuardError>,
}

#[derive(Debug, Deserialize)]
struct DeviceGuardError {
    status: FieldCollectionStatus,
    native_code: i64,
}

/// Reads aggregate security health and Win32_DeviceGuard without changing protection settings.
pub fn collect_windows_security() -> WindowsSecurityCollectionResult {
    let started = Instant::now();
    let health = PROVIDERS.map(platform::health);
    let device_guard = platform::device_guard();
    build_result(
        health,
        device_guard,
        u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    )
}

fn build_result(
    health: [Result<SecurityHealth, Failure>; 6],
    device_guard: Result<DeviceGuard, Failure>,
    duration_ms: u64,
) -> WindowsSecurityCollectionResult {
    let mut fields = Vec::new();
    let mut messages = health
        .iter()
        .enumerate()
        .filter(|(_, result)| matches!(result, Ok(SecurityHealth::NotMonitored)))
        .map(|(index, _)| CollectionMessage {
            code: "wsc_not_monitored".into(),
            native_code: None,
            message: Some(format!("Security Centerの監視対象外です: {}", PATHS[index])),
        })
        .collect::<Vec<_>>();
    let mut save = |path: &str, failure: Failure| {
        fields.push(FieldCollectionResult {
            path: path.into(),
            status: failure.status,
            code: failure.code.into(),
            native_code: failure.native_code,
        });
        messages.push(CollectionMessage {
            code: failure.code.into(),
            native_code: failure.native_code,
            message: Some(format!(
                "Windowsセキュリティ情報を取得できませんでした: {path}"
            )),
        });
    };
    let mut health = health
        .into_iter()
        .enumerate()
        .map(|(index, result)| match result {
            Ok(value) => Some(value),
            Err(failure) => {
                save(PATHS[index], failure);
                None
            }
        })
        .collect::<Vec<_>>()
        .into_iter();
    let (configured, running) = match device_guard {
        Ok(raw) => match raw.error {
            Some(error) => {
                let failure = Failure::new(
                    error.status,
                    "device_guard_query_failed",
                    Some(error.native_code),
                );
                (Err(failure.clone()), Err(failure))
            }
            None => (
                memory_integrity_present(raw.configured),
                memory_integrity_present(raw.running),
            ),
        },
        Err(failure) => (Err(failure.clone()), Err(failure)),
    };
    let configured = match configured {
        Ok(true) => Some(MemoryIntegrityConfiguration::Enabled),
        Ok(false) => Some(MemoryIntegrityConfiguration::Disabled),
        Err(failure) => {
            save(PATHS[6], failure);
            None
        }
    };
    let running = match running {
        Ok(true) => Some(MemoryIntegrityRunningState::Running),
        Ok(false) => Some(MemoryIntegrityRunningState::NotRunning),
        Err(failure) => {
            save(PATHS[7], failure);
            None
        }
    };
    WindowsSecurityCollectionResult {
        collection: WindowsSecurityCollection {
            firewall: health.next().flatten(),
            automatic_updates: health.next().flatten(),
            antivirus: health.next().flatten(),
            internet_settings: health.next().flatten(),
            user_account_control: health.next().flatten(),
            security_center_service: health.next().flatten(),
            memory_integrity: MemoryIntegrity {
                configured,
                running,
            },
        },
        status: CollectorResult {
            name: CollectorName::WindowsSecurity,
            status: match fields.len() {
                0 => CollectorStatus::Success,
                8 => CollectorStatus::Failed,
                _ => CollectorStatus::Partial,
            },
            duration_ms,
            messages,
            fields,
        },
    }
}

fn memory_integrity_present(services: Option<Vec<u32>>) -> Result<bool, Failure> {
    let services = services
        .filter(|values| !values.is_empty())
        .ok_or_else(|| {
            Failure::new(
                FieldCollectionStatus::SourceNull,
                "device_guard_property_unavailable",
                None,
            )
        })?;
    if services.contains(&0) && services.iter().any(|value| *value != 0) {
        return Err(Failure::new(
            FieldCollectionStatus::InvalidValue,
            "device_guard_services_invalid",
            None,
        ));
    }
    // Other values represent other security services, not HVCI.
    Ok(services.contains(&2))
}

#[cfg(any(windows, test))]
fn decode_health(hresult: i32, health: i32) -> Result<SecurityHealth, Failure> {
    // S_FALSE is a success HRESULT, but WSC uses it when its service is not running.
    // The accompanying POOR output must not be treated as an observed protection health.
    if hresult != 0 {
        let (status, code) = match hresult as u32 {
            1 => (
                FieldCollectionStatus::NotCollected,
                "wsc_service_not_running",
            ),
            0x80070005 => (
                FieldCollectionStatus::PermissionDenied,
                "wsc_permission_denied",
            ),
            0x80004001 | 0x80070032 => (FieldCollectionStatus::Unsupported, "wsc_unsupported"),
            _ => (FieldCollectionStatus::Failed, "wsc_query_failed"),
        };
        return Err(Failure::new(status, code, Some(i64::from(hresult))));
    }
    match health {
        0 => Ok(SecurityHealth::Good),
        1 => Ok(SecurityHealth::NotMonitored),
        2 => Ok(SecurityHealth::Poor),
        3 => Ok(SecurityHealth::Snooze),
        _ => Err(Failure::new(
            FieldCollectionStatus::InvalidValue,
            "wsc_health_invalid",
            Some(i64::from(health)),
        )),
    }
}

#[cfg(any(windows, test))]
fn parse_device_guard(bytes: &[u8]) -> Result<DeviceGuard, Failure> {
    serde_json::from_slice(bytes).map_err(|_| {
        Failure::new(
            FieldCollectionStatus::InvalidValue,
            "device_guard_invalid_output",
            None,
        )
    })
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::process::{Command, Stdio};
    use windows_sys::Win32::{
        Foundation::{FreeLibrary, GetLastError},
        System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
    };

    pub(super) fn health(provider: u32) -> Result<SecurityHealth, Failure> {
        // Load only the OS DLL. Missing WSC must not prevent the entire CLI from starting.
        // SAFETY: constant NUL-terminated name, no file handle, documented search flag.
        let library = unsafe {
            LoadLibraryExW(
                windows_sys::w!("wscapi.dll"),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };
        if library.is_null() {
            // SAFETY: GetLastError has no preconditions and follows the failed call.
            let error = unsafe { GetLastError() };
            let status = match error {
                126 | 127 => FieldCollectionStatus::Unsupported,
                5 => FieldCollectionStatus::PermissionDenied,
                _ => FieldCollectionStatus::Failed,
            };
            return Err(Failure::new(
                status,
                "wsc_library_unavailable",
                Some(i64::from(error)),
            ));
        }
        // SAFETY: library remains loaded until after the synchronous call below.
        let address =
            unsafe { GetProcAddress(library, c"WscGetSecurityProviderHealth".as_ptr().cast()) };
        let result = if let Some(address) = address {
            // SAFETY: this exported WSC function uses the documented system ABI with DWORD,
            // a pointer to a 32-bit health enum, and an HRESULT return value.
            let query = unsafe {
                std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    unsafe extern "system" fn(u32, *mut i32) -> i32,
                >(address)
            };
            let mut health = -1;
            // SAFETY: one provider flag and a valid writable enum pointer.
            decode_health(unsafe { query(provider, &mut health) }, health)
        } else {
            // SAFETY: immediately follows the failed symbol lookup.
            Err(Failure::new(
                FieldCollectionStatus::Unsupported,
                "wsc_api_unavailable",
                Some(i64::from(unsafe { GetLastError() })),
            ))
        };
        // SAFETY: releases exactly the reference acquired by LoadLibraryExW above.
        unsafe { FreeLibrary(library) };
        result
    }

    pub(super) fn device_guard() -> Result<DeviceGuard, Failure> {
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                include_str!("windows_security.ps1"),
            ])
            .stdin(Stdio::null())
            .output()
            .map_err(|error| {
                Failure::new(
                    FieldCollectionStatus::Failed,
                    "device_guard_process_failed",
                    error.raw_os_error().map(i64::from),
                )
            })?;
        if !output.status.success() {
            return Err(Failure::new(
                FieldCollectionStatus::Failed,
                "device_guard_process_failed",
                output.status.code().map(i64::from),
            ));
        }
        parse_device_guard(&output.stdout)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    pub(super) fn health(_: u32) -> Result<SecurityHealth, Failure> {
        Err(unsupported())
    }
    pub(super) fn device_guard() -> Result<DeviceGuard, Failure> {
        Err(unsupported())
    }
    fn unsupported() -> Failure {
        Failure::new(
            FieldCollectionStatus::Unsupported,
            "windows_security_platform_unsupported",
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy() -> [Result<SecurityHealth, Failure>; 6] {
        std::array::from_fn(|_| Ok(SecurityHealth::Good))
    }
    fn device_guard() -> DeviceGuard {
        DeviceGuard {
            configured: Some(vec![1, 2]),
            running: Some(vec![1]),
            error: None,
        }
    }

    #[test]
    fn maps_all_wsc_values_and_rejects_unknown_values() {
        for (raw, expected) in [
            (0, SecurityHealth::Good),
            (1, SecurityHealth::NotMonitored),
            (2, SecurityHealth::Poor),
            (3, SecurityHealth::Snooze),
        ] {
            assert_eq!(decode_health(0, raw).unwrap(), expected);
        }
        assert_eq!(
            decode_health(0, 99).unwrap_err().status,
            FieldCollectionStatus::InvalidValue
        );
    }

    #[test]
    fn unmonitored_is_an_observation_and_is_also_recorded_in_status() {
        let mut health = healthy();
        health[3] = Ok(SecurityHealth::NotMonitored);
        let result = build_result(health, Ok(device_guard()), 0);
        assert_eq!(result.status.status, CollectorStatus::Success);
        assert_eq!(result.status.messages[0].code, "wsc_not_monitored");
        assert!(result.status.fields.is_empty());
    }

    #[test]
    fn poor_and_snooze_are_successful_observations_not_collection_failures() {
        let mut health = healthy();
        health[0] = Ok(SecurityHealth::Poor);
        health[1] = Ok(SecurityHealth::Snooze);
        let result = build_result(health, Ok(device_guard()), 0);
        assert_eq!(result.status.status, CollectorStatus::Success);
        assert_eq!(result.collection.firewall, Some(SecurityHealth::Poor));
        assert_eq!(
            result.collection.automatic_updates,
            Some(SecurityHealth::Snooze)
        );
    }

    #[test]
    fn all_sources_failing_keeps_each_reason() {
        let health = std::array::from_fn(|_| decode_health(1, 2));
        let result = build_result(
            health,
            Err(Failure::new(
                FieldCollectionStatus::PermissionDenied,
                "device_guard_query_failed",
                Some(-2147024891),
            )),
            0,
        );
        assert_eq!(result.status.status, CollectorStatus::Failed);
        assert_eq!(result.collection, WindowsSecurityCollection::default());
        assert_eq!(result.status.fields.len(), 8);
        assert_eq!(result.status.fields[0].code, "wsc_service_not_running");
        assert_eq!(
            result.status.fields[7].status,
            FieldCollectionStatus::PermissionDenied
        );
    }

    #[cfg(windows)]
    #[test]
    fn live_windows_collection_returns_valid_data_or_explicit_failure_reasons() {
        // Read-only smoke test; succeeds on unsupported hosts too. Run from a standard-user
        // terminal on Windows 10/11 for the manual non-elevated acceptance check.
        let result = collect_windows_security();
        let mut collection: pcdiag_core::Collection = serde_json::from_str(include_str!(
            "../../pcdiag-core/tests/fixtures/memory-success-collection.json"
        ))
        .unwrap();
        let mut status: pcdiag_core::CollectionStatus = serde_json::from_str(include_str!(
            "../../pcdiag-core/tests/fixtures/memory-success-status.json"
        ))
        .unwrap();
        collection.windows_security = Some(result.collection);
        status.collectors.push(result.status);
        collection.validate_with_status(&status).unwrap();
    }

    #[test]
    fn service_stopped_and_api_failure_do_not_become_poor() {
        assert_eq!(
            decode_health(1, 2).unwrap_err().code,
            "wsc_service_not_running"
        );
        assert_eq!(
            decode_health(0x80070005_u32 as i32, 0).unwrap_err().status,
            FieldCollectionStatus::PermissionDenied
        );
        assert_eq!(
            decode_health(0x80004001_u32 as i32, 0).unwrap_err().status,
            FieldCollectionStatus::Unsupported
        );
        assert_eq!(
            decode_health(0x80004005_u32 as i32, 0).unwrap_err().status,
            FieldCollectionStatus::Failed
        );
    }

    #[test]
    fn successful_reads_keep_configuration_separate_from_runtime() {
        let result = build_result(healthy(), Ok(device_guard()), 1);
        assert_eq!(result.status.status, CollectorStatus::Success);
        assert_eq!(
            result.collection.memory_integrity.configured,
            Some(MemoryIntegrityConfiguration::Enabled)
        );
        assert_eq!(
            result.collection.memory_integrity.running,
            Some(MemoryIntegrityRunningState::NotRunning)
        );
    }

    #[test]
    fn one_permission_denied_category_preserves_other_categories() {
        let mut health = healthy();
        health[0] = decode_health(0x80070005_u32 as i32, 0);
        let result = build_result(health, Ok(device_guard()), 1);
        assert_eq!(result.status.status, CollectorStatus::Partial);
        assert_eq!(result.collection.firewall, None);
        assert_eq!(result.collection.antivirus, Some(SecurityHealth::Good));
        assert_eq!(result.status.fields[0].path, PATHS[0]);
        assert_eq!(result.status.fields[0].native_code, Some(-2147024891));
    }

    #[test]
    fn unavailable_device_guard_does_not_erase_security_center_health() {
        let raw = parse_device_guard(
            br#"{"error":{"status":"permission_denied","native_code":-2147024891}}"#,
        )
        .unwrap();
        let result = build_result(healthy(), Ok(raw), 0);
        assert_eq!(result.status.status, CollectorStatus::Partial);
        assert_eq!(result.collection.antivirus, Some(SecurityHealth::Good));
        assert_eq!(
            result.collection.memory_integrity,
            MemoryIntegrity::default()
        );
        assert_eq!(result.status.fields.len(), 2);
    }

    #[test]
    fn missing_device_guard_property_is_not_disabled() {
        let raw = parse_device_guard(br#"{"configured":null,"running":[2]}"#).unwrap();
        let result = build_result(healthy(), Ok(raw), 0);
        assert_eq!(result.collection.memory_integrity.configured, None);
        assert_eq!(
            result.collection.memory_integrity.running,
            Some(MemoryIntegrityRunningState::Running)
        );
        assert_eq!(
            result.status.fields[0].status,
            FieldCollectionStatus::SourceNull
        );
        assert!(memory_integrity_present(Some(vec![])).is_err());
        assert!(memory_integrity_present(Some(vec![0, 2])).is_err());
        assert!(!memory_integrity_present(Some(vec![0])).unwrap());
        assert!(parse_device_guard(br#"{"running":"unexpected"}"#).is_err());
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_platform_preserves_field_reasons_even_when_all_fail() {
        let result = collect_windows_security();
        assert_eq!(result.status.status, CollectorStatus::Failed);
        assert_eq!(result.collection, WindowsSecurityCollection::default());
        assert_eq!(result.status.fields.len(), 8);
        assert!(
            result
                .status
                .fields
                .iter()
                .all(|field| field.status == FieldCollectionStatus::Unsupported)
        );
    }
}
