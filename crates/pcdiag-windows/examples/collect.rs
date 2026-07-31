use pcdiag_windows::collect_all;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = collect_all(30);
    let output = json!({
        "collection": result.collection,
        "status": result.status,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
