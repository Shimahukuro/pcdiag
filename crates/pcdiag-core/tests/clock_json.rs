use pcdiag_core::{Collection, CollectionStatus};
use serde_json::{Value, json};

const COLLECTION: &str = include_str!("fixtures/memory-success-collection.json");
const MEMORY_STATUS: &str = include_str!("fixtures/memory-success-status.json");

#[test]
fn clock_information_round_trips() {
    let expected: Value = serde_json::from_str(COLLECTION).unwrap();
    let collection: Collection = serde_json::from_value(expected.clone()).unwrap();

    assert_eq!(serde_json::to_value(collection).unwrap(), expected);
}

#[test]
fn missing_clock_field_is_rejected_but_null_is_allowed() {
    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["clock"]
        .as_object_mut()
        .unwrap()
        .remove("hardware_clock");
    assert!(serde_json::from_value::<Collection>(value).is_err());

    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["clock"]["system_time_utc"] = Value::Null;
    assert!(serde_json::from_value::<Collection>(value).is_ok());
}

#[test]
fn hardware_clock_null_is_valid_when_marked_unsupported() {
    let collection: Collection = serde_json::from_str(COLLECTION).unwrap();
    let mut status_value: Value = serde_json::from_str(MEMORY_STATUS).unwrap();
    status_value["collectors"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "name": "clock",
            "status": "partial",
            "duration_ms": 1,
            "messages": [],
            "fields": [{
                "path": "/clock/hardware_clock",
                "status": "unsupported",
                "code": "hardware_clock_direct_access_unsupported"
            }]
        }));
    let status: CollectionStatus = serde_json::from_value(status_value).unwrap();

    collection.validate_with_status(&status).unwrap();
}

#[test]
fn utc_offset_outside_supported_range_is_rejected() {
    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["clock"]["utc_offset_minutes"] = json!(1_441);
    let collection: Collection = serde_json::from_value(value).unwrap();

    let errors = collection.validate().unwrap_err();
    assert!(
        errors
            .errors()
            .iter()
            .any(|error| error.path == "/clock/utc_offset_minutes")
    );
}
