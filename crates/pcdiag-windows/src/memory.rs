use std::time::Instant;

use pcdiag_core::{
    CollectionMessage, CollectorName, CollectorResult, CollectorStatus, CommitMemory,
    FieldCollectionResult, FieldCollectionStatus, MemoryCollection, PhysicalMemory, VirtualMemory,
};

const PHYSICAL_AND_VIRTUAL_PATHS: [&str; 5] = [
    "/memory/physical/total_bytes",
    "/memory/physical/available_bytes",
    "/memory/physical/load_percent",
    "/memory/virtual/total_bytes",
    "/memory/virtual/available_bytes",
];

const COMMIT_PATHS: [&str; 2] = [
    "/memory/commit/limit_bytes",
    "/memory/commit/available_bytes",
];

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryCollectionResult {
    pub collection: MemoryCollection,
    pub status: CollectorResult,
}

/// Collects a system-wide memory snapshot on Windows.
///
/// On non-Windows hosts this returns a skipped result so the workspace remains
/// buildable and testable without pretending that Windows data was collected.
pub fn collect_memory() -> MemoryCollectionResult {
    let started = Instant::now();
    let global = platform::global_memory_snapshot();
    let performance = platform::performance_memory_snapshot();
    build_result(global, performance, elapsed_milliseconds(started))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GlobalMemorySnapshot {
    total_physical_bytes: u64,
    available_physical_bytes: u64,
    load_percent: u32,
    total_virtual_bytes: u64,
    available_virtual_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PerformanceMemorySnapshot {
    commit_total_pages: u64,
    commit_limit_pages: u64,
    page_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CollectionFailure {
    code: &'static str,
    native_code: Option<i64>,
    message: &'static str,
    field_status: FieldCollectionStatus,
}

fn build_result(
    global: Result<GlobalMemorySnapshot, CollectionFailure>,
    performance: Result<PerformanceMemorySnapshot, CollectionFailure>,
    duration_ms: u64,
) -> MemoryCollectionResult {
    let mut messages = Vec::new();
    let mut fields = Vec::new();

    let (physical, virtual_memory, global_failed) = match global {
        Ok(snapshot) => (
            PhysicalMemory {
                total_bytes: Some(snapshot.total_physical_bytes),
                available_bytes: Some(snapshot.available_physical_bytes),
                load_percent: Some(f64::from(snapshot.load_percent)),
            },
            VirtualMemory {
                total_bytes: Some(snapshot.total_virtual_bytes),
                available_bytes: Some(snapshot.available_virtual_bytes),
            },
            false,
        ),
        Err(failure) => {
            messages.push(message_from_failure(&failure));
            fields.extend(field_results(&PHYSICAL_AND_VIRTUAL_PATHS, &failure));
            (
                PhysicalMemory {
                    total_bytes: None,
                    available_bytes: None,
                    load_percent: None,
                },
                VirtualMemory {
                    total_bytes: None,
                    available_bytes: None,
                },
                true,
            )
        }
    };

    let (commit, performance_failed) = match performance.and_then(commit_memory) {
        Ok(commit) => (commit, false),
        Err(failure) => {
            messages.push(message_from_failure(&failure));
            fields.extend(field_results(&COMMIT_PATHS, &failure));
            (
                CommitMemory {
                    limit_bytes: None,
                    available_bytes: None,
                },
                true,
            )
        }
    };

    let status = match (global_failed, performance_failed) {
        (false, false) => CollectorStatus::Success,
        (true, true) => {
            // Collector-wide failures explain all null values; individual
            // field failures would duplicate the same two API errors.
            fields.clear();
            CollectorStatus::Failed
        }
        _ => CollectorStatus::Partial,
    };

    MemoryCollectionResult {
        collection: MemoryCollection {
            physical,
            commit,
            virtual_memory,
        },
        status: CollectorResult {
            name: CollectorName::Memory,
            status,
            duration_ms,
            messages,
            fields,
        },
    }
}

fn commit_memory(snapshot: PerformanceMemorySnapshot) -> Result<CommitMemory, CollectionFailure> {
    let limit_bytes = snapshot
        .commit_limit_pages
        .checked_mul(snapshot.page_size_bytes)
        .ok_or_else(invalid_performance_value)?;
    let available_pages = snapshot
        .commit_limit_pages
        .checked_sub(snapshot.commit_total_pages)
        .ok_or_else(invalid_performance_value)?;
    let available_bytes = available_pages
        .checked_mul(snapshot.page_size_bytes)
        .ok_or_else(invalid_performance_value)?;

    Ok(CommitMemory {
        limit_bytes: Some(limit_bytes),
        available_bytes: Some(available_bytes),
    })
}

fn invalid_performance_value() -> CollectionFailure {
    CollectionFailure {
        code: "invalid_performance_information",
        native_code: None,
        message: "Windowsが報告したコミットメモリ情報を解釈できませんでした",
        field_status: FieldCollectionStatus::InvalidValue,
    }
}

fn message_from_failure(failure: &CollectionFailure) -> CollectionMessage {
    CollectionMessage {
        code: failure.code.into(),
        native_code: failure.native_code,
        message: Some(failure.message.into()),
    }
}

fn field_results(
    paths: &[&str],
    failure: &CollectionFailure,
) -> impl Iterator<Item = FieldCollectionResult> {
    paths.iter().map(|path| FieldCollectionResult {
        path: (*path).into(),
        status: failure.field_status,
        code: failure.code.into(),
        native_code: failure.native_code,
    })
}

fn elapsed_milliseconds(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(windows)]
mod platform {
    use std::{io, mem::size_of};

    use windows_sys::Win32::System::{
        ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION},
        SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
    };

    use super::{
        CollectionFailure, FieldCollectionStatus, GlobalMemorySnapshot, PerformanceMemorySnapshot,
    };

    pub(super) fn global_memory_snapshot() -> Result<GlobalMemorySnapshot, CollectionFailure> {
        let mut status = MEMORYSTATUSEX {
            dwLength: size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };

        // SAFETY: `status` is a valid, writable MEMORYSTATUSEX whose dwLength
        // is initialized as required by GlobalMemoryStatusEx.
        if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
            return Err(windows_failure(
                "global_memory_status_ex_failed",
                "Windowsから物理・仮想メモリ情報を取得できませんでした",
            ));
        }

        Ok(GlobalMemorySnapshot {
            total_physical_bytes: status.ullTotalPhys,
            available_physical_bytes: status.ullAvailPhys,
            load_percent: status.dwMemoryLoad,
            total_virtual_bytes: status.ullTotalVirtual,
            available_virtual_bytes: status.ullAvailVirtual,
        })
    }

    pub(super) fn performance_memory_snapshot()
    -> Result<PerformanceMemorySnapshot, CollectionFailure> {
        let mut performance = PERFORMANCE_INFORMATION {
            cb: size_of::<PERFORMANCE_INFORMATION>() as u32,
            ..Default::default()
        };

        // SAFETY: `performance` points to a writable PERFORMANCE_INFORMATION,
        // and the provided byte size matches the structure.
        if unsafe {
            GetPerformanceInfo(
                &mut performance,
                size_of::<PERFORMANCE_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(windows_failure(
                "get_performance_info_failed",
                "Windowsからコミットメモリ情報を取得できませんでした",
            ));
        }

        Ok(PerformanceMemorySnapshot {
            commit_total_pages: performance.CommitTotal as u64,
            commit_limit_pages: performance.CommitLimit as u64,
            page_size_bytes: performance.PageSize as u64,
        })
    }

    fn windows_failure(code: &'static str, message: &'static str) -> CollectionFailure {
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
    use super::{
        CollectionFailure, FieldCollectionStatus, GlobalMemorySnapshot, PerformanceMemorySnapshot,
    };

    pub(super) fn global_memory_snapshot() -> Result<GlobalMemorySnapshot, CollectionFailure> {
        Err(unsupported_platform())
    }

    pub(super) fn performance_memory_snapshot()
    -> Result<PerformanceMemorySnapshot, CollectionFailure> {
        Err(unsupported_platform())
    }

    fn unsupported_platform() -> CollectionFailure {
        CollectionFailure {
            code: "platform_not_supported",
            native_code: None,
            message: "Windows以外の環境ではメモリ情報を収集できません",
            field_status: FieldCollectionStatus::Unsupported,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_windows_snapshots_to_the_shared_memory_model() {
        let result = build_result(Ok(global_snapshot()), Ok(performance_snapshot()), 12);

        assert_eq!(result.status.status, CollectorStatus::Success);
        assert_eq!(result.status.duration_ms, 12);
        assert_eq!(result.collection.physical.total_bytes, Some(16_000));
        assert_eq!(result.collection.physical.available_bytes, Some(4_000));
        assert_eq!(result.collection.physical.load_percent, Some(75.0));
        assert_eq!(result.collection.commit.limit_bytes, Some(40_960));
        assert_eq!(result.collection.commit.available_bytes, Some(16_384));
        assert_eq!(result.collection.virtual_memory.total_bytes, Some(80_000));
    }

    #[test]
    fn preserves_available_data_when_one_windows_api_fails() {
        let result = build_result(
            Err(CollectionFailure {
                code: "global_memory_status_ex_failed",
                native_code: Some(5),
                message: "failed",
                field_status: FieldCollectionStatus::Failed,
            }),
            Ok(performance_snapshot()),
            3,
        );

        assert_eq!(result.status.status, CollectorStatus::Partial);
        assert_eq!(result.status.fields.len(), PHYSICAL_AND_VIRTUAL_PATHS.len());
        assert_eq!(result.collection.physical.total_bytes, None);
        assert_eq!(result.collection.commit.limit_bytes, Some(40_960));
    }

    #[test]
    fn rejects_inconsistent_commit_page_counts() {
        let result = build_result(
            Ok(global_snapshot()),
            Ok(PerformanceMemorySnapshot {
                commit_total_pages: 11,
                commit_limit_pages: 10,
                page_size_bytes: 4_096,
            }),
            1,
        );

        assert_eq!(result.status.status, CollectorStatus::Partial);
        assert_eq!(result.collection.commit.limit_bytes, None);
        assert!(result.status.fields.iter().all(|field| {
            field.status == FieldCollectionStatus::InvalidValue
                && field.code == "invalid_performance_information"
        }));
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_collection_returns_a_failed_result() {
        let result = collect_memory();

        assert_eq!(result.status.status, CollectorStatus::Failed);
        assert!(
            result
                .status
                .messages
                .iter()
                .all(|message| { message.code == "platform_not_supported" })
        );
    }

    fn global_snapshot() -> GlobalMemorySnapshot {
        GlobalMemorySnapshot {
            total_physical_bytes: 16_000,
            available_physical_bytes: 4_000,
            load_percent: 75,
            total_virtual_bytes: 80_000,
            available_virtual_bytes: 60_000,
        }
    }

    fn performance_snapshot() -> PerformanceMemorySnapshot {
        PerformanceMemorySnapshot {
            commit_total_pages: 6,
            commit_limit_pages: 10,
            page_size_bytes: 4_096,
        }
    }
}
