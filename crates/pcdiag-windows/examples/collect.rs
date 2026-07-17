use pcdiag_windows::{
    collect_clock, collect_devices, collect_gpus, collect_memory, collect_partitions,
    collect_physical_disks, collect_smart, collect_volumes, collect_windows_info,
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let windows = collect_windows_info();
    let clock = collect_clock();
    let memory = collect_memory();
    let gpus = collect_gpus();
    let devices = collect_devices();
    let physical_disks = collect_physical_disks();
    let partitions = collect_partitions();
    let volumes = collect_volumes();
    let smart = collect_smart();
    let output = json!({
        "collection": {
            "windows": windows.collection,
            "clock": clock.collection,
            "memory": memory.collection,
            "gpus": gpus.collection,
            "devices": devices.collection,
            "storage": {
                "disks": physical_disks.collection,
                "partitions": partitions.collection,
                "volumes": volumes.collection,
                "smart": smart.collection,
            },
        },
        "status": {
            "collectors": [
                windows.status,
                clock.status,
                memory.status,
                gpus.status,
                devices.status,
                physical_disks.status,
                partitions.status,
                volumes.status,
                smart.status,
            ],
        },
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
