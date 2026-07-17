use std::time::Instant;

use pcdiag_core::{
    CollectionMessage, CollectorName, CollectorResult, CollectorStatus, FieldCollectionResult,
    FieldCollectionStatus, FirmwareCollection, FirmwareInterfaceType,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareCollectionResult {
    pub collection: FirmwareCollection,
    pub status: CollectorResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldFailure {
    path: &'static str,
    status: FieldCollectionStatus,
    code: &'static str,
    native_code: Option<i64>,
    message: Option<&'static str>,
}

pub fn collect_firmware() -> FirmwareCollectionResult {
    let started = Instant::now();
    let mut failures = Vec::new();

    let vendor = collect_value(platform::vendor(), "/firmware/vendor", &mut failures);
    let version = collect_value(platform::version(), "/firmware/version", &mut failures);
    let release_date = collect_value(
        platform::release_date(),
        "/firmware/release_date",
        &mut failures,
    );
    let interface_type = collect_value(
        platform::interface_type(),
        "/firmware/interface_type",
        &mut failures,
    );
    let secure_boot_enabled = match interface_type {
        Some(FirmwareInterfaceType::Uefi) => collect_value(
            platform::secure_boot_enabled(),
            "/firmware/secure_boot_enabled",
            &mut failures,
        ),
        Some(FirmwareInterfaceType::Bios) => {
            failures.push(FieldFailure {
                path: "/firmware/secure_boot_enabled",
                status: FieldCollectionStatus::NotApplicable,
                code: "secure_boot_only_applies_to_uefi",
                native_code: None,
                message: None,
            });
            None
        }
        Some(FirmwareInterfaceType::Unknown) => {
            failures.push(FieldFailure {
                path: "/firmware/secure_boot_enabled",
                status: FieldCollectionStatus::Unsupported,
                code: "secure_boot_firmware_type_unknown",
                native_code: None,
                message: None,
            });
            None
        }
        None => {
            failures.push(FieldFailure {
                path: "/firmware/secure_boot_enabled",
                status: FieldCollectionStatus::NotCollected,
                code: "secure_boot_requires_firmware_type",
                native_code: None,
                message: None,
            });
            None
        }
    };
    failures.push(FieldFailure {
        path: "/firmware/status",
        status: FieldCollectionStatus::Unsupported,
        code: "firmware_operational_status_unsupported",
        native_code: None,
        message: None,
    });

    let collection = FirmwareCollection {
        vendor,
        version,
        release_date,
        interface_type,
        secure_boot_enabled,
        status: None,
    };
    build_result(collection, failures, elapsed_ms(started))
}

fn collect_value<T>(
    value: Result<T, FieldFailure>,
    expected_path: &'static str,
    failures: &mut Vec<FieldFailure>,
) -> Option<T> {
    match value {
        Ok(value) => Some(value),
        Err(failure) => {
            debug_assert_eq!(failure.path, expected_path);
            failures.push(failure);
            None
        }
    }
}

fn build_result(
    collection: FirmwareCollection,
    failures: Vec<FieldFailure>,
    duration_ms: u64,
) -> FirmwareCollectionResult {
    let collected_count = [
        collection.vendor.is_some(),
        collection.version.is_some(),
        collection.release_date.is_some(),
        collection.interface_type.is_some(),
        collection.secure_boot_enabled.is_some(),
    ]
    .into_iter()
    .filter(|collected| *collected)
    .count();
    let status = if collected_count == 0 {
        CollectorStatus::Failed
    } else if failures.is_empty() {
        CollectorStatus::Success
    } else {
        CollectorStatus::Partial
    };
    let messages = failures
        .iter()
        .filter_map(|failure| {
            failure.message.map(|message| CollectionMessage {
                code: failure.code.into(),
                native_code: failure.native_code,
                message: Some(message.into()),
            })
        })
        .collect();
    let fields = if status == CollectorStatus::Failed {
        Vec::new()
    } else {
        failures
            .into_iter()
            .map(|failure| FieldCollectionResult {
                path: failure.path.into(),
                status: failure.status,
                code: failure.code.into(),
                native_code: failure.native_code,
            })
            .collect()
    };

    FirmwareCollectionResult {
        collection,
        status: CollectorResult {
            name: CollectorName::Firmware,
            status,
            duration_ms,
            messages,
            fields,
        },
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(windows)]
mod platform {
    use std::{ffi::c_void, mem::size_of_val, ptr};

    use pcdiag_core::{FieldCollectionStatus, FirmwareInterfaceType};
    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS},
        System::{
            Registry::{
                HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_MULTI_SZ, RRF_RT_REG_SZ,
                RegGetValueW,
            },
            SystemInformation::{
                FirmwareTypeBios, FirmwareTypeUefi, FirmwareTypeUnknown, GetFirmwareType,
            },
        },
    };

    use super::FieldFailure;

    const BIOS_KEY: &str = r"HARDWARE\DESCRIPTION\System\BIOS";
    const SECURE_BOOT_KEY: &str = r"SYSTEM\CurrentControlSet\Control\SecureBoot\State";

    pub(super) fn vendor() -> Result<String, FieldFailure> {
        read_string(
            BIOS_KEY,
            "BIOSVendor",
            "/firmware/vendor",
            "firmware_vendor",
        )
    }

    pub(super) fn version() -> Result<String, FieldFailure> {
        read_string(
            BIOS_KEY,
            "BIOSVersion",
            "/firmware/version",
            "firmware_version",
        )
    }

    pub(super) fn release_date() -> Result<String, FieldFailure> {
        let raw = read_string(
            BIOS_KEY,
            "BIOSReleaseDate",
            "/firmware/release_date",
            "firmware_release_date",
        )?;
        normalize_date(&raw).ok_or(FieldFailure {
            path: "/firmware/release_date",
            status: FieldCollectionStatus::InvalidValue,
            code: "firmware_release_date_invalid",
            native_code: None,
            message: Some("BIOS公開日をYYYY-MM-DD形式へ変換できませんでした"),
        })
    }

    pub(super) fn interface_type() -> Result<FirmwareInterfaceType, FieldFailure> {
        let mut value = FirmwareTypeUnknown;
        // SAFETY: `value` points to writable FIRMWARE_TYPE storage.
        if unsafe { GetFirmwareType(&mut value) } == 0 {
            return Err(win32_failure(
                "/firmware/interface_type",
                "firmware_interface_type_failed",
                "WindowsからBIOS・UEFI区分を取得できませんでした",
                last_error_code(),
            ));
        }
        Ok(if value == FirmwareTypeBios {
            FirmwareInterfaceType::Bios
        } else if value == FirmwareTypeUefi {
            FirmwareInterfaceType::Uefi
        } else {
            FirmwareInterfaceType::Unknown
        })
    }

    pub(super) fn secure_boot_enabled() -> Result<bool, FieldFailure> {
        let key = wide(SECURE_BOOT_KEY);
        let name = wide("UEFISecureBootEnabled");
        let mut value = 0u32;
        let mut byte_len = u32::try_from(size_of_val(&value)).unwrap_or(4);
        // SAFETY: key/name are NUL-terminated and value points to writable DWORD storage.
        let result = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_DWORD,
                ptr::null_mut(),
                (&mut value as *mut u32).cast::<c_void>(),
                &mut byte_len,
            )
        };
        if result != ERROR_SUCCESS {
            return Err(registry_failure(
                "/firmware/secure_boot_enabled",
                "secure_boot_state",
                "WindowsからSecure Boot状態を取得できませんでした",
                result,
            ));
        }
        Ok(value != 0)
    }

    fn read_string(
        subkey: &str,
        value_name: &str,
        path: &'static str,
        code_prefix: &'static str,
    ) -> Result<String, FieldFailure> {
        let key = wide(subkey);
        let name = wide(value_name);
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_MULTI_SZ;
        let mut byte_len = 0u32;
        // SAFETY: key/name are NUL-terminated; a null output buffer requests its size.
        let result = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                name.as_ptr(),
                flags,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut byte_len,
            )
        };
        if result != ERROR_SUCCESS {
            return Err(registry_failure(
                path,
                code_prefix,
                "WindowsからBIOS文字列を取得できませんでした",
                result,
            ));
        }
        let word_len = usize::try_from(byte_len)
            .ok()
            .and_then(|length| length.checked_add(1))
            .map(|length| length / 2)
            .ok_or(FieldFailure {
                path,
                status: FieldCollectionStatus::Failed,
                code: "firmware_registry_value_too_large",
                native_code: None,
                message: Some("BIOS文字列用バッファーを確保できませんでした"),
            })?;
        let mut buffer = vec![0u16; word_len];
        // SAFETY: buffer has at least byte_len writable bytes and strings are NUL-terminated.
        let result = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                name.as_ptr(),
                flags,
                ptr::null_mut(),
                buffer.as_mut_ptr().cast::<c_void>(),
                &mut byte_len,
            )
        };
        if result != ERROR_SUCCESS {
            return Err(registry_failure(
                path,
                code_prefix,
                "WindowsからBIOS文字列を取得できませんでした",
                result,
            ));
        }
        let parts: Vec<_> = buffer
            .split(|value| *value == 0)
            .filter(|part| !part.is_empty())
            .map(String::from_utf16_lossy)
            .filter(|part| !part.trim().is_empty())
            .collect();
        if parts.is_empty() {
            return Err(FieldFailure {
                path,
                status: FieldCollectionStatus::SourceNull,
                code: "firmware_registry_value_empty",
                native_code: None,
                message: None,
            });
        }
        Ok(parts.join(" / ").trim().to_owned())
    }

    fn normalize_date(value: &str) -> Option<String> {
        let value = value.trim();
        let parts: Vec<_> = value.split(['/', '-']).collect();
        if parts.len() != 3 {
            return None;
        }
        let (year, month, day) = if parts[0].len() == 4 {
            (parts[0], parts[1], parts[2])
        } else {
            (parts[2], parts[0], parts[1])
        };
        let year = year.parse::<u32>().ok()?;
        let month = month.parse::<u32>().ok()?;
        let day = day.parse::<u32>().ok()?;
        if year == 0 || !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(format!("{year:04}-{month:02}-{day:02}"))
    }

    #[allow(clippy::manual_is_multiple_of)]
    fn days_in_month(year: u32, month: u32) -> u32 {
        match month {
            4 | 6 | 9 | 11 => 30,
            2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
            2 => 28,
            _ => 31,
        }
    }

    fn registry_failure(
        path: &'static str,
        code_prefix: &'static str,
        message: &'static str,
        result: u32,
    ) -> FieldFailure {
        let missing = result == ERROR_FILE_NOT_FOUND || result == ERROR_PATH_NOT_FOUND;
        FieldFailure {
            path,
            status: if missing {
                FieldCollectionStatus::SourceNull
            } else {
                FieldCollectionStatus::Failed
            },
            code: if missing {
                "firmware_registry_value_unavailable"
            } else {
                code_prefix
            },
            native_code: Some(i64::from(result)),
            message: (!missing).then_some(message),
        }
    }

    fn win32_failure(
        path: &'static str,
        code: &'static str,
        message: &'static str,
        native_code: Option<i64>,
    ) -> FieldFailure {
        FieldFailure {
            path,
            status: FieldCollectionStatus::Failed,
            code,
            native_code,
            message: Some(message),
        }
    }

    fn last_error_code() -> Option<i64> {
        std::io::Error::last_os_error()
            .raw_os_error()
            .map(i64::from)
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    #[cfg(test)]
    mod tests {
        use super::normalize_date;

        #[test]
        fn normalizes_common_bios_dates() {
            assert_eq!(normalize_date("07/17/2026").as_deref(), Some("2026-07-17"));
            assert_eq!(normalize_date("2026-07-17").as_deref(), Some("2026-07-17"));
            assert_eq!(normalize_date("02/30/2026"), None);
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use pcdiag_core::{FieldCollectionStatus, FirmwareInterfaceType};

    use super::FieldFailure;

    pub(super) fn vendor() -> Result<String, FieldFailure> {
        Err(unsupported("/firmware/vendor"))
    }

    pub(super) fn version() -> Result<String, FieldFailure> {
        Err(unsupported("/firmware/version"))
    }

    pub(super) fn release_date() -> Result<String, FieldFailure> {
        Err(unsupported("/firmware/release_date"))
    }

    pub(super) fn interface_type() -> Result<FirmwareInterfaceType, FieldFailure> {
        Err(unsupported("/firmware/interface_type"))
    }

    pub(super) fn secure_boot_enabled() -> Result<bool, FieldFailure> {
        Err(unsupported("/firmware/secure_boot_enabled"))
    }

    fn unsupported(path: &'static str) -> FieldFailure {
        FieldFailure {
            path,
            status: FieldCollectionStatus::Unsupported,
            code: "platform_not_supported",
            native_code: None,
            message: Some("Windows以外の環境ではBIOS・UEFI情報を収集できません"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcdiag_core::FirmwareOperationalStatus;

    #[test]
    fn maps_firmware_values_and_preserves_unsupported_status() {
        let result = build_result(
            FirmwareCollection {
                vendor: Some("Example Vendor".into()),
                version: Some("1.2.3".into()),
                release_date: Some("2026-07-17".into()),
                interface_type: Some(FirmwareInterfaceType::Uefi),
                secure_boot_enabled: Some(true),
                status: None,
            },
            vec![FieldFailure {
                path: "/firmware/status",
                status: FieldCollectionStatus::Unsupported,
                code: "firmware_operational_status_unsupported",
                native_code: None,
                message: None,
            }],
            2,
        );

        assert_eq!(result.status.status, CollectorStatus::Partial);
        assert_eq!(result.collection.vendor.as_deref(), Some("Example Vendor"));
        assert_eq!(result.collection.status, None::<FirmwareOperationalStatus>);
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_collection_returns_failed() {
        let result = collect_firmware();

        assert_eq!(result.status.status, CollectorStatus::Failed);
        assert!(result.collection.vendor.is_none());
        assert!(!result.status.messages.is_empty());
    }
}
