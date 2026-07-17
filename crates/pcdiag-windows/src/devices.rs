use std::time::Instant;

use pcdiag_core::{
    CollectionMessage, CollectorName, CollectorResult, CollectorStatus, ConnectedDevice,
    DeviceDriver, DeviceState, FieldCollectionResult, FieldCollectionStatus,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCollectionResult {
    pub collection: Option<Vec<ConnectedDevice>>,
    pub status: CollectorResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeviceSnapshot {
    name: Option<String>,
    manufacturer: Option<String>,
    class: Option<String>,
    class_guid: Option<String>,
    device_instance_id: Option<String>,
    present: Option<bool>,
    enabled: Option<bool>,
    problem_code: Option<u32>,
    driver_version: Option<String>,
    driver_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumerationFailure {
    native_code: Option<i64>,
}

pub fn collect_devices() -> DeviceCollectionResult {
    let started = Instant::now();
    build_result(platform::enumerate_devices(), elapsed_ms(started))
}

fn build_result(
    snapshots: Result<Vec<DeviceSnapshot>, EnumerationFailure>,
    duration_ms: u64,
) -> DeviceCollectionResult {
    match snapshots {
        Ok(snapshots) => {
            let mut fields = Vec::new();
            let collection = snapshots
                .into_iter()
                .enumerate()
                .map(|(index, snapshot)| map_device(snapshot, index, &mut fields))
                .collect();
            DeviceCollectionResult {
                collection: Some(collection),
                status: CollectorResult {
                    name: CollectorName::Devices,
                    status: if fields.is_empty() {
                        CollectorStatus::Success
                    } else {
                        CollectorStatus::Partial
                    },
                    duration_ms,
                    messages: vec![],
                    fields,
                },
            }
        }
        Err(failure) => DeviceCollectionResult {
            collection: None,
            status: CollectorResult {
                name: CollectorName::Devices,
                status: CollectorStatus::Failed,
                duration_ms,
                messages: vec![CollectionMessage {
                    code: "setupapi_device_enumeration_failed".into(),
                    native_code: failure.native_code,
                    message: Some("SetupAPIでデバイスを列挙できませんでした".into()),
                }],
                fields: vec![],
            },
        },
    }
}

fn map_device(
    snapshot: DeviceSnapshot,
    index: usize,
    fields: &mut Vec<FieldCollectionResult>,
) -> ConnectedDevice {
    let base = format!("/devices/{index}");
    for (suffix, value, code) in [
        ("name", snapshot.name.is_some(), "device_name_unavailable"),
        (
            "manufacturer",
            snapshot.manufacturer.is_some(),
            "device_manufacturer_unavailable",
        ),
        (
            "class",
            snapshot.class.is_some(),
            "device_class_unavailable",
        ),
        (
            "class_guid",
            snapshot.class_guid.is_some(),
            "device_class_guid_unavailable",
        ),
        (
            "device_instance_id",
            snapshot.device_instance_id.is_some(),
            "device_instance_id_unavailable",
        ),
        (
            "device_state/present",
            snapshot.present.is_some(),
            "device_present_state_unavailable",
        ),
        (
            "device_state/enabled",
            snapshot.enabled.is_some(),
            "device_enabled_state_unavailable",
        ),
        (
            "device_state/problem_code",
            snapshot.problem_code.is_some(),
            "device_problem_code_unavailable",
        ),
        (
            "driver/version",
            snapshot.driver_version.is_some(),
            "device_driver_version_unavailable",
        ),
        (
            "driver/date",
            snapshot.driver_date.is_some(),
            "device_driver_date_unavailable",
        ),
    ] {
        if !value {
            fields.push(FieldCollectionResult {
                path: format!("{base}/{suffix}"),
                status: FieldCollectionStatus::SourceNull,
                code: code.into(),
                native_code: None,
            });
        }
    }

    ConnectedDevice {
        name: snapshot.name,
        manufacturer: snapshot.manufacturer,
        class: snapshot.class,
        class_guid: snapshot.class_guid,
        device_instance_id: snapshot.device_instance_id,
        device_state: DeviceState {
            present: snapshot.present,
            enabled: snapshot.enabled,
            problem_code: snapshot.problem_code,
        },
        driver: DeviceDriver {
            version: snapshot.driver_version,
            date: snapshot.driver_date,
        },
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(windows)]
mod platform {
    use std::mem::size_of;

    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        DIGCF_ALLCLASSES, DN_STARTED, HDEVINFO, SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList,
        SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDevicePropertyW,
    };
    use windows::Win32::Devices::Properties::{
        DEVPKEY_Device_Class, DEVPKEY_Device_DevNodeStatus, DEVPKEY_Device_DeviceDesc,
        DEVPKEY_Device_DriverDate, DEVPKEY_Device_DriverVersion, DEVPKEY_Device_FriendlyName,
        DEVPKEY_Device_InstanceId, DEVPKEY_Device_IsPresent, DEVPKEY_Device_Manufacturer,
        DEVPKEY_Device_ProblemCode, DEVPROPTYPE,
    };
    use windows::Win32::Foundation::{DEVPROPKEY, ERROR_NO_MORE_ITEMS, FILETIME, SYSTEMTIME};
    use windows::Win32::System::Time::FileTimeToSystemTime;
    use windows::core::PCWSTR;

    use super::{DeviceSnapshot, EnumerationFailure};

    struct DeviceInfoSet(HDEVINFO);

    impl Drop for DeviceInfoSet {
        fn drop(&mut self) {
            // SAFETY: The handle was returned by SetupDiGetClassDevsW and is released once.
            let _ = unsafe { SetupDiDestroyDeviceInfoList(self.0) };
        }
    }

    pub(super) fn enumerate_devices() -> Result<Vec<DeviceSnapshot>, EnumerationFailure> {
        // SAFETY: No class filter is supplied, and DIGCF_ALLCLASSES requests all installed devices.
        let info = DeviceInfoSet(
            unsafe { SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_ALLCLASSES) }
                .map_err(failure)?,
        );
        let mut devices = Vec::new();

        for index in 0.. {
            let mut data = SP_DEVINFO_DATA {
                cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            // SAFETY: The device info set is valid and data has the required initialized size.
            match unsafe { SetupDiEnumDeviceInfo(info.0, index, &mut data) } {
                Ok(()) => {}
                Err(error) if error.code() == ERROR_NO_MORE_ITEMS.to_hresult() => break,
                Err(error) => return Err(failure(error)),
            }

            let status = property_u32(info.0, &data, &DEVPKEY_Device_DevNodeStatus);
            devices.push(DeviceSnapshot {
                name: property_string(info.0, &data, &DEVPKEY_Device_FriendlyName)
                    .or_else(|| property_string(info.0, &data, &DEVPKEY_Device_DeviceDesc)),
                manufacturer: property_string(info.0, &data, &DEVPKEY_Device_Manufacturer),
                class: property_string(info.0, &data, &DEVPKEY_Device_Class),
                class_guid: Some(format!("{{{}}}", data.ClassGuid)),
                device_instance_id: property_string(info.0, &data, &DEVPKEY_Device_InstanceId),
                present: property_bool(info.0, &data, &DEVPKEY_Device_IsPresent),
                enabled: status.map(|value| value & DN_STARTED.0 != 0),
                problem_code: property_u32(info.0, &data, &DEVPKEY_Device_ProblemCode),
                driver_version: property_string(info.0, &data, &DEVPKEY_Device_DriverVersion),
                driver_date: property_date(info.0, &data, &DEVPKEY_Device_DriverDate),
            });
        }

        Ok(devices)
    }

    fn property_bytes(info: HDEVINFO, data: &SP_DEVINFO_DATA, key: &DEVPROPKEY) -> Option<Vec<u8>> {
        let mut property_type = DEVPROPTYPE::default();
        let mut required_size = 0;
        // SAFETY: The first call intentionally supplies no buffer to obtain its required size.
        let _ = unsafe {
            SetupDiGetDevicePropertyW(
                info,
                data,
                key,
                &mut property_type,
                None,
                Some(&mut required_size),
                0,
            )
        };
        if required_size == 0 {
            return None;
        }
        let mut buffer = vec![0; required_size as usize];
        // SAFETY: The buffer has the size reported by SetupDiGetDevicePropertyW.
        unsafe {
            SetupDiGetDevicePropertyW(
                info,
                data,
                key,
                &mut property_type,
                Some(buffer.as_mut_slice()),
                Some(&mut required_size),
                0,
            )
        }
        .ok()?;
        buffer.truncate(required_size as usize);
        Some(buffer)
    }

    fn property_string(info: HDEVINFO, data: &SP_DEVINFO_DATA, key: &DEVPROPKEY) -> Option<String> {
        let bytes = property_bytes(info, data, key)?;
        let utf16: Vec<_> = bytes
            .chunks_exact(2)
            .map(|value| u16::from_le_bytes([value[0], value[1]]))
            .take_while(|value| *value != 0)
            .collect();
        let value = String::from_utf16(&utf16).ok()?;
        (!value.is_empty()).then_some(value)
    }

    fn property_u32(info: HDEVINFO, data: &SP_DEVINFO_DATA, key: &DEVPROPKEY) -> Option<u32> {
        let bytes = property_bytes(info, data, key)?;
        Some(u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?))
    }

    fn property_bool(info: HDEVINFO, data: &SP_DEVINFO_DATA, key: &DEVPROPKEY) -> Option<bool> {
        Some(*property_bytes(info, data, key)?.first()? != 0)
    }

    fn property_date(info: HDEVINFO, data: &SP_DEVINFO_DATA, key: &DEVPROPKEY) -> Option<String> {
        let bytes = property_bytes(info, data, key)?;
        let file_time = FILETIME {
            dwLowDateTime: u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?),
            dwHighDateTime: u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?),
        };
        let mut system_time = SYSTEMTIME::default();
        // SAFETY: Both structures are valid and fully initialized.
        unsafe { FileTimeToSystemTime(&file_time, &mut system_time) }.ok()?;
        Some(format!(
            "{:04}-{:02}-{:02}",
            system_time.wYear, system_time.wMonth, system_time.wDay
        ))
    }

    fn failure(error: windows::core::Error) -> EnumerationFailure {
        EnumerationFailure {
            native_code: Some(i64::from(error.code().0)),
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{DeviceSnapshot, EnumerationFailure};

    pub(super) fn enumerate_devices() -> Result<Vec<DeviceSnapshot>, EnumerationFailure> {
        Err(EnumerationFailure { native_code: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_device_snapshot_to_shared_model() {
        let result = build_result(Ok(vec![snapshot()]), 5);

        assert_eq!(result.status.status, CollectorStatus::Success);
        assert_eq!(
            result.collection.unwrap()[0].device_state.present,
            Some(true)
        );
    }

    #[test]
    fn missing_property_makes_collection_partial() {
        let mut device = snapshot();
        device.driver_date = None;
        let result = build_result(Ok(vec![device]), 2);

        assert_eq!(result.status.status, CollectorStatus::Partial);
        assert_eq!(result.status.fields[0].path, "/devices/0/driver/date");
    }

    fn snapshot() -> DeviceSnapshot {
        DeviceSnapshot {
            name: Some("Example Device".into()),
            manufacturer: Some("Example Vendor".into()),
            class: Some("USB".into()),
            class_guid: Some("{00000000-0000-0000-0000-000000000000}".into()),
            device_instance_id: Some("USB\\VID_1234&PID_5678\\TEST".into()),
            present: Some(true),
            enabled: Some(true),
            problem_code: Some(0),
            driver_version: Some("1.2.3.4".into()),
            driver_date: Some("2026-07-17".into()),
        }
    }
}
