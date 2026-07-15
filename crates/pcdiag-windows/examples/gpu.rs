use pcdiag_windows::collect_gpus;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = collect_gpus();
    let output = json!({
        "collection": {
            "gpus": result.collection,
        },
        "status": {
            "collectors": [result.status],
        },
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
