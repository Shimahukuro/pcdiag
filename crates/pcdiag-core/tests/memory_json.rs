use pcdiag_core::{Collection, CollectionStatus, Diagnosis};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

const SUCCESS_COLLECTION: &str = include_str!("fixtures/memory-success-collection.json");
const SUCCESS_STATUS: &str = include_str!("fixtures/memory-success-status.json");
const PARTIAL_COLLECTION: &str = include_str!("fixtures/memory-partial-collection.json");
const PARTIAL_STATUS: &str = include_str!("fixtures/memory-partial-status.json");
const FAILED_COLLECTION: &str = include_str!("fixtures/memory-failed-collection.json");
const FAILED_STATUS: &str = include_str!("fixtures/memory-failed-status.json");
const WARNING_DIAGNOSIS: &str = include_str!("fixtures/memory-warning-diagnosis.json");

#[test]
fn success_fixtures_round_trip_and_validate() {
    let collection: Collection = assert_json_round_trip(SUCCESS_COLLECTION);
    let status: CollectionStatus = assert_json_round_trip(SUCCESS_STATUS);

    collection.validate_with_status(&status).unwrap();
}

#[test]
fn partial_fixtures_round_trip_and_validate() {
    let collection: Collection = assert_json_round_trip(PARTIAL_COLLECTION);
    let status: CollectionStatus = assert_json_round_trip(PARTIAL_STATUS);

    collection.validate_with_status(&status).unwrap();
}

#[test]
fn failed_fixtures_round_trip_and_validate() {
    let collection: Collection = assert_json_round_trip(FAILED_COLLECTION);
    let status: CollectionStatus = assert_json_round_trip(FAILED_STATUS);

    collection.validate_with_status(&status).unwrap();
}

#[test]
fn diagnosis_fixture_round_trips_and_matches_collection() {
    let collection: Collection = serde_json::from_str(SUCCESS_COLLECTION).unwrap();
    let diagnosis: Diagnosis = assert_json_round_trip(WARNING_DIAGNOSIS);

    diagnosis.validate_against(&collection).unwrap();
}

#[test]
fn enum_values_use_the_specified_snake_case_names() {
    let status: Value = serde_json::from_str(PARTIAL_STATUS).unwrap();
    let diagnosis: Value = serde_json::from_str(WARNING_DIAGNOSIS).unwrap();

    assert_eq!(status["collectors"][0]["status"], "partial");
    assert_eq!(
        status["collectors"][0]["fields"][0]["status"],
        "source_null"
    );
    assert_eq!(diagnosis["summary"]["overall_severity"], "warning");
    assert_eq!(diagnosis["evaluations"][0]["status"], "triggered");
    assert_eq!(
        diagnosis["evaluations"][0]["evidence"][2]["unit"],
        "percent"
    );
}

#[test]
fn missing_required_collection_field_is_rejected() {
    let mut value: Value = serde_json::from_str(SUCCESS_COLLECTION).unwrap();
    value["memory"]["physical"]
        .as_object_mut()
        .unwrap()
        .remove("total_bytes");

    assert!(serde_json::from_value::<Collection>(value).is_err());
}

#[test]
fn unknown_collector_status_is_rejected() {
    let mut value: Value = serde_json::from_str(SUCCESS_STATUS).unwrap();
    value["collectors"][0]["status"] = json!("completed");

    assert!(serde_json::from_value::<CollectionStatus>(value).is_err());
}

#[test]
fn load_percent_outside_the_valid_range_is_rejected_by_validation() {
    let mut value: Value = serde_json::from_str(SUCCESS_COLLECTION).unwrap();
    value["memory"]["physical"]["load_percent"] = json!(101.0);
    let collection: Collection = serde_json::from_value(value).unwrap();

    let errors = collection.validate().unwrap_err();
    assert!(errors.errors().iter().any(|error| {
        error.path == "/memory/physical/load_percent" && error.message.contains("between 0 and 100")
    }));
}

fn assert_json_round_trip<T>(source: &str) -> T
where
    T: DeserializeOwned + Serialize,
{
    let expected: Value = serde_json::from_str(source).unwrap();
    let parsed: T = serde_json::from_value(expected.clone()).unwrap();
    let actual = serde_json::to_value(&parsed).unwrap();

    assert_eq!(actual, expected);
    parsed
}
