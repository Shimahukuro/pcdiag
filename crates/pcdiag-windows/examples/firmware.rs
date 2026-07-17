use pcdiag_windows::collect_firmware;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = collect_firmware();
    let output = json!({
        "collection": {"firmware": result.collection},
        "status": {"collectors": [result.status]},
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
