use std::time::Instant;

use pcdiag_core::{
    ClockCollection, CollectionMessage, CollectorName, CollectorResult, CollectorStatus,
    FieldCollectionResult, FieldCollectionStatus, WindowsServiceState,
};

const SYSTEM_TIME_PATH: &str = "/clock/system_time_utc";
const UTC_OFFSET_PATH: &str = "/clock/utc_offset_minutes";
const TIME_SERVICE_PATH: &str = "/clock/windows_time_service";
const HARDWARE_CLOCK_PATH: &str = "/clock/hardware_clock";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockCollectionResult {
    pub collection: ClockCollection,
    pub status: CollectorResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CollectionFailure {
    code: &'static str,
    native_code: Option<i64>,
    message: &'static str,
    field_status: FieldCollectionStatus,
}

/// Collects Windows system-clock and time-service information.
pub fn collect_clock() -> ClockCollectionResult {
    let started = Instant::now();
    build_result(
        platform::system_time_utc(),
        platform::utc_offset_minutes(),
        platform::windows_time_service(),
        elapsed_ms(started),
    )
}

fn build_result(
    system_time_utc: Result<String, CollectionFailure>,
    utc_offset_minutes: Result<i16, CollectionFailure>,
    windows_time_service: Result<WindowsServiceState, CollectionFailure>,
    duration_ms: u64,
) -> ClockCollectionResult {
    let mut messages = Vec::new();
    let mut fields = vec![FieldCollectionResult {
        path: HARDWARE_CLOCK_PATH.into(),
        status: FieldCollectionStatus::Unsupported,
        code: "hardware_clock_direct_access_unsupported".into(),
        native_code: None,
    }];

    let system_time_utc = map_result(
        system_time_utc,
        SYSTEM_TIME_PATH,
        &mut messages,
        &mut fields,
    );
    let utc_offset_minutes = map_result(
        utc_offset_minutes,
        UTC_OFFSET_PATH,
        &mut messages,
        &mut fields,
    );
    let windows_time_service = map_result(
        windows_time_service,
        TIME_SERVICE_PATH,
        &mut messages,
        &mut fields,
    );

    let failed_values = fields.len();
    let status = if failed_values == 1 {
        // The standard Windows user-mode API does not expose the hardware RTC
        // independently. Keep that fact explicit without misreporting the
        // system clock as an RTC reading.
        CollectorStatus::Partial
    } else if failed_values == 4 {
        fields.clear();
        CollectorStatus::Failed
    } else {
        CollectorStatus::Partial
    };

    ClockCollectionResult {
        collection: ClockCollection {
            system_time_utc,
            utc_offset_minutes,
            windows_time_service,
            hardware_clock: None,
        },
        status: CollectorResult {
            name: CollectorName::Clock,
            status,
            duration_ms,
            messages,
            fields,
        },
    }
}

fn map_result<T>(
    result: Result<T, CollectionFailure>,
    path: &str,
    messages: &mut Vec<CollectionMessage>,
    fields: &mut Vec<FieldCollectionResult>,
) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(failure) => {
            messages.push(CollectionMessage {
                code: failure.code.into(),
                native_code: failure.native_code,
                message: Some(failure.message.into()),
            });
            fields.push(FieldCollectionResult {
                path: path.into(),
                status: failure.field_status,
                code: failure.code.into(),
                native_code: failure.native_code,
            });
            None
        }
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(windows)]
mod platform {
    use std::{io, mem::size_of, ptr};

    use windows_sys::Win32::{
        Foundation::{FILETIME, SYSTEMTIME},
        System::{
            Services::{
                CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatusEx, SC_HANDLE,
                SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO, SERVICE_CONTINUE_PENDING,
                SERVICE_PAUSE_PENDING, SERVICE_PAUSED, SERVICE_QUERY_STATUS, SERVICE_RUNNING,
                SERVICE_START_PENDING, SERVICE_STATUS_PROCESS, SERVICE_STOP_PENDING,
                SERVICE_STOPPED,
            },
            SystemInformation::GetSystemTimePreciseAsFileTime,
            Time::{
                DYNAMIC_TIME_ZONE_INFORMATION, FileTimeToSystemTime, GetDynamicTimeZoneInformation,
                TIME_ZONE_ID_INVALID,
            },
        },
    };

