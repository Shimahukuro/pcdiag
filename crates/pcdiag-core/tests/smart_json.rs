use pcdiag_core::{Collection, CollectionStatus, SmartProtocol};
use serde_json::{Value, json};

const BASE_COLLECTION: &str = include_str!("fixtures/memory-success-collection.json");

#[test]
fn nvme_smart_round_trip() {
    let value = nvme_collection();
    let collection: Collection = serde_json::from_value(value.clone()).unwrap();

    assert_eq!(
        collection.storage.smart.as_ref().unwrap()[0].protocol,
        SmartProtocol::Nvme
    );
    assert_eq!(serde_json::to_value(collection).unwrap(), value);
}

#[test]
fn smart_disk_number_must_refer_to_a_physical_disk() {
    let mut value = nvme_collection();
    value["storage"]["smart"][0]["disk_number"] = json!(9);
    let collection: Collection = serde_json::from_value(value).unwrap();

    assert!(collection.validate().is_err());
}

#[test]
fn permission_denied_nulls_are_valid_for_partial_collection() {
    let mut value = nvme_collection();
    value["storage"]["smart"][0] = json!({
        "disk_number": 0,
        "protocol": "unknown",
        "predict_failure": null,
        "critical_warning": null,
        "temperature_celsius": null,
        "available_spare_percent": null,
        "percentage_used": null,
        "power_on_hours": null,
        "unsafe_shutdowns": null,
        "media_errors": null
    });
    let collection: Collection = serde_json::from_value(value).unwrap();
    let fields: Vec<_> = [
        "predict_failure",
        "critical_warning",
        "temperature_celsius",
        "available_spare_percent",
        "percentage_used",
        "power_on_hours",
        "unsafe_shutdowns",
        "media_errors",
    ]
    .into_iter()
    .map(|field| {
        json!({
            "path": format!("/storage/smart/0/{field}"),
            "status": "permission_denied",
            "code": "smart_permission_denied",
            "native_code": -2147024891_i64
        })
    })
    .collect();
    let status: CollectionStatus = serde_json::from_value(json!({
        "collectors": [
            {
                "name": "memory",
                "status": "success",
                "duration_ms": 1,
                "messages": [],
                "fields": []
            },
            {
                "name": "smart",
                "status": "partial",
                "duration_ms": 1,
                "messages": [],
                "fields": fields
            }
        ]
    }))
    .unwrap();

    collection.validate_with_status(&status).unwrap();
}

fn nvme_collection() -> Value {
    let mut value: Value = serde_json::from_str(BASE_COLLECTION).unwrap();
    value["storage"]["disks"] = json!([{
        "number": 0,
        "model": "Example NVMe",
        "manufacturer": "Example Vendor",
        "firmware_revision": "1.0",
        "bus_type": "nvme",
        "capacity_bytes": 1_000_000_000_000_u64,
        "logical_sector_size_bytes": 512,
        "removable": false
    }]);
    value["storage"]["smart"] = json!([{
        "disk_number": 0,
        "protocol": "nvme",
        "predict_failure": null,
        "critical_warning": 0,
        "temperature_celsius": 38,
        "available_spare_percent": 100,
        "percentage_used": 4,
        "power_on_hours": 1200,
        "unsafe_shutdowns": 2,
        "media_errors": 0
    }]);
    value
}
