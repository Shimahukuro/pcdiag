use pcdiag_windows::collect_physical_disks;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = collect_physical_disks();
    let output = json!({
        "collection": {
            "storage": {
                "disks": result.collection,
            },
        },
        "status": {
            "collectors": [result.status],
        },
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
