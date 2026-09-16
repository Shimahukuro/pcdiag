use pcdiag_core::{
    Collection, CollectionStatus, CollectorStatus, WindowsSecurityCollection, diagnose_collection,
};
use serde_json::{Value, json};

const BASE: &str = include_str!("fixtures/memory-success-collection.json");
const STATUS: &str = include_str!("fixtures/memory-success-status.json");
const SECURITY: &str = include_str!("fixtures/windows-security.json");

fn fixture() -> (Collection, CollectionStatus) {
    let mut collection: Collection = serde_json::from_str(BASE).unwrap();
    collection.windows_security = Some(serde_json::from_str(SECURITY).unwrap());
    let mut status: CollectionStatus = serde_json::from_str(STATUS).unwrap();
    status.collectors.push(serde_json::from_value(json!({
        "name": "windows_security", "status": "success", "duration_ms": 1,
        "messages": [{"code":"wsc_not_monitored", "message":"Security Centerの監視対象外です: /windows_security/internet_settings"}], "fields": []
    })).unwrap());
    (collection, status)
}

#[test]
fn security_and_status_round_trip_without_changing_diagnosis() {
    let (collection, status) = fixture();
    collection.validate_with_status(&status).unwrap();
    let json = serde_json::to_value(&collection).unwrap();
    assert_eq!(
        json["windows_security"],
        serde_json::from_str::<Value>(SECURITY).unwrap()
    );
    assert_eq!(
        serde_json::from_value::<Collection>(json).unwrap(),
        collection
    );
    assert_eq!(
        serde_json::from_slice::<CollectionStatus>(&serde_json::to_vec(&status).unwrap()).unwrap(),
        status
    );
    let old: Collection = serde_json::from_str(BASE).unwrap();
    assert_eq!(diagnose_collection(&collection), diagnose_collection(&old));
}

#[test]
fn older_collection_remains_readable_and_does_not_invent_security_facts() {
    let collection: Collection = serde_json::from_str(BASE).unwrap();
    assert!(collection.windows_security.is_none());
    assert_eq!(
        serde_json::to_value(collection).unwrap(),
        serde_json::from_str::<Value>(BASE).unwrap()
    );
}

#[test]
fn security_requires_explicit_nullable_fields_and_known_enum_values() {
    let original: Value = serde_json::from_str(SECURITY).unwrap();
    for field in [
        "firewall",
        "automatic_updates",
        "antivirus",
        "internet_settings",
        "user_account_control",
        "security_center_service",
    ] {
        let mut value = original.clone();
        value.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<WindowsSecurityCollection>(value).is_err());
        let mut value = original.clone();
        value[field] = Value::Null;
        assert!(serde_json::from_value::<WindowsSecurityCollection>(value).is_ok());
        let mut value = original.clone();
        value[field] = json!("unknown");
        assert!(serde_json::from_value::<WindowsSecurityCollection>(value).is_err());
    }
    for field in ["configured", "running"] {
        let mut value = original.clone();
        value["memory_integrity"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(serde_json::from_value::<WindowsSecurityCollection>(value).is_err());
        let mut value = original.clone();
        value["memory_integrity"][field] = json!(true);
        assert!(serde_json::from_value::<WindowsSecurityCollection>(value).is_err());
    }
}

#[test]
fn partial_collection_requires_matching_unique_null_reasons() {
    let (mut collection, mut status) = fixture();
    collection.windows_security.as_mut().unwrap().firewall = None;
    status.collectors.last_mut().unwrap().status = CollectorStatus::Partial;
    assert!(collection.validate_with_status(&status).is_err());
    let reason = serde_json::from_value(json!({"path":"/windows_security/firewall", "status":"not_collected", "code":"wsc_service_not_running", "native_code":1})).unwrap();
    status.collectors.last_mut().unwrap().fields.push(reason);
    collection.validate_with_status(&status).unwrap();
    let mut duplicate = status.clone();
    let collector = duplicate.collectors.last_mut().unwrap();
    collector.fields.push(collector.fields[0].clone());
    assert!(collection.validate_with_status(&duplicate).is_err());
    status.collectors.last_mut().unwrap().fields[0].path = "/windows_security/antivirus".into();
    assert!(collection.validate_with_status(&status).is_err());
}

#[test]
fn absent_duplicate_and_inconsistent_collector_status_are_rejected() {
    let (collection, status) = fixture();
    let mut missing = status.clone();
    missing.collectors.pop();
    assert!(collection.validate_with_status(&missing).is_err());
    let mut duplicate = status.clone();
    duplicate
        .collectors
        .push(status.collectors.last().unwrap().clone());
    assert!(collection.validate_with_status(&duplicate).is_err());
    let mut failed = status.clone();
    failed.collectors.last_mut().unwrap().status = CollectorStatus::Failed;
    assert!(collection.validate_with_status(&failed).is_err());
    let old: Collection = serde_json::from_str(BASE).unwrap();
    assert!(old.validate_with_status(&status).is_err());
}

#[test]
fn total_worker_failure_uses_collector_reason_and_keeps_null_values() {
    let (mut collection, mut status) = fixture();
    collection.windows_security = Some(WindowsSecurityCollection::default());
    let collector = status.collectors.last_mut().unwrap();
    collector.status = CollectorStatus::Failed;
    collector.messages = vec![
        serde_json::from_value(json!({"code":"collector_timeout", "message":"timed out"})).unwrap(),
    ];
    collection.validate_with_status(&status).unwrap();
    status.collectors.last_mut().unwrap().messages.clear();
    assert!(collection.validate_with_status(&status).is_err());
}
