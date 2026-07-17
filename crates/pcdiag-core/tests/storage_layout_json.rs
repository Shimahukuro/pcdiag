use pcdiag_core::{Collection, CollectionStatus, PartitionStyle};
use serde_json::{Value, json};

const BASE_COLLECTION: &str = include_str!("fixtures/memory-success-collection.json");

#[test]
fn partition_and_volume_round_trip() {
    let expected = storage_collection();
    let collection: Collection = serde_json::from_value(expected.clone()).unwrap();

    assert_eq!(
        collection.storage.partitions.as_ref().unwrap()[0].style,
        PartitionStyle::Gpt
    );
    assert_eq!(
        collection.storage.volumes.as_ref().unwrap()[0].mount_points,
        Some(vec!["C:\\".into()])
    );
    assert_eq!(serde_json::to_value(collection).unwrap(), expected);
}

#[test]
fn partition_must_refer_to_a_collected_disk() {
    let mut value = storage_collection();
    value["storage"]["partitions"][0]["disk_number"] = json!(99);
    let collection: Collection = serde_json::from_value(value).unwrap();

    assert!(collection.validate().is_err());
}

#[test]
fn volume_free_space_must_not_exceed_capacity() {
    let mut value = storage_collection();
    value["storage"]["volumes"][0]["free_bytes"] = json!(101_000_000_000_u64);
    let collection: Collection = serde_json::from_value(value).unwrap();

    assert!(collection.validate().is_err());
}

#[test]
fn gpt_bootable_null_is_valid_when_not_applicable() {
    let collection: Collection = serde_json::from_value(storage_collection()).unwrap();
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
                "name": "partitions",
                "status": "success",
                "duration_ms": 1,
                "messages": [],
                "fields": [{
                    "path": "/storage/partitions/0/bootable",
                    "status": "not_applicable",
                    "code": "boot_indicator_only_applies_to_mbr"
                }]
            },
            {
                "name": "volumes",
                "status": "success",
                "duration_ms": 1,
                "messages": [],
                "fields": []
            }
        ]
    }))
    .unwrap();

    collection.validate_with_status(&status).unwrap();
}

fn storage_collection() -> Value {
    let mut value: Value = serde_json::from_str(BASE_COLLECTION).unwrap();
    value["storage"] = json!({
        "disks": [{
            "number": 0,
            "model": "Example Disk",
            "manufacturer": "Example Vendor",
            "firmware_revision": "1.0",
            "bus_type": "nvme",
            "capacity_bytes": 100_000_000_000_u64,
            "logical_sector_size_bytes": 512,
            "removable": false
        }],
        "partitions": [{
            "disk_number": 0,
            "partition_number": 1,
            "offset_bytes": 1_048_576,
            "length_bytes": 99_000_000_000_u64,
            "style": "gpt",
            "type_id": "{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}",
            "bootable": null
        }],
        "volumes": [{
            "mount_points": ["C:\\"],
            "file_system": "NTFS",
            "capacity_bytes": 99_000_000_000_u64,
            "free_bytes": 50_000_000_000_u64,
            "extents": [{
                "disk_number": 0,
                "offset_bytes": 1_048_576,
                "length_bytes": 99_000_000_000_u64
            }]
        }]
    });
    value
}
