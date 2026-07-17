use pcdiag_core::{Collection, CollectionStatus};
use serde_json::{Value, json};

const COLLECTION: &str = include_str!("fixtures/memory-success-collection.json");
const MEMORY_STATUS: &str = include_str!("fixtures/memory-success-status.json");

#[test]
fn windows_information_round_trips() {
    let expected: Value = serde_json::from_str(COLLECTION).unwrap();
    let collection: Collection = serde_json::from_value(expected.clone()).unwrap();

    assert_eq!(serde_json::to_value(collection).unwrap(), expected);
}

#[test]
fn missing_windows_field_is_rejected_but_null_is_allowed() {
    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["windows"]
        .as_object_mut()
        .unwrap()
        .remove("boot_mode");
    assert!(serde_json::from_value::<Collection>(value).is_err());

    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["windows"]["boot_mode"] = Value::Null;
    assert!(serde_json::from_value::<Collection>(value).is_ok());
}

#[test]
fn partial_windows_collection_requires_reasons_for_nulls() {
    let mut collection_value: Value = serde_json::from_str(COLLECTION).unwrap();
    collection_value["windows"]["edition"] = Value::Null;
    let collection: Collection = serde_json::from_value(collection_value).unwrap();

    let mut status_value: Value = serde_json::from_str(MEMORY_STATUS).unwrap();
    status_value["collectors"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "name": "windows",
            "status": "partial",
            "duration_ms": 1,
            "messages": [{"code": "windows_edition_failed"}],
            "fields": []
        }));
    let status: CollectionStatus = serde_json::from_value(status_value).unwrap();

    let errors = collection.validate_with_status(&status).unwrap_err();
    assert!(
        errors
            .errors()
            .iter()
            .any(|error| error.path == "/windows/edition")
    );
}

#[test]
fn partial_windows_collection_accepts_explained_null() {
    let mut collection_value: Value = serde_json::from_str(COLLECTION).unwrap();
    collection_value["windows"]["edition"] = Value::Null;
    let collection: Collection = serde_json::from_value(collection_value).unwrap();

    let mut status_value: Value = serde_json::from_str(MEMORY_STATUS).unwrap();
    status_value["collectors"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "name": "windows",
            "status": "partial",
            "duration_ms": 1,
            "messages": [{"code": "windows_edition_failed"}],
            "fields": [{
                "path": "/windows/edition",
                "status": "failed",
                "code": "windows_edition_failed"
            }]
        }));
    let status: CollectionStatus = serde_json::from_value(status_value).unwrap();

    collection.validate_with_status(&status).unwrap();
}
