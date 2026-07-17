use pcdiag_core::{Collection, CollectionStatus};
use serde_json::{Value, json};

const BASE_COLLECTION: &str = include_str!("fixtures/memory-success-collection.json");

#[test]
fn connected_devices_round_trip() {
    let mut expected: Value = serde_json::from_str(BASE_COLLECTION).unwrap();
    expected["devices"] = json!([
        {
            "name": "Example Device",
            "manufacturer": "Example Vendor",
            "class": "USB",
            "class_guid": "{00000000-0000-0000-0000-000000000000}",
            "device_instance_id": "USB\\VID_1234&PID_5678\\TEST",
            "device_state": {
                "present": true,
                "enabled": true,
                "problem_code": 0
            },
            "driver": {
                "version": "1.2.3.4",
                "date": "2026-07-17"
            }
        }
    ]);

    let collection: Collection = serde_json::from_value(expected.clone()).unwrap();
    assert_eq!(serde_json::to_value(collection).unwrap(), expected);
}

#[test]
fn missing_device_field_is_rejected_but_null_is_allowed() {
    let mut value: Value = serde_json::from_str(BASE_COLLECTION).unwrap();
    value["devices"] = json!([{
        "name": "Example Device",
        "manufacturer": null,
        "class": "USB",
        "class_guid": null,
        "device_instance_id": null,
        "device_state": {"present": false, "enabled": null, "problem_code": null},
        "driver": {"version": null, "date": null}
    }]);
    assert!(serde_json::from_value::<Collection>(value.clone()).is_ok());

    value["devices"][0].as_object_mut().unwrap().remove("name");
    assert!(serde_json::from_value::<Collection>(value).is_err());
}

#[test]
fn absent_device_state_is_valid_when_marked_not_applicable() {
    let collection = absent_device_collection();
    let status = device_status("success", "not_applicable");

    collection.validate_with_status(&status).unwrap();
}

#[test]
fn absent_device_state_must_not_be_reported_as_source_null() {
    let collection = absent_device_collection();
    let status = device_status("partial", "source_null");

    let errors = collection.validate_with_status(&status).unwrap_err();
    assert!(errors.errors().iter().any(|error| {
        error.path == "/devices/0/device_state/enabled"
            && error.message.contains("must be not_applicable")
    }));
}

#[test]
fn duplicate_device_instance_ids_are_rejected() {
    let mut value: Value = serde_json::from_str(BASE_COLLECTION).unwrap();
    let device = present_device();
    value["devices"] = json!([device.clone(), device]);
    let collection: Collection = serde_json::from_value(value).unwrap();

    assert!(collection.validate().is_err());
}

fn absent_device_collection() -> Collection {
    let mut value: Value = serde_json::from_str(BASE_COLLECTION).unwrap();
    value["devices"] = json!([{
        "name": "Previously Connected Device",
        "manufacturer": "Example Vendor",
        "class": "USB",
        "class_guid": "{00000000-0000-0000-0000-000000000000}",
        "device_instance_id": "USB\\VID_1234&PID_5678\\OLD",
        "device_state": {"present": false, "enabled": null, "problem_code": null},
        "driver": {"version": "1.2.3.4", "date": "2026-07-17"}
    }]);
    serde_json::from_value(value).unwrap()
}

fn present_device() -> Value {
    json!({
        "name": "Example Device",
        "manufacturer": "Example Vendor",
        "class": "USB",
        "class_guid": "{00000000-0000-0000-0000-000000000000}",
        "device_instance_id": "USB\\VID_1234&PID_5678\\TEST",
        "device_state": {"present": true, "enabled": true, "problem_code": 0},
        "driver": {"version": "1.2.3.4", "date": "2026-07-17"}
    })
}

fn device_status(collector_status: &str, field_status: &str) -> CollectionStatus {
    serde_json::from_value(json!({
        "collectors": [
            {
                "name": "memory",
                "status": "success",
                "duration_ms": 1,
                "messages": [],
                "fields": []
            },
            {
                "name": "devices",
                "status": collector_status,
                "duration_ms": 1,
                "messages": [],
                "fields": [
                    {
                        "path": "/devices/0/device_state/enabled",
                        "status": field_status,
                        "code": "device_not_present"
                    },
                    {
                        "path": "/devices/0/device_state/problem_code",
                        "status": field_status,
                        "code": "device_not_present"
                    }
                ]
            }
        ]
    }))
    .unwrap()
}
