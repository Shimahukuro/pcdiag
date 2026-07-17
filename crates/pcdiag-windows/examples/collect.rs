use pcdiag_windows::{
    collect_devices, collect_gpus, collect_memory, collect_partitions, collect_physical_disks,
    collect_volumes,
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let memory = collect_memory();
    let gpus = collect_gpus();
    let devices = collect_devices();
    let physical_disks = collect_physical_disks();
    let partitions = collect_partitions();
    let volumes = collect_volumes();
    let output = json!({
        "collection": {
            "memory": memory.collection,
            "gpus": gpus.collection,
            "devices": devices.collection,
            "storage": {
                "disks": physical_disks.collection,
                "partitions": partitions.collection,
                "volumes": volumes.collection,
            },
        },
        "status": {
            "collectors": [
                memory.status,
                gpus.status,
                devices.status,
                physical_disks.status,
                partitions.status,
                volumes.status,
            ],
        },
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
