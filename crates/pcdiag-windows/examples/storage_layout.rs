use pcdiag_windows::{collect_partitions, collect_physical_disks, collect_smart, collect_volumes};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let disks = collect_physical_disks();
    let partitions = collect_partitions();
    let volumes = collect_volumes();
    let smart = collect_smart();
    let output = json!({
        "collection": {
            "storage": {
                "disks": disks.collection,
                "partitions": partitions.collection,
                "volumes": volumes.collection,
                "smart": smart.collection,
            },
        },
        "status": {
            "collectors": [disks.status, partitions.status, volumes.status, smart.status],
        },
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
