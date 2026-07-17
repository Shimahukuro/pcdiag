use pcdiag_windows::collect_windows_info;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = collect_windows_info();
    let output = json!({
        "collection": {
            "windows": result.collection,
        },
        "status": {
            "collectors": [result.status],
        },
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
