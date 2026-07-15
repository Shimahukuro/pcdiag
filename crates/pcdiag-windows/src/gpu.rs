use std::time::Instant;

use pcdiag_core::{
    CollectionMessage, CollectorName, CollectorResult, CollectorStatus, FieldCollectionResult,
    FieldCollectionStatus, Gpu, GpuAdapterType, GpuDeviceState, GpuDriver, GpuMemory,
    GpuPciIdentifiers,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuCollectionResult {
    pub collection: Option<Vec<Gpu>>,
    pub status: CollectorResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AdapterSnapshot {
    name: String,
    vendor_id: u32,
    device_id: u32,
    subsystem_id: u32,
    revision_id: u32,
    dedicated_video_bytes: u64,
    dedicated_system_bytes: u64,
    shared_system_bytes: u64,
    adapter_type: GpuAdapterType,
    device_instance_id: Option<String>,
    driver_version: Option<String>,
    driver_date: Option<String>,
    enabled: Option<bool>,
    problem_code: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumerationFailure {
    code: &'static str,
    native_code: Option<i64>,
    message: &'static str,
}

pub fn collect_gpus() -> GpuCollectionResult {
    let started = Instant::now();
    build_result(platform::enumerate_adapters(), elapsed_ms(started))
}

fn build_result(
    snapshots: Result<Vec<AdapterSnapshot>, EnumerationFailure>,
    duration_ms: u64,
) -> GpuCollectionResult {
    match snapshots {
        Ok(snapshots) => {
            let mut fields = Vec::new();
            let collection = snapshots
                .into_iter()
                .enumerate()
                .map(|(index, snapshot)| map_adapter(snapshot, index, &mut fields))
                .collect();

            GpuCollectionResult {
                collection: Some(collection),
                status: CollectorResult {
                    name: CollectorName::Gpu,
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
        Err(failure) => GpuCollectionResult {
            collection: None,
            status: CollectorResult {
                name: CollectorName::Gpu,
                status: CollectorStatus::Failed,
                duration_ms,
                messages: vec![CollectionMessage {
                    code: failure.code.into(),
                    native_code: failure.native_code,
                    message: Some(failure.message.into()),
                }],
                fields: vec![],
            },
        },
    }
}

fn map_adapter(
    snapshot: AdapterSnapshot,
    index: usize,
    fields: &mut Vec<FieldCollectionResult>,
) -> Gpu {
    let base = format!("/gpus/{index}");
    let vendor_id = checked_pci_u16(snapshot.vendor_id, format!("{base}/pci/vendor_id"), fields);
    let device_id = checked_pci_u16(snapshot.device_id, format!("{base}/pci/device_id"), fields);
    let revision_id = checked_revision(snapshot.revision_id, &base, fields);
    let vendor = vendor_id.and_then(vendor_name).map(str::to_owned);

    if vendor.is_none() {
        unavailable(
            fields,
            format!("{base}/vendor"),
            "vendor_name_unknown",
            FieldCollectionStatus::SourceNull,
        );
    }

    record_missing(
        &snapshot.device_instance_id,
        fields,
        format!("{base}/device_instance_id"),
        "device_instance_id_unavailable",
    );
    record_missing(
        &snapshot.driver_version,
        fields,
        format!("{base}/driver/version"),
        "driver_version_unavailable",
    );
    record_missing(
        &snapshot.driver_date,
        fields,
        format!("{base}/driver/date"),
        "driver_date_unavailable",
    );
    record_missing(
        &snapshot.enabled,
        fields,
        format!("{base}/device_state/enabled"),
        "device_enabled_state_unavailable",
    );
    record_missing(
        &snapshot.problem_code,
        fields,
        format!("{base}/device_state/problem_code"),
        "problem_code_unavailable",
    );

    Gpu {
        name: nonempty(snapshot.name),
        vendor,
        adapter_type: snapshot.adapter_type,
        device_instance_id: snapshot.device_instance_id,
        pci: GpuPciIdentifiers {
            vendor_id,
            device_id,
            subsystem_id: Some(snapshot.subsystem_id),
            revision_id,
        },
        memory: GpuMemory {
            dedicated_video_bytes: Some(snapshot.dedicated_video_bytes),
            dedicated_system_bytes: Some(snapshot.dedicated_system_bytes),
            shared_system_bytes: Some(snapshot.shared_system_bytes),
        },
        driver: GpuDriver {
            version: snapshot.driver_version,
            date: snapshot.driver_date,
        },
        device_state: GpuDeviceState {
            present: Some(true),
            enabled: snapshot.enabled,
            problem_code: snapshot.problem_code,
        },
    }
}

fn record_missing<T>(
    value: &Option<T>,
    fields: &mut Vec<FieldCollectionResult>,
    path: String,
    code: &str,
) {
    if value.is_none() {
        unavailable(fields, path, code, FieldCollectionStatus::SourceNull);
    }
}

fn checked_pci_u16(
    value: u32,
    path: String,
    fields: &mut Vec<FieldCollectionResult>,
) -> Option<u16> {
    match u16::try_from(value) {
        Ok(value) => Some(value),
        Err(_) => {
            unavailable(
                fields,
                path,
                "identifier_is_not_a_pci_id",
                FieldCollectionStatus::InvalidValue,
            );
            None
        }
    }
}

fn checked_revision(value: u32, base: &str, fields: &mut Vec<FieldCollectionResult>) -> Option<u8> {
    match u8::try_from(value) {
        Ok(value) => Some(value),
        Err(_) => {
            unavailable(
                fields,
                format!("{base}/pci/revision_id"),
                "revision_id_out_of_range",
                FieldCollectionStatus::InvalidValue,
            );
            None
        }
    }
}

fn unavailable(
    fields: &mut Vec<FieldCollectionResult>,
    path: String,
    code: &str,
    status: FieldCollectionStatus,
) {
    fields.push(FieldCollectionResult {
        path,
        status,
        code: code.into(),
        native_code: None,
    });
}

fn nonempty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn vendor_name(vendor_id: u16) -> Option<&'static str> {
    match vendor_id {
        0x1002 | 0x1022 => Some("AMD"),
        0x10DE => Some("NVIDIA"),
        0x1414 => Some("Microsoft"),
        0x17CB => Some("Qualcomm"),
        0x8086 => Some("Intel"),
        _ => None,
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(windows)]
mod platform {
    use std::mem::size_of;

    use pcdiag_core::GpuAdapterType;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        DIGCF_PRESENT, DN_STARTED, GUID_DEVCLASS_DISPLAY, HDEVINFO, SP_DEVINFO_DATA,
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
        SetupDiGetDevicePropertyW,
    };
    use windows::Win32::Devices::Display::DEVPKEY_Device_AdapterLuid;
    use windows::Win32::Devices::Properties::{
        DEVPKEY_Device_DevNodeStatus, DEVPKEY_Device_DriverDate, DEVPKEY_Device_DriverVersion,
        DEVPKEY_Device_InstanceId, DEVPKEY_Device_ProblemCode, DEVPROPTYPE,
    };
    use windows::Win32::Foundation::{DEVPROPKEY, ERROR_NO_MORE_ITEMS, FILETIME, SYSTEMTIME};
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, DXGI_ADAPTER_FLAG_REMOTE, DXGI_ADAPTER_FLAG_SOFTWARE,
        DXGI_ERROR_NOT_FOUND, IDXGIFactory1,
    };
    use windows::Win32::System::Time::FileTimeToSystemTime;
    use windows::core::PCWSTR;

    use super::{AdapterSnapshot, EnumerationFailure};

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct DeviceDetails {
        luid: (u32, i32),
        device_instance_id: Option<String>,
        driver_version: Option<String>,
        driver_date: Option<String>,
        enabled: Option<bool>,
        problem_code: Option<u32>,
    }

    struct DeviceInfoSet(HDEVINFO);

    impl Drop for DeviceInfoSet {
        fn drop(&mut self) {
            // SAFETY: The handle was returned by SetupDiGetClassDevsW and is released once.
            let _ = unsafe { SetupDiDestroyDeviceInfoList(self.0) };
        }
    }

    pub(super) fn enumerate_adapters() -> Result<Vec<AdapterSnapshot>, EnumerationFailure> {
        // SAFETY: CreateDXGIFactory1 initializes and returns an owned COM interface.
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
            .map_err(|error| windows_failure("dxgi_factory_creation_failed", error))?;
        let device_details = enumerate_device_details().unwrap_or_default();
        let mut snapshots = Vec::new();

        for index in 0.. {
            // SAFETY: The factory is valid and the generated binding owns the returned adapter.
            let adapter = match unsafe { factory.EnumAdapters1(index) } {
                Ok(adapter) => adapter,
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(error) => {
                    return Err(windows_failure("dxgi_adapter_enumeration_failed", error));
                }
            };

            // SAFETY: The adapter is a valid IDXGIAdapter1 returned by the factory.
            let description = unsafe { adapter.GetDesc1() }
                .map_err(|error| windows_failure("dxgi_adapter_description_failed", error))?;
            let name_end = description
                .Description
                .iter()
                .position(|character| *character == 0)
                .unwrap_or(description.Description.len());
            let details = device_details.iter().find(|details| {
                details.luid
                    == (
                        description.AdapterLuid.LowPart,
                        description.AdapterLuid.HighPart,
                    )
            });

            snapshots.push(AdapterSnapshot {
                name: String::from_utf16_lossy(&description.Description[..name_end]),
                vendor_id: description.VendorId,
                device_id: description.DeviceId,
                subsystem_id: description.SubSysId,
                revision_id: description.Revision,
                dedicated_video_bytes: description.DedicatedVideoMemory as u64,
                dedicated_system_bytes: description.DedicatedSystemMemory as u64,
                shared_system_bytes: description.SharedSystemMemory as u64,
                adapter_type: adapter_type(description.Flags),
                device_instance_id: details.and_then(|value| value.device_instance_id.clone()),
                driver_version: details.and_then(|value| value.driver_version.clone()),
                driver_date: details.and_then(|value| value.driver_date.clone()),
                enabled: details.and_then(|value| value.enabled),
                problem_code: details.and_then(|value| value.problem_code),
            });
        }

        Ok(snapshots)
    }

    fn enumerate_device_details() -> windows::core::Result<Vec<DeviceDetails>> {
        // SAFETY: The class GUID and flags are valid; no parent window or enumerator is required.
        let info = DeviceInfoSet(unsafe {
            SetupDiGetClassDevsW(
                Some(&GUID_DEVCLASS_DISPLAY),
                PCWSTR::null(),
                None,
                DIGCF_PRESENT,
            )?
        });
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
                Err(error) => return Err(error),
            }

            let Some(luid) = property_luid(info.0, &data, &DEVPKEY_Device_AdapterLuid) else {
                continue;
            };
            let status = property_u32(info.0, &data, &DEVPKEY_Device_DevNodeStatus);

            devices.push(DeviceDetails {
                luid,
                device_instance_id: property_string(info.0, &data, &DEVPKEY_Device_InstanceId),
                driver_version: property_string(info.0, &data, &DEVPKEY_Device_DriverVersion),
                driver_date: property_date(info.0, &data, &DEVPKEY_Device_DriverDate),
                enabled: status.map(|status| status & DN_STARTED.0 != 0),
                problem_code: property_u32(info.0, &data, &DEVPKEY_Device_ProblemCode),
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

    fn property_luid(
        info: HDEVINFO,
        data: &SP_DEVINFO_DATA,
        key: &DEVPROPKEY,
    ) -> Option<(u32, i32)> {
        let bytes = property_bytes(info, data, key)?;
        Some((
            u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?),
            i32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?),
        ))
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

    fn adapter_type(flags: u32) -> GpuAdapterType {
        if flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
            GpuAdapterType::Software
        } else if flags & DXGI_ADAPTER_FLAG_REMOTE.0 as u32 != 0 {
            GpuAdapterType::Remote
        } else {
            GpuAdapterType::Hardware
        }
    }

    fn windows_failure(code: &'static str, error: windows::core::Error) -> EnumerationFailure {
        EnumerationFailure {
            code,
            native_code: Some(i64::from(error.code().0)),
            message: "WindowsからGPU情報を取得できませんでした",
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{AdapterSnapshot, EnumerationFailure};

    pub(super) fn enumerate_adapters() -> Result<Vec<AdapterSnapshot>, EnumerationFailure> {
        Err(EnumerationFailure {
            code: "platform_not_supported",
            native_code: None,
            message: "Windows以外の環境ではGPU情報を収集できません",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_dxgi_adapter_to_the_shared_gpu_model() {
        let result = build_result(Ok(vec![snapshot()]), 2);
        let gpu = &result.collection.as_ref().unwrap()[0];

        assert_eq!(result.status.status, CollectorStatus::Partial);
        assert_eq!(result.status.duration_ms, 2);
        assert_eq!(gpu.name.as_deref(), Some("Example GPU"));
        assert_eq!(gpu.vendor.as_deref(), Some("NVIDIA"));
        assert_eq!(gpu.adapter_type, GpuAdapterType::Hardware);
        assert_eq!(gpu.pci.vendor_id, Some(0x10DE));
        assert_eq!(gpu.memory.dedicated_video_bytes, Some(8_589_934_592));
        assert!(
            result
                .status
                .fields
                .iter()
                .any(|field| field.path == "/gpus/0/driver/date")
        );
    }

    #[test]
    fn successful_empty_enumeration_is_not_a_failure() {
        let result = build_result(Ok(vec![]), 1);

        assert_eq!(result.collection, Some(vec![]));
        assert_eq!(result.status.status, CollectorStatus::Success);
    }

    #[test]
    fn setupapi_details_complete_the_gpu_result() {
        let mut adapter = snapshot();
        adapter.device_instance_id = Some("PCI\\VEN_10DE&DEV_2684\\TEST".into());
        adapter.driver_version = Some("32.0.15.1234".into());
        adapter.driver_date = Some("2026-07-15".into());
        adapter.enabled = Some(true);
        adapter.problem_code = Some(0);

        let result = build_result(Ok(vec![adapter]), 3);
        let gpu = &result.collection.as_ref().unwrap()[0];

        assert_eq!(result.status.status, CollectorStatus::Success);
        assert!(result.status.fields.is_empty());
        assert_eq!(gpu.driver.version.as_deref(), Some("32.0.15.1234"));
        assert_eq!(gpu.device_state.problem_code, Some(0));
    }

    #[test]
    fn enumeration_failure_produces_null_collection_and_a_reason() {
        let result = build_result(
            Err(EnumerationFailure {
                code: "dxgi_factory_creation_failed",
                native_code: Some(-1),
                message: "failed",
            }),
            4,
        );

        assert_eq!(result.collection, None);
        assert_eq!(result.status.status, CollectorStatus::Failed);
        assert_eq!(result.status.messages[0].native_code, Some(-1));
    }

    #[test]
    fn non_pci_identifier_is_recorded_as_an_invalid_value() {
        let mut adapter = snapshot();
        adapter.vendor_id = 0x1_0000;
        let result = build_result(Ok(vec![adapter]), 1);

        assert_eq!(result.collection.unwrap()[0].pci.vendor_id, None);
        assert!(result.status.fields.iter().any(|field| {
            field.path == "/gpus/0/pci/vendor_id"
                && field.status == FieldCollectionStatus::InvalidValue
        }));
    }

    fn snapshot() -> AdapterSnapshot {
        AdapterSnapshot {
            name: "Example GPU".into(),
            vendor_id: 0x10DE,
            device_id: 0x2684,
            subsystem_id: 0x0000_0001,
            revision_id: 0xA1,
            dedicated_video_bytes: 8_589_934_592,
            dedicated_system_bytes: 0,
            shared_system_bytes: 34_210_639_872,
            adapter_type: GpuAdapterType::Hardware,
            device_instance_id: None,
            driver_version: None,
            driver_date: None,
            enabled: None,
            problem_code: None,
        }
    }
}
