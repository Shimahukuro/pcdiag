use pcdiag_core::{Collection, CollectionStatus, StorageCollection};

use crate::{
    collect_clock, collect_cpu, collect_devices, collect_firmware, collect_gpus, collect_memory,
    collect_partitions, collect_physical_disks, collect_smart, collect_volumes,
    collect_windows_info,
};

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteCollectionResult {
    pub collection: Collection,
    pub status: CollectionStatus,
}

pub fn collect_all() -> CompleteCollectionResult {
    let windows = collect_windows_info();
    let clock = collect_clock();
    let cpu = collect_cpu();
    let firmware = collect_firmware();
    let memory = collect_memory();
    let gpus = collect_gpus();
    let devices = collect_devices();
    let physical_disks = collect_physical_disks();
    let partitions = collect_partitions();
    let volumes = collect_volumes();
    let smart = collect_smart();

    CompleteCollectionResult {
        collection: Collection {
            windows: windows.collection,
            clock: clock.collection,
            cpu: cpu.collection,
            firmware: firmware.collection,
            memory: memory.collection,
            gpus: gpus.collection,
            devices: devices.collection,
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
                clock.status,
                cpu.status,
                firmware.status,
                memory.status,
                gpus.status,
                devices.status,
                physical_disks.status,
                partitions.status,
                volumes.status,
                smart.status,
            ],
        },
    }
}
