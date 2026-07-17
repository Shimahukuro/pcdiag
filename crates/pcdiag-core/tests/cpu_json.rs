use pcdiag_core::{Collection, CollectionStatus};
use serde_json::{Value, json};

const COLLECTION: &str = include_str!("fixtures/memory-success-collection.json");
const MEMORY_STATUS: &str = include_str!("fixtures/memory-success-status.json");
const FAILED_COLLECTION: &str = include_str!("fixtures/memory-failed-collection.json");
const FAILED_STATUS: &str = include_str!("fixtures/memory-failed-status.json");

#[test]
fn cpu_information_round_trips() {
    let expected: Value = serde_json::from_str(COLLECTION).unwrap();
    let collection: Collection = serde_json::from_value(expected.clone()).unwrap();

    assert_eq!(serde_json::to_value(collection).unwrap(), expected);
}

#[test]
fn missing_cpu_field_is_rejected_but_null_is_allowed() {
    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["cpu"]["features"]
        .as_object_mut()
        .unwrap()
        .remove("hardware_virtualization_supported");
    assert!(serde_json::from_value::<Collection>(value).is_err());

    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["cpu"]["features"]["hardware_virtualization_supported"] = Value::Null;
    assert!(serde_json::from_value::<Collection>(value).is_ok());
}

#[test]
fn unknown_instruction_set_is_rejected() {
    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["cpu"]["features"]["available_instruction_sets"] = json!(["future_extension"]);

    assert!(serde_json::from_value::<Collection>(value).is_err());
}

#[test]
fn duplicate_package_indexes_are_rejected() {
    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    let package = value["cpu"]["packages"][0].clone();
    value["cpu"]["packages"]
        .as_array_mut()
        .unwrap()
        .push(package);
    value["cpu"]["topology"]["physical_packages"] = json!(2);
    value["cpu"]["topology"]["physical_cores"] = json!(28);
    value["cpu"]["topology"]["logical_processors"] = json!(40);
    let collection: Collection = serde_json::from_value(value).unwrap();

    let errors = collection.validate().unwrap_err();
    assert!(errors.errors().iter().any(|error| {
        error.path == "/cpu/packages/1/package_index" && error.message.contains("unique")
    }));
}

#[test]
fn topology_must_match_package_sums() {
    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["cpu"]["topology"]["physical_cores"] = json!(12);
    let collection: Collection = serde_json::from_value(value).unwrap();

    let errors = collection.validate().unwrap_err();
    assert!(errors.errors().iter().any(|error| {
        error.path == "/cpu/topology/physical_cores" && error.message.contains("sum")
    }));
}

#[test]
fn duplicate_instruction_sets_are_rejected() {
    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["cpu"]["features"]["available_instruction_sets"] = json!(["sse2", "sse2"]);
    let collection: Collection = serde_json::from_value(value).unwrap();

    let errors = collection.validate().unwrap_err();
    assert!(errors.errors().iter().any(|error| {
        error.path == "/cpu/features/available_instruction_sets"
            && error.message.contains("duplicate")
    }));
}

#[test]
fn partial_cpu_collection_accepts_explained_null() {
    let mut collection_value: Value = serde_json::from_str(COLLECTION).unwrap();
    collection_value["cpu"]["packages"][0]["model"] = Value::Null;
    let collection: Collection = serde_json::from_value(collection_value).unwrap();

    let mut status_value: Value = serde_json::from_str(MEMORY_STATUS).unwrap();
    status_value["collectors"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "name": "cpu",
            "status": "partial",
            "duration_ms": 1,
            "messages": [],
            "fields": [{
                "path": "/cpu/packages/0/model",
                "status": "unsupported",
                "code": "cpu_model_unsupported"
            }]
        }));
    let status: CollectionStatus = serde_json::from_value(status_value).unwrap();

    collection.validate_with_status(&status).unwrap();
}

#[test]
fn failed_cpu_collection_accepts_all_null_values_with_a_reason() {
    let collection: Collection = serde_json::from_str(FAILED_COLLECTION).unwrap();
    let mut status_value: Value = serde_json::from_str(FAILED_STATUS).unwrap();
    status_value["collectors"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "name": "cpu",
            "status": "failed",
            "duration_ms": 1,
            "messages": [{"code": "cpu_enumeration_failed"}],
            "fields": []
        }));
    let status: CollectionStatus = serde_json::from_value(status_value).unwrap();

    collection.validate_with_status(&status).unwrap();
}
