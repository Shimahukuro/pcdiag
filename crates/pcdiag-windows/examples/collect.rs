use pcdiag_windows::{collect_gpus, collect_memory};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let memory = collect_memory();
    let gpus = collect_gpus();
    let output = json!({
        "collection": {
            "memory": memory.collection,
            "gpus": gpus.collection,
        },
        "status": {
            "collectors": [memory.status, gpus.status],
        },
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
