use pcdiag_windows::{collect_partitions, collect_physical_disks, collect_volumes};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let disks = collect_physical_disks();
    let partitions = collect_partitions();
    let volumes = collect_volumes();
    let output = json!({
        "collection": {
            "storage": {
                "disks": disks.collection,
                "partitions": partitions.collection,
                "volumes": volumes.collection,
            },
        },
        "status": {
            "collectors": [disks.status, partitions.status, volumes.status],
        },
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
