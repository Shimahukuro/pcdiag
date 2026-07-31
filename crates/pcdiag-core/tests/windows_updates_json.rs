use pcdiag_core::{
    Collection, CollectionMessage, CollectionStatus, CollectorName, CollectorResult,
    CollectorStatus, WindowsUpdateOperation, WindowsUpdateResult,
};
use serde_json::{Value, json};

const COLLECTION: &str = include_str!("fixtures/memory-success-collection.json");
const STATUS: &str = include_str!("fixtures/memory-success-status.json");

#[test]
fn windows_update_history_round_trips_and_validates() {
    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["windows_updates"]["history"] = json!([{
        "occurred_at": "2026-07-17T00:02:03.000Z",
        "title": "2026-07 Cumulative Update (KB5060001)",
        "kb_ids": ["KB5060001"],
        "operation": "installation",
        "operation_code": 1,
        "result": "succeeded",
        "result_code": 2,
        "hresult": 0,
        "update_id": "cdb1f3c1-7e92-4a48-8416-b72a09a3fd55",
        "revision_number": 1,
        "support_url": "https://support.microsoft.com/help/5060001",
        "client_application_id": "UpdateOrchestrator"
    }]);
    let collection: Collection = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        collection.windows_updates.history.as_ref().unwrap()[0].operation,
        WindowsUpdateOperation::Installation
    );
    assert_eq!(
        collection.windows_updates.history.as_ref().unwrap()[0].result,
        WindowsUpdateResult::Succeeded
    );
    assert_eq!(serde_json::to_value(&collection).unwrap(), value);

    let mut status: CollectionStatus = serde_json::from_str(STATUS).unwrap();
    status.collectors.push(CollectorResult {
        name: CollectorName::WindowsUpdates,
        status: CollectorStatus::Success,
        duration_ms: 10,
        messages: vec![CollectionMessage {
            code: "windows_update_history_truncated_by_date".into(),
            native_code: None,
            message: Some("older entries omitted".into()),
        }],
        fields: vec![],
    });
    collection.validate_with_status(&status).unwrap();
}

#[test]
fn missing_windows_update_category_is_rejected() {
    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value.as_object_mut().unwrap().remove("windows_updates");
    assert!(serde_json::from_value::<Collection>(value).is_err());
}

#[test]
fn invalid_windows_update_values_are_rejected() {
    let mut value: Value = serde_json::from_str(COLLECTION).unwrap();
    value["windows_updates"]["lookback_days"] = json!(0);
    value["windows_updates"]["history"] = json!([{
        "occurred_at": "not-a-date", "title": "", "kb_ids": ["kb1", "kb1"],
        "operation": "unknown", "operation_code": 99, "result": "unknown",
        "result_code": 99, "hresult": -1, "update_id": null,
        "revision_number": null, "support_url": null, "client_application_id": null
    }]);
    let collection: Collection = serde_json::from_value(value).unwrap();
    let errors = collection.validate().unwrap_err();
    assert!(
        errors
            .errors()
            .iter()
            .any(|error| error.path == "/windows_updates/lookback_days")
    );
    assert!(
        errors
            .errors()
            .iter()
            .any(|error| error.path == "/windows_updates/history/0/occurred_at")
    );
    assert!(
        errors
            .errors()
            .iter()
            .any(|error| error.path == "/windows_updates/history/0/title")
    );
    assert!(
        errors
            .errors()
            .iter()
            .any(|error| error.path == "/windows_updates/history/0/kb_ids/0")
    );
}
