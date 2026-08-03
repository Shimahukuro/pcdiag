use pcdiag_core::{Collection, CollectionStatus, StorageCollection};

use crate::{
    WindowsUpdateCollectionOptions, collect_clock, collect_cpu, collect_devices,
    collect_event_logs, collect_firmware, collect_gpus, collect_memory, collect_partitions,
    collect_physical_disks, collect_smart, collect_volumes, collect_windows_info,
    collect_windows_updates,
};

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteCollectionResult {
    pub collection: Collection,
    pub status: CollectionStatus,
}

pub fn collect_all(
    event_log_days: u32,
    windows_update_options: WindowsUpdateCollectionOptions,
) -> CompleteCollectionResult {
    collect_all_cancellable(event_log_days, windows_update_options, || false)
        .expect("cancellation is disabled")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollectionInterrupted;

pub fn collect_all_cancellable<F>(
    event_log_days: u32,
    windows_update_options: WindowsUpdateCollectionOptions,
    is_cancelled: F,
) -> Result<CompleteCollectionResult, CollectionInterrupted>
where
    F: Fn() -> bool,
{
    macro_rules! check_cancelled {
        () => {
            if is_cancelled() {
                return Err(CollectionInterrupted);
            }
        };
    }

    check_cancelled!();
    let windows = collect_windows_info();
    check_cancelled!();
    let windows_updates = collect_windows_updates(windows_update_options);
    check_cancelled!();
    let clock = collect_clock();
    check_cancelled!();
    let cpu = collect_cpu();
    check_cancelled!();
    let firmware = collect_firmware();
    check_cancelled!();
    let memory = collect_memory();
    check_cancelled!();
    let gpus = collect_gpus();
    check_cancelled!();
    let devices = collect_devices();
    check_cancelled!();
    let event_logs = collect_event_logs(event_log_days);
    check_cancelled!();
    let physical_disks = collect_physical_disks();
    check_cancelled!();
    let partitions = collect_partitions();
    check_cancelled!();
    let volumes = collect_volumes();
    check_cancelled!();
    let smart = collect_smart();
    check_cancelled!();

    Ok(CompleteCollectionResult {
        collection: Collection {
            windows: windows.collection,
            windows_updates: windows_updates.collection,
            clock: clock.collection,
            cpu: cpu.collection,
            firmware: firmware.collection,
            memory: memory.collection,
            gpus: gpus.collection,
            devices: devices.collection,
            event_logs: event_logs.collection,
            storage: StorageCollection {
                disks: physical_disks.collection,
                partitions: partitions.collection,
                volumes: volumes.collection,
                smart: smart.collection,
            },
        },
        status: CollectionStatus {
            collectors: vec![
                windows.status,
                windows_updates.status,
                clock.status,
                cpu.status,
                firmware.status,
                memory.status,
                gpus.status,
                devices.status,
                event_logs.status,
                physical_disks.status,
                partitions.status,
                volumes.status,
                smart.status,
            ],
        },
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn cancellation_is_checked_before_collecting() {
        let checks = Cell::new(0);
        let result = collect_all_cancellable(30, WindowsUpdateCollectionOptions::default(), || {
            checks.set(checks.get() + 1);
            true
        });

        assert_eq!(result, Err(CollectionInterrupted));
        assert_eq!(checks.get(), 1);
    }
}
