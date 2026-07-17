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

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumerationSuccess {
    snapshots: Vec<AdapterSnapshot>,
    messages: Vec<CollectionMessage>,
}

#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PciIdentity {
    vendor_id: u32,
    device_id: u32,
    subsystem_id: u32,
    revision_id: u32,
}

#[cfg(any(windows, test))]
fn parse_pci_identity(device_instance_id: &str) -> Option<PciIdentity> {
    let value = device_instance_id.to_ascii_uppercase();
    let mut vendor_id = None;
    let mut device_id = None;
    let mut subsystem_id = None;
    let mut revision_id = None;

    for component in value.split(['\\', '&']) {
        if let Some(value) = component.strip_prefix("VEN_") {
            vendor_id = parse_hex(value, 4);
        } else if let Some(value) = component.strip_prefix("DEV_") {
            device_id = parse_hex(value, 4);
        } else if let Some(value) = component.strip_prefix("SUBSYS_") {
            subsystem_id = parse_hex(value, 8);
        } else if let Some(value) = component.strip_prefix("REV_") {
            revision_id = parse_hex(value, 2);
        }
    }

    Some(PciIdentity {
        vendor_id: vendor_id?,
        device_id: device_id?,
        subsystem_id: subsystem_id?,
        revision_id: revision_id?,
    })
}

#[cfg(any(windows, test))]
fn parse_hex(value: &str, digits: usize) -> Option<u32> {
    (value.len() == digits)
        .then(|| u32::from_str_radix(value, 16).ok())
        .flatten()
}

pub fn collect_gpus() -> GpuCollectionResult {
    let started = Instant::now();
    build_result(platform::enumerate_adapters(), elapsed_ms(started))
}

