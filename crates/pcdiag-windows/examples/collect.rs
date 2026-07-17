use pcdiag_windows::{collect_devices, collect_gpus, collect_memory, collect_physical_disks};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let memory = collect_memory();
    let gpus = collect_gpus();
    let devices = collect_devices();
    let physical_disks = collect_physical_disks();
    let output = json!({
        "collection": {
            "memory": memory.collection,
            "gpus": gpus.collection,
            "devices": devices.collection,
            "storage": {
                "disks": physical_disks.collection,
            },
        },
        "status": {
            "collectors": [memory.status, gpus.status, devices.status, physical_disks.status],
        },
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