    use super::{CollectionFailure, FieldCollectionStatus, WindowsServiceState};

    pub(super) fn system_time_utc() -> Result<String, CollectionFailure> {
        let mut file_time = FILETIME::default();
        // SAFETY: `file_time` points to writable FILETIME storage.
        unsafe { GetSystemTimePreciseAsFileTime(&mut file_time) };
        let mut system_time = SYSTEMTIME::default();
        // SAFETY: both pointers refer to initialized readable/writable structures.
        if unsafe { FileTimeToSystemTime(&file_time, &mut system_time) } == 0 {
            return Err(last_error_failure(
                "system_time_conversion_failed",
                "Windowsのシステム時刻をUTC日時へ変換できませんでした",
            ));
        }
        Ok(format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            system_time.wYear,
            system_time.wMonth,
            system_time.wDay,
            system_time.wHour,
            system_time.wMinute,
            system_time.wSecond,
            system_time.wMilliseconds
        ))
    }

    pub(super) fn utc_offset_minutes() -> Result<i16, CollectionFailure> {
        let mut time_zone = DYNAMIC_TIME_ZONE_INFORMATION::default();
        // SAFETY: `time_zone` points to writable storage.
        let state = unsafe { GetDynamicTimeZoneInformation(&mut time_zone) };
        if state == TIME_ZONE_ID_INVALID {
            return Err(last_error_failure(
                "time_zone_information_failed",
                "Windowsから現在のUTCオフセットを取得できませんでした",
            ));
        }
        let active_bias = match state {
            1 => time_zone.Bias.saturating_add(time_zone.StandardBias),
            2 => time_zone.Bias.saturating_add(time_zone.DaylightBias),
            0 => time_zone.Bias,
            _ => time_zone.Bias,
        };
        let offset = active_bias.checked_neg().ok_or(CollectionFailure {
            code: "utc_offset_invalid",
            native_code: None,
            message: "Windowsが報告したUTCオフセットを解釈できませんでした",
            field_status: FieldCollectionStatus::InvalidValue,
        })?;
        i16::try_from(offset).map_err(|_| CollectionFailure {
            code: "utc_offset_invalid",
            native_code: None,
            message: "Windowsが報告したUTCオフセットが許容範囲外です",
            field_status: FieldCollectionStatus::InvalidValue,
        })
    }

    pub(super) fn windows_time_service() -> Result<WindowsServiceState, CollectionFailure> {
        // SAFETY: null server/database names select the local active database.
        let manager = unsafe { OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_CONNECT) };
        if manager.is_null() {
            return Err(last_error_failure(
                "service_manager_open_failed",
                "Windowsサービス管理機能を開けませんでした",
            ));
        }
        let manager = ServiceHandle(manager);
        const W32TIME: [u16; 8] = [
            b'W' as u16,
            b'3' as u16,
            b'2' as u16,
            b'T' as u16,
            b'i' as u16,
            b'm' as u16,
            b'e' as u16,
            0,
        ];
        // SAFETY: manager is valid and W32TIME is null-terminated.
        let service = unsafe { OpenServiceW(manager.0, W32TIME.as_ptr(), SERVICE_QUERY_STATUS) };
        if service.is_null() {
            return Err(last_error_failure(
                "windows_time_service_open_failed",
                "Windows Timeサービスを開けませんでした",
            ));
        }
        let service = ServiceHandle(service);
        let mut status = SERVICE_STATUS_PROCESS::default();
        let mut needed = 0;
        // SAFETY: status is writable and the supplied size matches its structure.
        if unsafe {
            QueryServiceStatusEx(
                service.0,
                SC_STATUS_PROCESS_INFO,
                (&mut status as *mut SERVICE_STATUS_PROCESS).cast(),
                size_of::<SERVICE_STATUS_PROCESS>() as u32,
                &mut needed,
            )
        } == 0
        {
            return Err(last_error_failure(
                "windows_time_service_query_failed",
                "Windows Timeサービスの状態を取得できませんでした",
            ));
        }
        Ok(if status.dwCurrentState == SERVICE_STOPPED {
            WindowsServiceState::Stopped
        } else if status.dwCurrentState == SERVICE_START_PENDING {
            WindowsServiceState::StartPending
        } else if status.dwCurrentState == SERVICE_STOP_PENDING {
            WindowsServiceState::StopPending
        } else if status.dwCurrentState == SERVICE_RUNNING {
            WindowsServiceState::Running
        } else if status.dwCurrentState == SERVICE_CONTINUE_PENDING {
            WindowsServiceState::ContinuePending
        } else if status.dwCurrentState == SERVICE_PAUSE_PENDING {
            WindowsServiceState::PausePending
        } else if status.dwCurrentState == SERVICE_PAUSED {
            WindowsServiceState::Paused
        } else {
            WindowsServiceState::Unknown
        })
    }

    struct ServiceHandle(SC_HANDLE);

    impl Drop for ServiceHandle {
        fn drop(&mut self) {
            // SAFETY: the handle was returned by an SCM function and is owned here.
            unsafe { CloseServiceHandle(self.0) };
        }
    }

    fn last_error_failure(code: &'static str, message: &'static str) -> CollectionFailure {
        CollectionFailure {
            code,
            native_code: io::Error::last_os_error().raw_os_error().map(i64::from),
            message,
            field_status: FieldCollectionStatus::Failed,
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{CollectionFailure, FieldCollectionStatus, WindowsServiceState};

    pub(super) fn system_time_utc() -> Result<String, CollectionFailure> {
        Err(unsupported())
    }

    pub(super) fn utc_offset_minutes() -> Result<i16, CollectionFailure> {
        Err(unsupported())
    }

    pub(super) fn windows_time_service() -> Result<WindowsServiceState, CollectionFailure> {
        Err(unsupported())
    }

    fn unsupported() -> CollectionFailure {
        CollectionFailure {
            code: "platform_not_supported",
            native_code: None,
            message: "Windows以外の環境では時計情報を収集できません",
            field_status: FieldCollectionStatus::Unsupported,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_available_clock_information_without_substituting_for_rtc() {
        let result = build_result(
            Ok("2026-07-17T04:00:00.000Z".into()),
            Ok(540),
            Ok(WindowsServiceState::Running),
            1,
        );

        assert_eq!(result.status.status, CollectorStatus::Partial);
        assert_eq!(result.collection.utc_offset_minutes, Some(540));
        assert_eq!(result.collection.hardware_clock, None);
        assert_eq!(result.status.fields.len(), 1);
        assert_eq!(result.status.fields[0].path, HARDWARE_CLOCK_PATH);
    }

    #[test]
    fn preserves_available_values_when_time_service_query_fails() {
        let result = build_result(
            Ok("2026-07-17T04:00:00.000Z".into()),
            Ok(540),
            Err(CollectionFailure {
                code: "windows_time_service_query_failed",
                native_code: Some(5),
                message: "failed",
                field_status: FieldCollectionStatus::Failed,
            }),
            2,
        );

        assert_eq!(result.status.status, CollectorStatus::Partial);
        assert_eq!(result.collection.windows_time_service, None);
        assert_eq!(result.status.fields.len(), 2);
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_collection_returns_failed() {
        let result = collect_clock();

        assert_eq!(result.status.status, CollectorStatus::Failed);
        assert!(result.collection.system_time_utc.is_none());
    }
}
