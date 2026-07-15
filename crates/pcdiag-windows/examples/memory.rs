use pcdiag_windows::collect_memory;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = collect_memory();
    let output = json!({
        "collection": {
            "memory": result.collection,
        },
        "status": {
            "collectors": [result.status],
        },
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
