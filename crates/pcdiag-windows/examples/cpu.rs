use pcdiag_windows::collect_cpu;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = collect_cpu();
    let output = json!({
        "collection": {
            "cpu": result.collection,
        },
        "status": {
            "collectors": [result.status],
        },
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
