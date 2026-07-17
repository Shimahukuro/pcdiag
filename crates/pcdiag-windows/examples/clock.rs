use pcdiag_windows::collect_clock;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let result = collect_clock();
    let output = json!({
        "collection": {
            "clock": result.collection,
        },
        "status": {
            "collectors": [result.status],
        },
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
