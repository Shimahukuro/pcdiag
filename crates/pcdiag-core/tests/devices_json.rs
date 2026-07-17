use pcdiag_core::Collection;
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
