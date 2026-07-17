use pcdiag_core::{Collection, CollectionStatus, DiskBusType};
use serde_json::{Value, json};

const BASE_COLLECTION: &str = include_str!("fixtures/memory-success-collection.json");

#[test]
fn physical_disks_round_trip() {
    let mut expected: Value = serde_json::from_str(BASE_COLLECTION).unwrap();
    expected["storage"]["disks"] = json!([disk()]);

    let collection: Collection = serde_json::from_value(expected.clone()).unwrap();

    assert_eq!(
        collection.storage.disks.as_ref().unwrap()[0].bus_type,
        Some(DiskBusType::Usb)
    );
    assert_eq!(serde_json::to_value(collection).unwrap(), expected);
}

#[test]
fn missing_disk_field_is_rejected_but_null_is_allowed() {
    let mut value: Value = serde_json::from_str(BASE_COLLECTION).unwrap();
    value["storage"]["disks"] = json!([{
        "number": 0,
        "model": null,
        "manufacturer": null,
        "firmware_revision": null,
        "bus_type": null,
        "capacity_bytes": null,
        "logical_sector_size_bytes": null,
        "removable": null
    }]);
    assert!(serde_json::from_value::<Collection>(value.clone()).is_ok());

    value["storage"]["disks"][0]
        .as_object_mut()
        .unwrap()
        .remove("capacity_bytes");
    assert!(serde_json::from_value::<Collection>(value).is_err());
}

#[test]
fn unknown_bus_type_is_rejected() {
    let mut value: Value = serde_json::from_str(BASE_COLLECTION).unwrap();
    let mut disk = disk();
    disk["bus_type"] = json!("pcie");
    value["storage"]["disks"] = json!([disk]);

    assert!(serde_json::from_value::<Collection>(value).is_err());
}

#[test]
fn duplicate_disk_numbers_are_rejected() {
    let mut value: Value = serde_json::from_str(BASE_COLLECTION).unwrap();
    value["storage"]["disks"] = json!([disk(), disk()]);
    let collection: Collection = serde_json::from_value(value).unwrap();

    assert!(collection.validate().is_err());
}

#[test]
fn partial_disk_collection_requires_a_reason_for_null() {
    let mut value: Value = serde_json::from_str(BASE_COLLECTION).unwrap();
    let mut disk = disk();
    disk["firmware_revision"] = Value::Null;
    value["storage"]["disks"] = json!([disk]);
    let collection: Collection = serde_json::from_value(value).unwrap();
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
                "name": "physical_disks",
                "status": "partial",
                "duration_ms": 1,
                "messages": [],
                "fields": [{
                    "path": "/storage/disks/0/firmware_revision",
                    "status": "source_null",
                    "code": "disk_firmware_revision_unavailable"
                }]
            }
        ]
    }))
    .unwrap();

    collection.validate_with_status(&status).unwrap();
}

fn disk() -> Value {
    json!({
        "number": 2,
        "model": "Example USB Disk",
        "manufacturer": "Example Vendor",
        "firmware_revision": "1.0",
        "bus_type": "usb",
        "capacity_bytes": 32000000000_u64,
        "logical_sector_size_bytes": 512,
        "removable": true
    })
}
