use std::time::Instant;

use pcdiag_core::{
    CollectionMessage, CollectorName, CollectorResult, CollectorStatus, DiskPartition,
    FieldCollectionResult, FieldCollectionStatus, PartitionStyle,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionCollectionResult {
    pub collection: Option<Vec<DiskPartition>>,
    pub status: CollectorResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PartitionSnapshot {
    disk_number: u32,
    partition_number: u32,
    offset_bytes: u64,
    length_bytes: u64,
    style: PartitionStyle,
    type_id: Option<String>,
    bootable: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumerationResult {
    partitions: Vec<PartitionSnapshot>,
    messages: Vec<CollectionMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EnumerationFailure {
    native_code: Option<i64>,
}

pub fn collect_partitions() -> PartitionCollectionResult {
    let started = Instant::now();
    build_result(platform::enumerate_partitions(), elapsed_ms(started))
}

fn build_result(
    result: Result<EnumerationResult, EnumerationFailure>,
    duration_ms: u64,
) -> PartitionCollectionResult {
    match result {
        Ok(result) => {
            let mut fields = Vec::new();
            let collection = result
                .partitions
                .into_iter()
                .enumerate()
                .map(|(index, snapshot)| map_partition(snapshot, index, &mut fields))
                .collect();
            let has_failure = !result.messages.is_empty()
                || fields
                    .iter()
                    .any(|field| field.status != FieldCollectionStatus::NotApplicable);
            PartitionCollectionResult {
                collection: Some(collection),
                status: CollectorResult {
                    name: CollectorName::Partitions,
                    status: if has_failure {
                        CollectorStatus::Partial
                    } else {
                        CollectorStatus::Success
                    },
                    duration_ms,
                    messages: result.messages,
                    fields,
                },
            }
        }
        Err(failure) => PartitionCollectionResult {
            collection: None,
            status: CollectorResult {
                name: CollectorName::Partitions,
                status: CollectorStatus::Failed,
                duration_ms,
                messages: vec![CollectionMessage {
                    code: "partition_enumeration_failed".into(),
                    native_code: failure.native_code,
                    message: Some("パーティションを列挙できませんでした".into()),
                }],
                fields: vec![],
            },
        },
    }
}

fn map_partition(
    snapshot: PartitionSnapshot,
    index: usize,
    fields: &mut Vec<FieldCollectionResult>,
) -> DiskPartition {
    let base = format!("/storage/partitions/{index}");
    if snapshot.type_id.is_none() {
        fields.push(FieldCollectionResult {
            path: format!("{base}/type_id"),
            status: FieldCollectionStatus::NotApplicable,
            code: "raw_partition_has_no_type".into(),
            native_code: None,
        });
    }
    if snapshot.bootable.is_none() {
        fields.push(FieldCollectionResult {
            path: format!("{base}/bootable"),
            status: FieldCollectionStatus::NotApplicable,
            code: "boot_indicator_only_applies_to_mbr".into(),
            native_code: None,
        });
    }

    DiskPartition {
        disk_number: snapshot.disk_number,
        partition_number: snapshot.partition_number,
        offset_bytes: snapshot.offset_bytes,
        length_bytes: snapshot.length_bytes,
        style: snapshot.style,
        type_id: snapshot.type_id,
        bootable: snapshot.bootable,
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(any(windows, test))]
fn format_guid(value: u128) -> String {
    format!(
        "{{{:08x}-{:04x}-{:04x}-{:04x}-{:012x}}}",
        value >> 96,
        (value >> 80) & 0xffff,
        (value >> 64) & 0xffff,
        (value >> 48) & 0xffff,
        value & 0xffff_ffff_ffff,
    )
}

#[cfg(windows)]
mod platform {
    use std::{ffi::c_void, mem::size_of, ptr::read_unaligned};

    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{
        DRIVE_LAYOUT_INFORMATION_EX, IOCTL_DISK_GET_DRIVE_LAYOUT_EX, PARTITION_INFORMATION_EX,
        PARTITION_STYLE_GPT, PARTITION_STYLE_MBR,
    };

    use super::{
        CollectionMessage, EnumerationFailure, EnumerationResult, PartitionSnapshot,
        PartitionStyle, format_guid,
    };
    use crate::physical_disks::{enumerate_disk_numbers, open_disk};

    const LAYOUT_BUFFER_SIZE: usize = 64 * 1024;

    pub(super) fn enumerate_partitions() -> Result<EnumerationResult, EnumerationFailure> {
        let numbers = enumerate_disk_numbers().map_err(failure)?;
        let mut partitions = Vec::new();
        let mut messages = Vec::new();

        for number in numbers {
            match open_disk(number) {
                Ok(handle) => match query_layout(handle.0, number) {
                    Ok(mut values) => partitions.append(&mut values),
                    Err(error) => messages.push(CollectionMessage {
                        code: "partition_layout_query_failed".into(),
                        native_code: Some(i64::from(error.code().0)),
                        message: Some(format!("PhysicalDrive{number}の構成を取得できませんでした")),
                    }),
                },
                Err(error) => messages.push(CollectionMessage {
                    code: "physical_disk_open_failed".into(),
                    native_code: Some(i64::from(error.code().0)),
                    message: Some(format!("PhysicalDrive{number}を開けませんでした")),
                }),
            }
        }

        Ok(EnumerationResult {
            partitions,
            messages,
        })
    }

    fn query_layout(
        handle: windows::Win32::Foundation::HANDLE,
        disk_number: u32,
    ) -> windows::core::Result<Vec<PartitionSnapshot>> {
        let mut buffer = vec![0_u8; LAYOUT_BUFFER_SIZE];
        let mut returned = 0;
        // SAFETY: The output buffer is writable for the supplied length and the handle refers to
        // a physical disk opened for metadata queries.
        unsafe {
            DeviceIoControl(
                handle,
                IOCTL_DISK_GET_DRIVE_LAYOUT_EX,
                None,
                0,
                Some(buffer.as_mut_ptr().cast::<c_void>()),
                buffer.len() as u32,
                Some(&mut returned),
                None,
            )?
        };
        if (returned as usize) < size_of::<DRIVE_LAYOUT_INFORMATION_EX>() {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x8007000D_u32 as i32,
            )));
        }

        // SAFETY: DeviceIoControl initialized the drive layout header on success.
        let layout =
            unsafe { read_unaligned(buffer.as_ptr().cast::<DRIVE_LAYOUT_INFORMATION_EX>()) };
        let entries_offset =
            size_of::<DRIVE_LAYOUT_INFORMATION_EX>() - size_of::<PARTITION_INFORMATION_EX>();
        let count = layout.PartitionCount as usize;
        let required = entries_offset
            .checked_add(count.saturating_mul(size_of::<PARTITION_INFORMATION_EX>()))
            .ok_or_else(|| {
                windows::core::Error::from_hresult(windows::core::HRESULT(0x8007000D_u32 as i32))
            })?;
        if required > returned as usize {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x8007000D_u32 as i32,
            )));
        }

        let mut partitions = Vec::new();
        for index in 0..count {
            // SAFETY: Bounds were checked above; read_unaligned copies one entry from the buffer.
            let entry = unsafe {
                read_unaligned(
                    buffer
                        .as_ptr()
                        .add(entries_offset + index * size_of::<PARTITION_INFORMATION_EX>())
                        .cast::<PARTITION_INFORMATION_EX>(),
                )
            };
            let (Ok(offset_bytes), Ok(length_bytes)) = (
                u64::try_from(entry.StartingOffset),
                u64::try_from(entry.PartitionLength),
            ) else {
                continue;
            };
            if entry.PartitionNumber == 0 || length_bytes == 0 {
                continue;
            }

            let (style, type_id, bootable) = if entry.PartitionStyle == PARTITION_STYLE_MBR {
                // SAFETY: PartitionStyle identifies the active union member.
                let mbr = unsafe { entry.Anonymous.Mbr };
                (
                    PartitionStyle::Mbr,
                    Some(format!("0x{:02x}", mbr.PartitionType)),
                    Some(mbr.BootIndicator),
                )
            } else if entry.PartitionStyle == PARTITION_STYLE_GPT {
                // SAFETY: PartitionStyle identifies the active union member.
                let gpt = unsafe { entry.Anonymous.Gpt };
                (
                    PartitionStyle::Gpt,
                    Some(format_guid(gpt.PartitionType.to_u128())),
                    None,
                )
            } else {
                (PartitionStyle::Raw, None, None)
            };
            partitions.push(PartitionSnapshot {
                disk_number,
                partition_number: entry.PartitionNumber,
                offset_bytes,
                length_bytes,
                style,
                type_id,
                bootable,
            });
        }
        Ok(partitions)
    }

    fn failure(error: windows::core::Error) -> EnumerationFailure {
        EnumerationFailure {
            native_code: Some(i64::from(error.code().0)),
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{EnumerationFailure, EnumerationResult};

    pub(super) fn enumerate_partitions() -> Result<EnumerationResult, EnumerationFailure> {
        Err(EnumerationFailure { native_code: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_gpt_partition_with_not_applicable_boot_indicator() {
        let result = build_result(
            Ok(EnumerationResult {
                partitions: vec![snapshot()],
                messages: vec![],
            }),
            3,
        );

        assert_eq!(result.status.status, CollectorStatus::Success);
        assert_eq!(result.status.fields.len(), 1);
        assert_eq!(
            result.status.fields[0].status,
            FieldCollectionStatus::NotApplicable
        );
    }

    #[test]
    fn formats_partition_type_guid() {
        assert_eq!(
            format_guid(0xebd0a0a2_b9e5_4433_87c0_68b6b72699c7),
            "{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}"
        );
    }

    fn snapshot() -> PartitionSnapshot {
        PartitionSnapshot {
            disk_number: 1,
            partition_number: 2,
            offset_bytes: 1_048_576,
            length_bytes: 100_000_000_000,
            style: PartitionStyle::Gpt,
            type_id: Some("{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}".into()),
            bootable: None,
        }
    }
}
