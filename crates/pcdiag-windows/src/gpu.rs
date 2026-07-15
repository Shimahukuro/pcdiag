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

    for (suffix, code) in [
        ("device_instance_id", "device_instance_id_unavailable"),
        ("driver/version", "driver_version_unavailable"),
        ("driver/date", "driver_date_unavailable"),
        ("device_state/problem_code", "problem_code_unavailable"),
    ] {
        unavailable(
            fields,
            format!("{base}/{suffix}"),
            code,
            FieldCollectionStatus::Unsupported,
        );
    }

    Gpu {
        name: nonempty(snapshot.name),
        vendor,
        adapter_type: snapshot.adapter_type,
        device_instance_id: None,
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
            version: None,
            date: None,
        },
        device_state: GpuDeviceState {
            present: Some(true),
            enabled: Some(true),
            problem_code: None,
        },
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
    use pcdiag_core::GpuAdapterType;
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, DXGI_ADAPTER_FLAG_REMOTE, DXGI_ADAPTER_FLAG_SOFTWARE,
        DXGI_ERROR_NOT_FOUND, IDXGIFactory1,
    };

    use super::{AdapterSnapshot, EnumerationFailure};

    pub(super) fn enumerate_adapters() -> Result<Vec<AdapterSnapshot>, EnumerationFailure> {
        // SAFETY: CreateDXGIFactory1 initializes and returns an owned COM interface.
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
            .map_err(|error| windows_failure("dxgi_factory_creation_failed", error))?;
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
            });
        }

        Ok(snapshots)
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
        }
    }
}
