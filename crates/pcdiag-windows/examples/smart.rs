use pcdiag_windows::collect_smart;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = collect_smart();
    let output = json!({
        "collection": {
            "storage": {
                "smart": result.collection,
            },
        },
        "status": {
            "collectors": [result.status],
        },
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
