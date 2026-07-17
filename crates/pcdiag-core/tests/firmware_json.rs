use pcdiag_core::{Collection, CollectionStatus};
use serde_json::{Value, json};

const COLLECTION: &str = include_str!("fixtures/memory-success-collection.json");
const MEMORY_STATUS: &str = include_str!("fixtures/memory-success-status.json");

#[test]
fn firmware_information_round_trips() {
    let expected: Value = serde_json::from_str(COLLECTION).unwrap();
    let collection: Collection = serde_json::from_value(expected.clone()).unwrap();

    assert_eq!(serde_json::to_value(collection).unwrap(), expected);
}

#[test]
fn missing_firmware_field_is_rejected_but_null_is_allowed() {
    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["firmware"]
        .as_object_mut()
        .unwrap()
        .remove("secure_boot_enabled");
    assert!(serde_json::from_value::<Collection>(value).is_err());

    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["firmware"]["secure_boot_enabled"] = Value::Null;
    assert!(serde_json::from_value::<Collection>(value).is_ok());
}

#[test]
fn release_date_must_be_a_valid_iso_date() {
    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["firmware"]["release_date"] = json!("2026-02-30");
    let collection: Collection = serde_json::from_value(value).unwrap();

    let errors = collection.validate().unwrap_err();
    assert!(
        errors
            .errors()
            .iter()
            .any(|error| error.path == "/firmware/release_date")
    );
}

#[test]
fn intentionally_unsupported_status_is_explained() {
    let collection: Collection = serde_json::from_str(COLLECTION).unwrap();
    let mut status_value: Value = serde_json::from_str(MEMORY_STATUS).unwrap();
    status_value["collectors"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "name": "firmware",
            "status": "partial",
            "duration_ms": 1,
            "messages": [],
            "fields": [{
                "path": "/firmware/status",
                "status": "unsupported",
                "code": "firmware_operational_status_unsupported"
            }]
        }));
    let status: CollectionStatus = serde_json::from_value(status_value).unwrap();

    collection.validate_with_status(&status).unwrap();
}

#[test]
fn secure_boot_value_is_not_valid_for_legacy_bios() {
    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["firmware"]["interface_type"] = json!("bios");
    let collection: Collection = serde_json::from_value(value).unwrap();

    let errors = collection.validate().unwrap_err();
    assert!(
        errors
            .errors()
            .iter()
            .any(|error| error.path == "/firmware/secure_boot_enabled")
    );
}
