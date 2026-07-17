use std::time::Instant;

use pcdiag_core::{
    CollectionMessage, CollectorName, CollectorResult, CollectorStatus, DiskSmart,
    FieldCollectionResult, FieldCollectionStatus, SmartProtocol,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmartCollectionResult {
    pub collection: Option<Vec<DiskSmart>>,
    pub status: CollectorResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SmartSnapshot {
    disk_number: u32,
    protocol: SmartProtocol,
    predict_failure: Option<bool>,
    critical_warning: Option<u8>,
    temperature_celsius: Option<i16>,
    available_spare_percent: Option<u8>,
    percentage_used: Option<u8>,
    power_on_hours: Option<u64>,
    unsafe_shutdowns: Option<u64>,
    media_errors: Option<u64>,
    missing: Vec<MissingField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MissingField {
    suffix: &'static str,
    status: FieldCollectionStatus,
    code: &'static str,
    native_code: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumerationFailure {
    native_code: Option<i64>,
}

pub fn collect_smart() -> SmartCollectionResult {
    let started = Instant::now();
    build_result(platform::collect(), elapsed_ms(started))
}

fn build_result(
    snapshots: Result<Vec<SmartSnapshot>, EnumerationFailure>,
    duration_ms: u64,
) -> SmartCollectionResult {
    match snapshots {
        Ok(snapshots) => {
            let mut fields = Vec::new();
            let collection = snapshots
                .into_iter()
                .enumerate()
                .map(|(index, snapshot)| map_smart(snapshot, index, &mut fields))
                .collect();
            let has_failure = fields
                .iter()
                .any(|field| field.status != FieldCollectionStatus::NotApplicable);
            SmartCollectionResult {
                collection: Some(collection),
                status: CollectorResult {
                    name: CollectorName::Smart,
                    status: if has_failure {
                        CollectorStatus::Partial
                    } else {
                        CollectorStatus::Success
                    },
                    duration_ms,
                    messages: vec![],
                    fields,
                },
            }
        }
        Err(failure) => SmartCollectionResult {
            collection: None,
            status: CollectorResult {
                name: CollectorName::Smart,
                status: CollectorStatus::Failed,
                duration_ms,
                messages: vec![CollectionMessage {
                    code: "smart_enumeration_failed".into(),
                    native_code: failure.native_code,
                    message: Some("SMART対象の物理ディスクを列挙できませんでした".into()),
                }],
                fields: vec![],
            },
        },
    }
}

fn map_smart(
    snapshot: SmartSnapshot,
    index: usize,
    fields: &mut Vec<FieldCollectionResult>,
) -> DiskSmart {
    let base = format!("/storage/smart/{index}");
    for (suffix, available) in [
        ("predict_failure", snapshot.predict_failure.is_some()),
        ("critical_warning", snapshot.critical_warning.is_some()),
        (
            "temperature_celsius",
            snapshot.temperature_celsius.is_some(),
        ),
        (
            "available_spare_percent",
            snapshot.available_spare_percent.is_some(),
        ),
        ("percentage_used", snapshot.percentage_used.is_some()),
        ("power_on_hours", snapshot.power_on_hours.is_some()),
        ("unsafe_shutdowns", snapshot.unsafe_shutdowns.is_some()),
        ("media_errors", snapshot.media_errors.is_some()),
    ] {
        if !available {
            let missing = snapshot
                .missing
                .iter()
                .find(|missing| missing.suffix == suffix);
            fields.push(FieldCollectionResult {
                path: format!("{base}/{suffix}"),
                status: missing.map_or(FieldCollectionStatus::SourceNull, |value| value.status),
                code: missing
                    .map_or("smart_value_unavailable", |value| value.code)
                    .into(),
                native_code: missing.and_then(|value| value.native_code),
            });
        }
    }

    DiskSmart {
        disk_number: snapshot.disk_number,
        protocol: snapshot.protocol,
        predict_failure: snapshot.predict_failure,
        critical_warning: snapshot.critical_warning,
        temperature_celsius: snapshot.temperature_celsius,
        available_spare_percent: snapshot.available_spare_percent,
        percentage_used: snapshot.percentage_used,
        power_on_hours: snapshot.power_on_hours,
        unsafe_shutdowns: snapshot.unsafe_shutdowns,
        media_errors: snapshot.media_errors,
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(windows)]
mod platform {
    use std::{ffi::c_void, mem::size_of, ptr::read_unaligned};

    use windows::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
        HANDLE,
    };
    use windows::Win32::Storage::Nvme::{NVME_HEALTH_INFO_LOG, NVME_LOG_PAGE_HEALTH_INFO};
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{
        IOCTL_STORAGE_PREDICT_FAILURE, IOCTL_STORAGE_QUERY_PROPERTY, NVMeDataTypeLogPage,
        PropertyStandardQuery, ProtocolTypeNvme, STORAGE_PREDICT_FAILURE, STORAGE_PROPERTY_QUERY,
        STORAGE_PROTOCOL_DATA_DESCRIPTOR, STORAGE_PROTOCOL_SPECIFIC_DATA,
        StorageDeviceProtocolSpecificProperty,
    };

    use super::{
        EnumerationFailure, FieldCollectionStatus, MissingField, SmartProtocol, SmartSnapshot,
    };
    use crate::physical_disks::{enumerate_disk_numbers, open_disk};

    const NVME_HEALTH_LOG_SIZE: usize = 512;

    pub(super) fn collect() -> Result<Vec<SmartSnapshot>, EnumerationFailure> {
        let numbers = enumerate_disk_numbers().map_err(failure)?;
        let mut snapshots = Vec::with_capacity(numbers.len());
        for number in numbers {
            match open_disk(number) {
                Ok(handle) => snapshots.push(query_smart(handle.0, number)),
                Err(error) => snapshots.push(unavailable_snapshot(number, &[error])),
            }
        }
        Ok(snapshots)
    }

    fn query_smart(handle: HANDLE, disk_number: u32) -> SmartSnapshot {
        match query_nvme_health(handle) {
            Ok(health) => nvme_snapshot(disk_number, health),
            Err(nvme_error) => match query_failure_prediction(handle) {
                Ok(predict_failure) => prediction_snapshot(disk_number, predict_failure),
                Err(prediction_error) => {
                    unavailable_snapshot(disk_number, &[nvme_error, prediction_error])
                }
            },
        }
    }

    fn query_failure_prediction(handle: HANDLE) -> windows::core::Result<bool> {
        let mut value = STORAGE_PREDICT_FAILURE::default();
        let mut returned = 0;
        // SAFETY: The output pointer refers to a writable STORAGE_PREDICT_FAILURE structure.
        unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_PREDICT_FAILURE,
                None,
                0,
                Some((&raw mut value).cast::<c_void>()),
                size_of::<STORAGE_PREDICT_FAILURE>() as u32,
                Some(&mut returned),
                None,
            )?
        };
        Ok(value.PredictFailure != 0)
    }

    fn query_nvme_health(handle: HANDLE) -> windows::core::Result<NVME_HEALTH_INFO_LOG> {
        let protocol_offset = std::mem::offset_of!(STORAGE_PROPERTY_QUERY, AdditionalParameters);
        let data_offset = size_of::<STORAGE_PROTOCOL_SPECIFIC_DATA>();
        let buffer_size = protocol_offset + data_offset + NVME_HEALTH_LOG_SIZE;
        let mut buffer = vec![0_u8; buffer_size];
        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceProtocolSpecificProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        let protocol = STORAGE_PROTOCOL_SPECIFIC_DATA {
            ProtocolType: ProtocolTypeNvme,
            DataType: NVMeDataTypeLogPage.0 as u32,
            ProtocolDataRequestValue: NVME_LOG_PAGE_HEALTH_INFO.0 as u32,
            ProtocolDataRequestSubValue: 0,
            ProtocolDataOffset: data_offset as u32,
            ProtocolDataLength: NVME_HEALTH_LOG_SIZE as u32,
            FixedProtocolReturnData: 0,
            ProtocolDataRequestSubValue2: 0,
            ProtocolDataRequestSubValue3: 0,
            ProtocolDataRequestSubValue4: 0,
        };
        // SAFETY: Both structures are copied into non-overlapping, sufficiently large regions.
        unsafe {
            std::ptr::write_unaligned(buffer.as_mut_ptr().cast::<STORAGE_PROPERTY_QUERY>(), query);
            std::ptr::write_unaligned(
                buffer
                    .as_mut_ptr()
                    .add(protocol_offset)
                    .cast::<STORAGE_PROTOCOL_SPECIFIC_DATA>(),
                protocol,
            );
        }
        let mut returned = 0;
        // SAFETY: The same buffer contains a valid query and is writable for the response.
        unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                Some(buffer.as_ptr().cast::<c_void>()),
                buffer.len() as u32,
                Some(buffer.as_mut_ptr().cast::<c_void>()),
                buffer.len() as u32,
                Some(&mut returned),
                None,
            )?
        };
        if (returned as usize) < size_of::<STORAGE_PROTOCOL_DATA_DESCRIPTOR>() {
            return Err(invalid_data());
        }
        // SAFETY: DeviceIoControl initialized the descriptor header on success.
        let descriptor =
            unsafe { read_unaligned(buffer.as_ptr().cast::<STORAGE_PROTOCOL_DATA_DESCRIPTOR>()) };
        let response_protocol_offset =
            std::mem::offset_of!(STORAGE_PROTOCOL_DATA_DESCRIPTOR, ProtocolSpecificData);
        let start = response_protocol_offset
            .checked_add(descriptor.ProtocolSpecificData.ProtocolDataOffset as usize)
            .ok_or_else(invalid_data)?;
        let length = descriptor.ProtocolSpecificData.ProtocolDataLength as usize;
        if length < NVME_HEALTH_LOG_SIZE
            || start
                .checked_add(NVME_HEALTH_LOG_SIZE)
                .is_none_or(|end| end > returned as usize || end > buffer.len())
        {
            return Err(invalid_data());
        }
        // SAFETY: Bounds and minimum log length were checked above.
        Ok(unsafe { read_unaligned(buffer.as_ptr().add(start).cast::<NVME_HEALTH_INFO_LOG>()) })
    }

    fn nvme_snapshot(disk_number: u32, health: NVME_HEALTH_INFO_LOG) -> SmartSnapshot {
        let mut missing = vec![not_applicable(
            "predict_failure",
            "nvme_uses_critical_warning",
        )];
        let temperature_kelvin = u16::from_le_bytes(health.Temperature);
        let temperature_celsius = if temperature_kelvin == 0 {
            missing.push(invalid_value(
                "temperature_celsius",
                "nvme_temperature_invalid",
            ));
            None
        } else {
            i16::try_from(i32::from(temperature_kelvin) - 273).ok()
        };
        let power_on_hours = counter(&health.PowerOnHours, "power_on_hours", &mut missing);
        let unsafe_shutdowns = counter(&health.UnsafeShutdowns, "unsafe_shutdowns", &mut missing);
        let media_errors = counter(&health.MediaErrors, "media_errors", &mut missing);

        SmartSnapshot {
            disk_number,
            protocol: SmartProtocol::Nvme,
            predict_failure: None,
            // SAFETY: AsUchar reads the complete byte representation of the union.
            critical_warning: Some(unsafe { health.CriticalWarning.AsUchar }),
            temperature_celsius,
            available_spare_percent: Some(health.AvailableSpare),
            percentage_used: Some(health.PercentageUsed),
            power_on_hours,
            unsafe_shutdowns,
            media_errors,
            missing,
        }
    }

    fn prediction_snapshot(disk_number: u32, predict_failure: bool) -> SmartSnapshot {
        let missing = [
            "critical_warning",
            "temperature_celsius",
            "available_spare_percent",
            "percentage_used",
            "power_on_hours",
            "unsafe_shutdowns",
            "media_errors",
        ]
        .into_iter()
        .map(|suffix| not_applicable(suffix, "nvme_health_log_not_used"))
        .collect();
        SmartSnapshot {
            disk_number,
            protocol: SmartProtocol::FailurePrediction,
            predict_failure: Some(predict_failure),
            critical_warning: None,
            temperature_celsius: None,
            available_spare_percent: None,
            percentage_used: None,
            power_on_hours: None,
            unsafe_shutdowns: None,
            media_errors: None,
            missing,
        }
    }

    fn unavailable_snapshot(disk_number: u32, errors: &[windows::core::Error]) -> SmartSnapshot {
        let (status, code, native_code) = classify_errors(errors);
        let missing = [
            "predict_failure",
            "critical_warning",
            "temperature_celsius",
            "available_spare_percent",
            "percentage_used",
            "power_on_hours",
            "unsafe_shutdowns",
            "media_errors",
        ]
        .into_iter()
        .map(|suffix| MissingField {
            suffix,
            status,
            code,
            native_code,
        })
        .collect();
        SmartSnapshot {
            disk_number,
            protocol: SmartProtocol::Unknown,
            predict_failure: None,
            critical_warning: None,
            temperature_celsius: None,
            available_spare_percent: None,
            percentage_used: None,
            power_on_hours: None,
            unsafe_shutdowns: None,
            media_errors: None,
            missing,
        }
    }

    fn classify_errors(
        errors: &[windows::core::Error],
    ) -> (FieldCollectionStatus, &'static str, Option<i64>) {
        if let Some(error) = errors
            .iter()
            .find(|error| error.code() == ERROR_ACCESS_DENIED.to_hresult())
        {
            return (
                FieldCollectionStatus::PermissionDenied,
                "smart_permission_denied",
                Some(i64::from(error.code().0)),
            );
        }
        let unsupported = |error: &windows::core::Error| {
            matches!(
                error.code(),
                code if code == ERROR_INVALID_FUNCTION.to_hresult()
                    || code == ERROR_INVALID_PARAMETER.to_hresult()
                    || code == ERROR_NOT_SUPPORTED.to_hresult()
            )
        };
        if errors.iter().all(unsupported) {
            return (
                FieldCollectionStatus::Unsupported,
                "smart_not_supported",
                errors.first().map(|error| i64::from(error.code().0)),
            );
        }
        (
            FieldCollectionStatus::Failed,
            "smart_query_failed",
            errors.first().map(|error| i64::from(error.code().0)),
        )
    }

    fn counter(
        bytes: &[u8; 16],
        suffix: &'static str,
        missing: &mut Vec<MissingField>,
    ) -> Option<u64> {
        let value = u128::from_le_bytes(*bytes);
        match u64::try_from(value) {
            Ok(value) => Some(value),
            Err(_) => {
                missing.push(invalid_value(suffix, "nvme_counter_out_of_range"));
                None
            }
        }
    }

    fn not_applicable(suffix: &'static str, code: &'static str) -> MissingField {
        MissingField {
            suffix,
            status: FieldCollectionStatus::NotApplicable,
            code,
            native_code: None,
        }
    }

    fn invalid_value(suffix: &'static str, code: &'static str) -> MissingField {
        MissingField {
            suffix,
            status: FieldCollectionStatus::InvalidValue,
            code,
            native_code: None,
        }
    }

    fn failure(error: windows::core::Error) -> EnumerationFailure {
        EnumerationFailure {
            native_code: Some(i64::from(error.code().0)),
        }
    }

    fn invalid_data() -> windows::core::Error {
        windows::core::Error::from_hresult(windows::core::HRESULT(0x8007000D_u32 as i32))
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{EnumerationFailure, SmartSnapshot};

    pub(super) fn collect() -> Result<Vec<SmartSnapshot>, EnumerationFailure> {
        Err(EnumerationFailure { native_code: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_failure_prediction_without_nvme_fields() {
        let snapshot = SmartSnapshot {
            disk_number: 0,
            protocol: SmartProtocol::FailurePrediction,
            predict_failure: Some(false),
            critical_warning: None,
            temperature_celsius: None,
            available_spare_percent: None,
            percentage_used: None,
            power_on_hours: None,
            unsafe_shutdowns: None,
            media_errors: None,
            missing: [
                "critical_warning",
                "temperature_celsius",
                "available_spare_percent",
                "percentage_used",
                "power_on_hours",
                "unsafe_shutdowns",
                "media_errors",
            ]
            .into_iter()
            .map(|suffix| MissingField {
                suffix,
                status: FieldCollectionStatus::NotApplicable,
                code: "nvme_health_log_not_used",
                native_code: None,
            })
            .collect(),
        };
        let result = build_result(Ok(vec![snapshot]), 4);

        assert_eq!(result.status.status, CollectorStatus::Success);
        assert_eq!(result.collection.unwrap()[0].predict_failure, Some(false));
    }

    #[test]
    fn permission_denied_keeps_a_disk_result() {
        let snapshot = SmartSnapshot {
            disk_number: 2,
            protocol: SmartProtocol::Unknown,
            predict_failure: None,
            critical_warning: None,
            temperature_celsius: None,
            available_spare_percent: None,
            percentage_used: None,
            power_on_hours: None,
            unsafe_shutdowns: None,
            media_errors: None,
            missing: [
                "predict_failure",
                "critical_warning",
                "temperature_celsius",
                "available_spare_percent",
                "percentage_used",
                "power_on_hours",
                "unsafe_shutdowns",
                "media_errors",
            ]
            .into_iter()
            .map(|suffix| MissingField {
                suffix,
                status: FieldCollectionStatus::PermissionDenied,
                code: "smart_permission_denied",
                native_code: Some(5),
            })
            .collect(),
        };
        let result = build_result(Ok(vec![snapshot]), 4);

        assert_eq!(result.status.status, CollectorStatus::Partial);
        assert!(
            result
                .status
                .fields
                .iter()
                .all(|field| field.status == FieldCollectionStatus::PermissionDenied)
        );
    }
}
