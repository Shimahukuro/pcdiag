use std::time::Instant;

use pcdiag_core::{
    CollectionMessage, CollectorName, CollectorResult, CollectorStatus, FieldCollectionResult,
    FieldCollectionStatus, StorageVolume, VolumeExtent,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeCollectionResult {
    pub collection: Option<Vec<StorageVolume>>,
    pub status: CollectorResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VolumeSnapshot {
    mount_points: Option<Vec<String>>,
    file_system: Option<String>,
    capacity_bytes: Option<u64>,
    free_bytes: Option<u64>,
    extents: Option<Vec<VolumeExtent>>,
    failures: Vec<PropertyFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PropertyFailure {
    suffix: &'static str,
    code: &'static str,
    native_code: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumerationFailure {
    native_code: Option<i64>,
}

pub fn collect_volumes() -> VolumeCollectionResult {
    let started = Instant::now();
    build_result(platform::enumerate_volumes(), elapsed_ms(started))
}

fn build_result(
    snapshots: Result<Vec<VolumeSnapshot>, EnumerationFailure>,
    duration_ms: u64,
) -> VolumeCollectionResult {
    match snapshots {
        Ok(snapshots) => {
            let mut fields = Vec::new();
            let collection = snapshots
                .into_iter()
                .enumerate()
                .map(|(index, snapshot)| map_volume(snapshot, index, &mut fields))
                .collect();
            VolumeCollectionResult {
                collection: Some(collection),
                status: CollectorResult {
                    name: CollectorName::Volumes,
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
        Err(failure) => VolumeCollectionResult {
            collection: None,
            status: CollectorResult {
                name: CollectorName::Volumes,
                status: CollectorStatus::Failed,
                duration_ms,
                messages: vec![CollectionMessage {
                    code: "volume_enumeration_failed".into(),
                    native_code: failure.native_code,
                    message: Some("ボリュームを列挙できませんでした".into()),
                }],
                fields: vec![],
            },
        },
    }
}

fn map_volume(
    snapshot: VolumeSnapshot,
    index: usize,
    fields: &mut Vec<FieldCollectionResult>,
) -> StorageVolume {
    let base = format!("/storage/volumes/{index}");
    for (suffix, available, code) in [
        (
            "mount_points",
            snapshot.mount_points.is_some(),
            "volume_mount_points_unavailable",
        ),
        (
            "file_system",
            snapshot.file_system.is_some(),
            "volume_file_system_unavailable",
        ),
        (
            "capacity_bytes",
            snapshot.capacity_bytes.is_some(),
            "volume_capacity_unavailable",
        ),
        (
            "free_bytes",
            snapshot.free_bytes.is_some(),
            "volume_free_space_unavailable",
        ),
        (
            "extents",
            snapshot.extents.is_some(),
            "volume_extents_unavailable",
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

    StorageVolume {
        mount_points: snapshot.mount_points,
        file_system: snapshot.file_system,
        capacity_bytes: snapshot.capacity_bytes,
        free_bytes: snapshot.free_bytes,
        extents: snapshot.extents,
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(windows)]
mod platform {
    use std::{ffi::c_void, mem::size_of, ptr::read_unaligned};

    use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_FILES, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        FindFirstVolumeW, FindNextVolumeW, FindVolumeClose, GetDiskFreeSpaceExW,
        GetVolumeInformationW, GetVolumePathNamesForVolumeNameW,
        IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{DISK_EXTENT, VOLUME_DISK_EXTENTS};
    use windows::core::PCWSTR;

    use super::{EnumerationFailure, PropertyFailure, VolumeExtent, VolumeSnapshot};

    const VOLUME_NAME_CAPACITY: usize = 1024;
    const EXTENTS_BUFFER_SIZE: usize = 64 * 1024;

    struct VolumeSearch(HANDLE);

    impl Drop for VolumeSearch {
        fn drop(&mut self) {
            // SAFETY: The handle was returned by FindFirstVolumeW and is closed once.
            let _ = unsafe { FindVolumeClose(self.0) };
        }
    }

    struct OwnedHandle(HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            // SAFETY: The handle was returned by CreateFileW and is closed once.
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
        }
    }

    pub(super) fn enumerate_volumes() -> Result<Vec<VolumeSnapshot>, EnumerationFailure> {
        let mut name_buffer = vec![0_u16; VOLUME_NAME_CAPACITY];
        // SAFETY: The UTF-16 output buffer is writable for its full length.
        let search = VolumeSearch(unsafe { FindFirstVolumeW(&mut name_buffer) }.map_err(failure)?);
        let mut volumes = Vec::new();

        loop {
            let name = utf16_string(&name_buffer);
            if !name.is_empty() {
                volumes.push(query_volume(&name));
            }
            name_buffer.fill(0);
            // SAFETY: The search handle is valid and the output buffer is writable.
            match unsafe { FindNextVolumeW(search.0, &mut name_buffer) } {
                Ok(()) => {}
                Err(error) if error.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
                Err(error) => return Err(failure(error)),
            }
        }
        Ok(volumes)
    }

    fn query_volume(name: &str) -> VolumeSnapshot {
        let name_wide = wide(name);
        let mut snapshot = VolumeSnapshot {
            mount_points: None,
            file_system: None,
            capacity_bytes: None,
            free_bytes: None,
            extents: None,
            failures: vec![],
        };

        match query_mount_points(PCWSTR(name_wide.as_ptr())) {
            Ok(mount_points) => snapshot.mount_points = Some(mount_points),
            Err(error) => snapshot.failures.push(property_failure(
                "mount_points",
                "volume_mount_points_query_failed",
                &error,
            )),
        }

        let mut filesystem = vec![0_u16; 64];
        // SAFETY: The volume path is null-terminated and the filesystem buffer is writable.
        match unsafe {
            GetVolumeInformationW(
                PCWSTR(name_wide.as_ptr()),
                None,
                None,
                None,
                None,
                Some(&mut filesystem),
            )
        } {
            Ok(()) => snapshot.file_system = nonempty(utf16_string(&filesystem)),
            Err(error) => snapshot.failures.push(property_failure(
                "file_system",
                "volume_information_query_failed",
                &error,
            )),
        }

        let mut capacity = 0;
        let mut free = 0;
        // SAFETY: The volume path is null-terminated and both output pointers are valid.
        match unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(name_wide.as_ptr()),
                None,
                Some(&mut capacity),
                Some(&mut free),
            )
        } {
            Ok(()) => {
                snapshot.capacity_bytes = Some(capacity);
                snapshot.free_bytes = Some(free);
            }
            Err(error) => {
                snapshot.failures.push(property_failure(
                    "capacity_bytes",
                    "volume_space_query_failed",
                    &error,
                ));
                snapshot.failures.push(property_failure(
                    "free_bytes",
                    "volume_space_query_failed",
                    &error,
                ));
            }
        }

        match open_volume(name).and_then(|handle| query_extents(handle.0)) {
            Ok(extents) => snapshot.extents = Some(extents),
            Err(error) => snapshot.failures.push(property_failure(
                "extents",
                "volume_extents_query_failed",
                &error,
            )),
        }
        snapshot
    }

    fn query_mount_points(name: PCWSTR) -> windows::core::Result<Vec<String>> {
        let mut required = 0;
        let mut buffer = vec![0_u16; 256];
        loop {
            // SAFETY: The volume name and writable output buffer are valid.
            match unsafe {
                GetVolumePathNamesForVolumeNameW(name, Some(&mut buffer), &mut required)
            } {
                Ok(()) => {
                    return Ok(buffer
                        .split(|value| *value == 0)
                        .filter_map(|path| String::from_utf16(path).ok())
                        .filter(|path| is_drive_root(path))
                        .collect());
                }
                Err(error) if error.code() == ERROR_MORE_DATA.to_hresult() => {
                    buffer.resize(required as usize, 0);
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn open_volume(name: &str) -> windows::core::Result<OwnedHandle> {
        let path = wide(name.trim_end_matches('\\'));
        // SAFETY: The volume path is null-terminated and flags permit metadata-only access.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )?
        };
        Ok(OwnedHandle(handle))
    }

    fn query_extents(handle: HANDLE) -> windows::core::Result<Vec<VolumeExtent>> {
        let mut buffer = vec![0_u8; EXTENTS_BUFFER_SIZE];
        let mut returned = 0;
        // SAFETY: The output buffer is writable and the handle refers to a volume.
        unsafe {
            DeviceIoControl(
                handle,
                IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
                None,
                0,
                Some(buffer.as_mut_ptr().cast::<c_void>()),
                buffer.len() as u32,
                Some(&mut returned),
                None,
            )?
        };
        if (returned as usize) < size_of::<VOLUME_DISK_EXTENTS>() {
            return Err(invalid_data());
        }
        // SAFETY: DeviceIoControl initialized the volume extent header.
        let header = unsafe { read_unaligned(buffer.as_ptr().cast::<VOLUME_DISK_EXTENTS>()) };
        let count = header.NumberOfDiskExtents as usize;
        let entries_offset = size_of::<VOLUME_DISK_EXTENTS>() - size_of::<DISK_EXTENT>();
        let required = entries_offset
            .checked_add(count.saturating_mul(size_of::<DISK_EXTENT>()))
            .ok_or_else(invalid_data)?;
        if required > returned as usize {
            return Err(invalid_data());
        }

        let mut extents = Vec::with_capacity(count);
        for index in 0..count {
            // SAFETY: The entry range was checked above and read_unaligned copies the structure.
            let extent = unsafe {
                read_unaligned(
                    buffer
                        .as_ptr()
                        .add(entries_offset + index * size_of::<DISK_EXTENT>())
                        .cast::<DISK_EXTENT>(),
                )
            };
            let (Ok(offset_bytes), Ok(length_bytes)) = (
                u64::try_from(extent.StartingOffset),
                u64::try_from(extent.ExtentLength),
            ) else {
                continue;
            };
            extents.push(VolumeExtent {
                disk_number: extent.DiskNumber,
                offset_bytes,
                length_bytes,
            });
        }
        Ok(extents)
    }

    fn property_failure(
        suffix: &'static str,
        code: &'static str,
        error: &windows::core::Error,
    ) -> PropertyFailure {
        PropertyFailure {
            suffix,
            code,
            native_code: Some(i64::from(error.code().0)),
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

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    fn utf16_string(buffer: &[u16]) -> String {
        let end = buffer
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end])
    }

    fn nonempty(value: String) -> Option<String> {
        (!value.is_empty()).then_some(value)
    }

    fn is_drive_root(path: &str) -> bool {
        let bytes = path.as_bytes();
        bytes.len() == 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\'
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{EnumerationFailure, VolumeSnapshot};

    pub(super) fn enumerate_volumes() -> Result<Vec<VolumeSnapshot>, EnumerationFailure> {
        Err(EnumerationFailure { native_code: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_complete_volume() {
        let result = build_result(Ok(vec![snapshot()]), 3);

        assert_eq!(result.status.status, CollectorStatus::Success);
        let volume = &result.collection.unwrap()[0];
        assert_eq!(volume.mount_points, Some(vec!["E:\\".into()]));
        assert_eq!(volume.extents.as_ref().unwrap()[0].disk_number, 2);
    }

    #[test]
    fn failed_space_query_preserves_mount_points_and_extents() {
        let mut volume = snapshot();
        volume.capacity_bytes = None;
        volume.failures.push(PropertyFailure {
            suffix: "capacity_bytes",
            code: "volume_space_query_failed",
            native_code: Some(21),
        });
        let result = build_result(Ok(vec![volume]), 3);

        assert_eq!(result.status.status, CollectorStatus::Partial);
        assert_eq!(
            result.status.fields[0].status,
            FieldCollectionStatus::Failed
        );
    }

    fn snapshot() -> VolumeSnapshot {
        VolumeSnapshot {
            mount_points: Some(vec!["E:\\".into()]),
            file_system: Some("NTFS".into()),
            capacity_bytes: Some(1_000_000_000_000),
            free_bytes: Some(500_000_000_000),
            extents: Some(vec![VolumeExtent {
                disk_number: 2,
                offset_bytes: 1_048_576,
                length_bytes: 1_000_000_000_000,
            }]),
            failures: vec![],
        }
    }
}