fn build_result(
    enumeration: Result<EnumerationSuccess, EnumerationFailure>,
    duration_ms: u64,
) -> GpuCollectionResult {
    match enumeration {
        Ok(enumeration) => {
            let mut fields = Vec::new();
            let collection = enumeration
                .snapshots
                .into_iter()
                .enumerate()
                .map(|(index, snapshot)| map_adapter(snapshot, index, &mut fields))
                .collect();

            GpuCollectionResult {
                collection: Some(collection),
                status: CollectorResult {
                    name: CollectorName::Gpu,
                    status: if enumeration.messages.is_empty()
                        && fields
                            .iter()
                            .all(|field| field.status == FieldCollectionStatus::NotApplicable)
                    {
                        CollectorStatus::Success
                    } else {
                        CollectorStatus::Partial
                    },
                    duration_ms,
                    messages: enumeration.messages,
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
    let software = snapshot.adapter_type == GpuAdapterType::Software;

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
        if software {
            "device_instance_id_not_applicable"
        } else {
            "device_instance_id_unavailable"
        },
        missing_status(software),
    );
    record_missing(
        &snapshot.driver_version,
        fields,
        format!("{base}/driver/version"),
        if software {
            "driver_version_not_applicable"
        } else {
            "driver_version_unavailable"
        },
        missing_status(software),
    );
    record_missing(
        &snapshot.driver_date,
        fields,
        format!("{base}/driver/date"),
        if software {
            "driver_date_not_applicable"
        } else {
            "driver_date_unavailable"
        },
        missing_status(software),
    );
    record_missing(
        &snapshot.enabled,
        fields,
        format!("{base}/device_state/enabled"),
        if software {
            "device_enabled_state_not_applicable"
        } else {
            "device_enabled_state_unavailable"
        },
        missing_status(software),
    );
    record_missing(
        &snapshot.problem_code,
        fields,
        format!("{base}/device_state/problem_code"),
        if software {
            "problem_code_not_applicable"
        } else {
            "problem_code_unavailable"
        },
        missing_status(software),
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
    status: FieldCollectionStatus,
) {
    if value.is_none() {
        unavailable(fields, path, code, status);
    }
}

fn missing_status(software: bool) -> FieldCollectionStatus {
    if software {
        FieldCollectionStatus::NotApplicable
    } else {
        FieldCollectionStatus::SourceNull
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

    use pcdiag_core::{CollectionMessage, GpuAdapterType};
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

    use super::{
        AdapterSnapshot, EnumerationFailure, EnumerationSuccess, PciIdentity, parse_pci_identity,
    };

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct DeviceDetails {
        luid: Option<(u32, i32)>,
        pci: Option<PciIdentity>,
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

    pub(super) fn enumerate_adapters() -> Result<EnumerationSuccess, EnumerationFailure> {
        // SAFETY: CreateDXGIFactory1 initializes and returns an owned COM interface.
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
            .map_err(|error| windows_failure("dxgi_factory_creation_failed", error))?;
        let (device_details, mut messages) = match enumerate_device_details() {
            Ok(result) => result,
            Err(error) => (
                vec![],
                vec![setup_message(
                    "setupapi_device_enumeration_failed",
                    Some(i64::from(error.code().0)),
                    "SetupAPIで表示デバイスを列挙できませんでした".into(),
                )],
            ),
        };
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
            let pci = PciIdentity {
                vendor_id: description.VendorId,
                device_id: description.DeviceId,
                subsystem_id: description.SubSysId,
                revision_id: description.Revision,
            };
            let adapter_type = adapter_type(description.Flags);
            let details = match_device_details(
                (
                    description.AdapterLuid.LowPart,
                    description.AdapterLuid.HighPart,
                ),
                pci,
                &device_details,
                &mut messages,
                index,
                adapter_type,
            );

            snapshots.push(AdapterSnapshot {
                name: String::from_utf16_lossy(&description.Description[..name_end]),
                vendor_id: description.VendorId,
                device_id: description.DeviceId,
                subsystem_id: description.SubSysId,
                revision_id: description.Revision,
                dedicated_video_bytes: description.DedicatedVideoMemory as u64,
                dedicated_system_bytes: description.DedicatedSystemMemory as u64,
                shared_system_bytes: description.SharedSystemMemory as u64,
                adapter_type,
                device_instance_id: details.and_then(|value| value.device_instance_id.clone()),
                driver_version: details.and_then(|value| value.driver_version.clone()),
                driver_date: details.and_then(|value| value.driver_date.clone()),
                enabled: details.and_then(|value| value.enabled),
                problem_code: details.and_then(|value| value.problem_code),
            });
        }

        messages.shrink_to_fit();
        Ok(EnumerationSuccess {
            snapshots,
            messages,
        })
    }

    fn enumerate_device_details()
    -> windows::core::Result<(Vec<DeviceDetails>, Vec<CollectionMessage>)> {
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
        let mut messages = Vec::new();

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

            let device_instance_id = property_string(info.0, &data, &DEVPKEY_Device_InstanceId);
            let pci = device_instance_id.as_deref().and_then(parse_pci_identity);
            let luid = match property_luid(info.0, &data, &DEVPKEY_Device_AdapterLuid) {
                Ok(luid) => Some(luid),
                Err(native_code) => {
                    if pci.is_none() {
                        messages.push(setup_message(
                            "setupapi_device_identity_unavailable",
                            native_code,
                            format!(
                                "表示デバイスのLUIDとPCI識別子を取得できませんでした (index={index})"
                            ),
                        ));
                    }
                    None
                }
            };
            let status = property_u32(info.0, &data, &DEVPKEY_Device_DevNodeStatus);

            devices.push(DeviceDetails {
                luid,
                pci,
                device_instance_id,
                driver_version: property_string(info.0, &data, &DEVPKEY_Device_DriverVersion),
                driver_date: property_date(info.0, &data, &DEVPKEY_Device_DriverDate),
                enabled: status.map(|status| status & DN_STARTED.0 != 0),
                problem_code: property_u32(info.0, &data, &DEVPKEY_Device_ProblemCode),
            });
        }

        Ok((devices, messages))
    }

    fn match_device_details<'a>(
        luid: (u32, i32),
        pci: PciIdentity,
        devices: &'a [DeviceDetails],
        messages: &mut Vec<CollectionMessage>,
        adapter_index: u32,
        adapter_type: GpuAdapterType,
    ) -> Option<&'a DeviceDetails> {
        let luid_matches: Vec<_> = devices
            .iter()
            .filter(|details| details.luid == Some(luid))
            .collect();
        if luid_matches.len() == 1 {
            return luid_matches.into_iter().next();
        }

        let pci_matches: Vec<_> = devices
            .iter()
            .filter(|details| details.pci == Some(pci))
            .collect();
        match pci_matches.len() {
            1 => pci_matches.into_iter().next(),
            0 => {
                if adapter_type == GpuAdapterType::Hardware {
                    messages.push(setup_message(
                        "setupapi_device_match_not_found",
                        None,
                        format!(
                            "DXGIアダプターに一致する表示デバイスがありません (adapter_index={adapter_index})"
                        ),
                    ));
                }
                None
            }
            count => {
                messages.push(setup_message(
                    "setupapi_pci_match_ambiguous",
                    None,
                    format!(
                        "PCI識別子が一致する表示デバイスが複数あります (adapter_index={adapter_index}, candidates={count})"
                    ),
                ));
                None
            }
        }
    }

    fn property_bytes(
        info: HDEVINFO,
        data: &SP_DEVINFO_DATA,
        key: &DEVPROPKEY,
    ) -> Result<Vec<u8>, Option<i64>> {
        let mut property_type = DEVPROPTYPE::default();
        let mut required_size = 0;

        // SAFETY: The first call intentionally supplies no buffer to obtain its required size.
        let size_result = unsafe {
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
            return Err(size_result.err().map(|error| i64::from(error.code().0)));
        }

        let mut buffer = vec![0; required_size as usize];
        // SAFETY: The buffer has the size reported by SetupDiGetDevicePropertyW.
        let result = unsafe {
            SetupDiGetDevicePropertyW(
                info,
                data,
                key,
                &mut property_type,
                Some(buffer.as_mut_slice()),
                Some(&mut required_size),
                0,
            )
        };
        result.map_err(|error| Some(i64::from(error.code().0)))?;
        buffer.truncate(required_size as usize);
        Ok(buffer)
    }

    fn property_string(info: HDEVINFO, data: &SP_DEVINFO_DATA, key: &DEVPROPKEY) -> Option<String> {
        let bytes = property_bytes(info, data, key).ok()?;
        let utf16: Vec<_> = bytes
            .chunks_exact(2)
            .map(|value| u16::from_le_bytes([value[0], value[1]]))
            .take_while(|value| *value != 0)
            .collect();
        let value = String::from_utf16(&utf16).ok()?;
        (!value.is_empty()).then_some(value)
    }

    fn property_u32(info: HDEVINFO, data: &SP_DEVINFO_DATA, key: &DEVPROPKEY) -> Option<u32> {
        let bytes = property_bytes(info, data, key).ok()?;
        Some(u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?))
    }

    fn property_luid(
        info: HDEVINFO,
        data: &SP_DEVINFO_DATA,
        key: &DEVPROPKEY,
    ) -> Result<(u32, i32), Option<i64>> {
        let bytes = property_bytes(info, data, key)?;
        let low = bytes
            .get(..4)
            .and_then(|value| value.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or(None)?;
        let high = bytes
            .get(4..8)
            .and_then(|value| value.try_into().ok())
            .map(i32::from_le_bytes)
            .ok_or(None)?;
        Ok((low, high))
    }

    fn property_date(info: HDEVINFO, data: &SP_DEVINFO_DATA, key: &DEVPROPKEY) -> Option<String> {
        let bytes = property_bytes(info, data, key).ok()?;
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

    fn setup_message(code: &str, native_code: Option<i64>, message: String) -> CollectionMessage {
        CollectionMessage {
            code: code.into(),
            native_code,
            message: Some(message),
        }
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
    use super::{EnumerationFailure, EnumerationSuccess};

    pub(super) fn enumerate_adapters() -> Result<EnumerationSuccess, EnumerationFailure> {
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
    fn parses_pci_identity_from_a_device_instance_id() {
        let identity =
            parse_pci_identity("PCI\\VEN_10DE&DEV_1FB9&SUBSYS_09061028&REV_A1\\4&123456&0&0008")
                .unwrap();

        assert_eq!(identity.vendor_id, 0x10DE);
        assert_eq!(identity.device_id, 0x1FB9);
        assert_eq!(identity.subsystem_id, 0x0906_1028);
        assert_eq!(identity.revision_id, 0xA1);
    }

    #[test]
    fn incomplete_pci_identity_is_rejected() {
        assert!(parse_pci_identity("PCI\\VEN_10DE&DEV_1FB9\\TEST").is_none());
    }

    #[test]
    fn maps_dxgi_adapter_to_the_shared_gpu_model() {
        let result = build_result(success(vec![snapshot()]), 2);
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
        let result = build_result(success(vec![]), 1);

        assert_eq!(result.collection, Some(vec![]));
        assert_eq!(result.status.status, CollectorStatus::Success);
    }

    #[test]
    fn setupapi_messages_are_preserved_in_the_collector_result() {
        let result = build_result(
            Ok(EnumerationSuccess {
                snapshots: vec![],
                messages: vec![CollectionMessage {
                    code: "setupapi_device_enumeration_failed".into(),
                    native_code: Some(-1),
                    message: Some("failed".into()),
                }],
            }),
            1,
        );

        assert_eq!(result.status.status, CollectorStatus::Partial);
        assert_eq!(result.status.messages.len(), 1);
        assert_eq!(
            result.status.messages[0].code,
            "setupapi_device_enumeration_failed"
        );
    }

    #[test]
    fn setupapi_details_complete_the_gpu_result() {
        let mut adapter = snapshot();
        adapter.device_instance_id = Some("PCI\\VEN_10DE&DEV_2684\\TEST".into());
        adapter.driver_version = Some("32.0.15.1234".into());
        adapter.driver_date = Some("2026-07-15".into());
        adapter.enabled = Some(true);
        adapter.problem_code = Some(0);

        let result = build_result(success(vec![adapter]), 3);
        let gpu = &result.collection.as_ref().unwrap()[0];

        assert_eq!(result.status.status, CollectorStatus::Success);
        assert!(result.status.fields.is_empty());
        assert_eq!(gpu.driver.version.as_deref(), Some("32.0.15.1234"));
        assert_eq!(gpu.device_state.problem_code, Some(0));
    }

    #[test]
    fn software_adapter_fields_are_not_applicable_and_do_not_make_collection_partial() {
        let mut adapter = snapshot();
        adapter.name = "Microsoft Basic Render Driver".into();
        adapter.vendor_id = 0x1414;
        adapter.adapter_type = GpuAdapterType::Software;

        let result = build_result(success(vec![adapter]), 1);

        assert_eq!(result.status.status, CollectorStatus::Success);
        assert_eq!(result.status.fields.len(), 5);
        assert!(result.status.fields.iter().all(|field| {
            field.status == FieldCollectionStatus::NotApplicable
                && field.code.ends_with("_not_applicable")
        }));
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
        let result = build_result(success(vec![adapter]), 1);

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

    fn success(snapshots: Vec<AdapterSnapshot>) -> Result<EnumerationSuccess, EnumerationFailure> {
        Ok(EnumerationSuccess {
            snapshots,
            messages: vec![],
        })
    }
}
