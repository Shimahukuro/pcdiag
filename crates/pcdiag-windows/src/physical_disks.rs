use std::time::Instant;

use pcdiag_core::{
    CollectionMessage, CollectorName, CollectorResult, CollectorStatus, DiskBusType,
    FieldCollectionResult, FieldCollectionStatus, PhysicalDisk,
};

#[cfg(windows)]
pub(crate) use platform::{enumerate_disk_numbers, open_disk};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalDiskCollectionResult {
    pub collection: Option<Vec<PhysicalDisk>>,
    pub status: CollectorResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiskSnapshot {
    number: u32,
    model: Option<String>,
    manufacturer: Option<String>,
    firmware_revision: Option<String>,
    bus_type: Option<DiskBusType>,
    capacity_bytes: Option<u64>,
    logical_sector_size_bytes: Option<u32>,
    removable: Option<bool>,
    failures: Vec<PropertyFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PropertyFailure {
    suffix: &'static str,
    code: &'static str,
    native_code: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumerationResult {
    disks: Vec<DiskSnapshot>,
    messages: Vec<CollectionMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumerationFailure {
    native_code: Option<i64>,
}

pub fn collect_physical_disks() -> PhysicalDiskCollectionResult {
    let started = Instant::now();
    build_result(platform::enumerate_disks(), elapsed_ms(started))
}

fn build_result(
    result: Result<EnumerationResult, EnumerationFailure>,
    duration_ms: u64,
) -> PhysicalDiskCollectionResult {
    match result {
        Ok(result) => {
            let mut fields = Vec::new();
            let collection = result
                .disks
                .into_iter()
                .enumerate()
                .map(|(index, snapshot)| map_disk(snapshot, index, &mut fields))
                .collect();
            let status = if result.messages.is_empty() && fields.is_empty() {
                CollectorStatus::Success
            } else {
                CollectorStatus::Partial
            };
            PhysicalDiskCollectionResult {
                collection: Some(collection),
                status: CollectorResult {
                    name: CollectorName::PhysicalDisks,
                    status,
                    duration_ms,
                    messages: result.messages,
                    fields,
                },
            }
        }
        Err(failure) => PhysicalDiskCollectionResult {
            collection: None,
            status: CollectorResult {
                name: CollectorName::PhysicalDisks,
                status: CollectorStatus::Failed,
                duration_ms,
                messages: vec![CollectionMessage {
                    code: "physical_disk_enumeration_failed".into(),
                    native_code: failure.native_code,
                    message: Some("物理ディスクを列挙できませんでした".into()),
                }],
                fields: vec![],
            },
        },
    }
}

fn map_disk(
    snapshot: DiskSnapshot,
    index: usize,
    fields: &mut Vec<FieldCollectionResult>,
) -> PhysicalDisk {
    let base = format!("/storage/disks/{index}");
    for (suffix, available, code) in [
        ("model", snapshot.model.is_some(), "disk_model_unavailable"),
        (
            "manufacturer",
            snapshot.manufacturer.is_some(),
            "disk_manufacturer_unavailable",
        ),
        (
            "firmware_revision",
            snapshot.firmware_revision.is_some(),
            "disk_firmware_revision_unavailable",
        ),
        (
            "bus_type",
            snapshot.bus_type.is_some(),
            "disk_bus_type_unavailable",
        ),
        (
            "capacity_bytes",
            snapshot.capacity_bytes.is_some(),
            "disk_capacity_unavailable",
        ),
        (
            "logical_sector_size_bytes",
            snapshot.logical_sector_size_bytes.is_some(),
            "disk_logical_sector_size_unavailable",
        ),
        (
            "removable",
            snapshot.removable.is_some(),
            "disk_removable_state_unavailable",
        ),
    ] {
        if !available {
            let failure = snapshot
                .failures
                .iter()
                .find(|failure| failure.suffix == suffix);
            fields.push(FieldCollectionResult {
                path: format!("{base}/{suffix}"),
                status: if failure.is_some() {
                    FieldCollectionStatus::Failed
                } else {
                    FieldCollectionStatus::SourceNull
                },
                code: failure.map_or(code, |failure| failure.code).into(),
                native_code: failure.and_then(|failure| failure.native_code),
            });
        }
    }

    PhysicalDisk {
        number: snapshot.number,
        model: snapshot.model,
        manufacturer: snapshot.manufacturer,
        firmware_revision: snapshot.firmware_revision,
        bus_type: snapshot.bus_type,
        capacity_bytes: snapshot.capacity_bytes,
        logical_sector_size_bytes: snapshot.logical_sector_size_bytes,
        removable: snapshot.removable,
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(windows)]
mod platform {
    use std::{ffi::c_void, mem::size_of, ptr::read_unaligned};

    use windows::Win32::Foundation::{
        CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER, ERROR_PATH_NOT_FOUND,
        GetLastError, HANDLE,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        QueryDosDeviceW,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{
        DISK_GEOMETRY_EX, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, IOCTL_STORAGE_QUERY_PROPERTY,
        PropertyStandardQuery, STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY,
        StorageDeviceProperty,
    };
    use windows::core::PCWSTR;

    use super::{
        CollectionMessage, DiskBusType, DiskSnapshot, EnumerationFailure, EnumerationResult,
        PropertyFailure,
    };

    const IOCTL_BUFFER_SIZE: usize = 4096;

    pub(crate) struct OwnedHandle(pub(crate) HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            // SAFETY: The handle was returned by CreateFileW and is closed exactly once.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    pub(super) fn enumerate_disks() -> Result<EnumerationResult, EnumerationFailure> {
        let mut disks = Vec::new();
        let mut messages = Vec::new();

        let numbers = enumerate_disk_numbers().map_err(|error| EnumerationFailure {
            native_code: Some(i64::from(error.code().0)),
        })?;
        for number in numbers {
            match open_disk(number) {
                Ok(handle) => disks.push(query_disk(handle.0, number)),
                Err(error)
                    if error.code() == ERROR_FILE_NOT_FOUND.to_hresult()
                        || error.code() == ERROR_PATH_NOT_FOUND.to_hresult() => {}
                Err(error) => messages.push(CollectionMessage {
                    code: "physical_disk_open_failed".into(),
                    native_code: Some(i64::from(error.code().0)),
                    message: Some(format!("PhysicalDrive{number}を開けませんでした")),
                }),
            }
        }

        Ok(EnumerationResult { disks, messages })
    }

    pub(crate) fn enumerate_disk_numbers() -> windows::core::Result<Vec<u32>> {
        let mut capacity = 4096;
        loop {
            let mut buffer = vec![0_u16; capacity];
            // SAFETY: A null device name requests the complete DOS device name list and the
            // supplied UTF-16 buffer is writable for its full length.
            let length = unsafe { QueryDosDeviceW(PCWSTR::null(), Some(&mut buffer)) };
            if length != 0 {
                let mut numbers: Vec<_> = buffer[..length as usize]
                    .split(|value| *value == 0)
                    .filter_map(|name| String::from_utf16(name).ok())
                    .filter_map(|name| name.strip_prefix("PhysicalDrive")?.parse().ok())
                    .collect();
                numbers.sort_unstable();
                numbers.dedup();
                return Ok(numbers);
            }

            // SAFETY: QueryDosDeviceW failed immediately before this call.
            let error = windows::core::Error::from_hresult(unsafe { GetLastError() }.to_hresult());
            if error.code() != ERROR_INSUFFICIENT_BUFFER.to_hresult() {
                return Err(error);
            }
            capacity = capacity.checked_mul(2).ok_or(error)?;
        }
    }

    pub(crate) fn open_disk(number: u32) -> windows::core::Result<OwnedHandle> {
        let path: Vec<u16> = format!(r"\\.\PhysicalDrive{number}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: The path is null-terminated and all flags are valid for a read-only query handle.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )?
        };
        Ok(OwnedHandle(handle))
    }

    fn query_disk(handle: HANDLE, number: u32) -> DiskSnapshot {
        let mut snapshot = DiskSnapshot {
            number,
            model: None,
            manufacturer: None,
            firmware_revision: None,
            bus_type: None,
            capacity_bytes: None,
            logical_sector_size_bytes: None,
            removable: None,
            failures: Vec::new(),
        };

        match query_descriptor(handle) {
            Ok(descriptor) => {
                snapshot.model = descriptor.model;
                snapshot.manufacturer = descriptor.manufacturer;
                snapshot.firmware_revision = descriptor.firmware_revision;
                snapshot.bus_type = Some(map_bus_type(descriptor.bus_type));
                snapshot.removable = Some(descriptor.removable);
            }
            Err(error) => {
                let native_code = Some(i64::from(error.code().0));
                for (suffix, code) in [
                    ("model", "storage_descriptor_query_failed"),
                    ("manufacturer", "storage_descriptor_query_failed"),
                    ("firmware_revision", "storage_descriptor_query_failed"),
                    ("bus_type", "storage_descriptor_query_failed"),
                    ("removable", "storage_descriptor_query_failed"),
                ] {
                    snapshot.failures.push(PropertyFailure {
                        suffix,
                        code,
                        native_code,
                    });
                }
            }
        }

        match query_geometry(handle) {
            Ok((capacity, sector_size)) => {
                snapshot.capacity_bytes = capacity;
                snapshot.logical_sector_size_bytes = sector_size;
            }
            Err(error) => {
                let native_code = Some(i64::from(error.code().0));
                for (suffix, code) in [
                    ("capacity_bytes", "disk_geometry_query_failed"),
                    ("logical_sector_size_bytes", "disk_geometry_query_failed"),
                ] {
                    snapshot.failures.push(PropertyFailure {
                        suffix,
                        code,
                        native_code,
                    });
                }
            }
        }

        snapshot
    }

    struct DescriptorValues {
        model: Option<String>,
        manufacturer: Option<String>,
        firmware_revision: Option<String>,
        bus_type: i32,
        removable: bool,
    }

    fn query_descriptor(handle: HANDLE) -> windows::core::Result<DescriptorValues> {
        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        let mut buffer = vec![0_u8; IOCTL_BUFFER_SIZE];
        device_io_control(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some((&raw const query).cast()),
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
        )?;
        // SAFETY: DeviceIoControl initialized at least a STORAGE_DEVICE_DESCRIPTOR on success;
        // read_unaligned is required because Vec<u8> does not guarantee its alignment.
        let descriptor =
            unsafe { read_unaligned(buffer.as_ptr().cast::<STORAGE_DEVICE_DESCRIPTOR>()) };
        Ok(DescriptorValues {
            model: descriptor_string(&buffer, descriptor.ProductIdOffset),
            manufacturer: descriptor_string(&buffer, descriptor.VendorIdOffset),
            firmware_revision: descriptor_string(&buffer, descriptor.ProductRevisionOffset),
            bus_type: descriptor.BusType.0,
            removable: descriptor.RemovableMedia,
        })
    }

    fn query_geometry(handle: HANDLE) -> windows::core::Result<(Option<u64>, Option<u32>)> {
        let mut buffer = vec![0_u8; IOCTL_BUFFER_SIZE];
        device_io_control(
            handle,
            IOCTL_DISK_GET_DRIVE_GEOMETRY_EX,
            None,
            0,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
        )?;
        // SAFETY: DeviceIoControl initialized a DISK_GEOMETRY_EX on success and the value is
        // copied with read_unaligned from the byte buffer.
        let geometry = unsafe { read_unaligned(buffer.as_ptr().cast::<DISK_GEOMETRY_EX>()) };
        Ok((
            u64::try_from(geometry.DiskSize)
                .ok()
                .filter(|value| *value > 0),
            (geometry.Geometry.BytesPerSector > 0).then_some(geometry.Geometry.BytesPerSector),
        ))
    }

    fn device_io_control(
        handle: HANDLE,
        code: u32,
        input: Option<*const c_void>,
        input_size: u32,
        output: *mut c_void,
        output_size: u32,
    ) -> windows::core::Result<()> {
        let mut returned = 0;
        // SAFETY: Input and output pointers refer to initialized buffers of the supplied sizes.
        unsafe {
            DeviceIoControl(
                handle,
                code,
                input,
                input_size,
                Some(output),
                output_size,
                Some(&mut returned),
                None,
            )
        }
    }

    fn descriptor_string(buffer: &[u8], offset: u32) -> Option<String> {
        let start = usize::try_from(offset).ok()?;
        if start == 0 || start >= buffer.len() {
            return None;
        }
        let end = buffer[start..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|length| start + length)
            .unwrap_or(buffer.len());
        let value = String::from_utf8_lossy(&buffer[start..end])
            .trim()
            .to_owned();
        (!value.is_empty()).then_some(value)
    }

    fn map_bus_type(value: i32) -> DiskBusType {
        match value {
            1 => DiskBusType::Scsi,
            2 => DiskBusType::Atapi,
            3 => DiskBusType::Ata,
            4 => DiskBusType::Ieee1394,
            5 => DiskBusType::Ssa,
            6 => DiskBusType::Fibre,
            7 => DiskBusType::Usb,
            8 => DiskBusType::Raid,
            9 => DiskBusType::Iscsi,
            10 => DiskBusType::Sas,
            11 => DiskBusType::Sata,
            12 => DiskBusType::Sd,
            13 => DiskBusType::Mmc,
            14 => DiskBusType::Virtual,
            15 => DiskBusType::FileBackedVirtual,
            16 => DiskBusType::StorageSpaces,
            17 => DiskBusType::Nvme,
            18 => DiskBusType::StorageClassMemory,
            19 => DiskBusType::Ufs,
            _ => DiskBusType::Unknown,
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{EnumerationFailure, EnumerationResult};

    pub(super) fn enumerate_disks() -> Result<EnumerationResult, EnumerationFailure> {
        Err(EnumerationFailure { native_code: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_complete_disk_snapshot() {
        let result = build_result(
            Ok(EnumerationResult {
                disks: vec![snapshot()],
                messages: vec![],
            }),
            4,
        );

        assert_eq!(result.status.status, CollectorStatus::Success);
        let disk = &result.collection.unwrap()[0];
        assert_eq!(disk.number, 2);
        assert_eq!(disk.bus_type, Some(DiskBusType::Usb));
        assert_eq!(disk.capacity_bytes, Some(32_000_000_000));
    }

    #[test]
    fn failed_property_query_preserves_other_values() {
        let mut disk = snapshot();
        disk.capacity_bytes = None;
        disk.failures.push(PropertyFailure {
            suffix: "capacity_bytes",
            code: "disk_geometry_query_failed",
            native_code: Some(5),
        });
        let result = build_result(
            Ok(EnumerationResult {
                disks: vec![disk],
                messages: vec![],
            }),
            4,
        );

        assert_eq!(result.status.status, CollectorStatus::Partial);
        assert_eq!(
            result.status.fields[0].status,
            FieldCollectionStatus::Failed
        );
        assert_eq!(result.status.fields[0].native_code, Some(5));
    }

    #[test]
    fn non_windows_collection_fails_with_a_reason() {
        if cfg!(windows) {
            return;
        }
        let result = collect_physical_disks();

        assert_eq!(result.collection, None);
        assert_eq!(result.status.status, CollectorStatus::Failed);
        assert!(!result.status.messages.is_empty());
    }

    fn snapshot() -> DiskSnapshot {
        DiskSnapshot {
            number: 2,
            model: Some("Example Disk".into()),
            manufacturer: Some("Example Vendor".into()),
            firmware_revision: Some("1.0".into()),
            bus_type: Some(DiskBusType::Usb),
            capacity_bytes: Some(32_000_000_000),
            logical_sector_size_bytes: Some(512),
            removable: Some(true),
            failures: vec![],
        }
    }
}
