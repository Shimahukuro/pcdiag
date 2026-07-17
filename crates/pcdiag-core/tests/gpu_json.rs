use pcdiag_core::{Collection, CollectionStatus, GpuAdapterType};
use serde_json::{Value, json};

const GPU_COLLECTION: &str = include_str!("fixtures/gpu-success-collection.json");

#[test]
fn gpu_collection_round_trips() {
    let expected: Value = serde_json::from_str(GPU_COLLECTION).unwrap();
    let collection: Collection = serde_json::from_value(expected.clone()).unwrap();

    assert_eq!(
        collection.gpus.as_ref().unwrap()[0].adapter_type,
        GpuAdapterType::Hardware
    );
    assert_eq!(serde_json::to_value(collection).unwrap(), expected);
}

#[test]
fn unknown_adapter_type_value_is_rejected() {
    let mut value: Value = serde_json::from_str(GPU_COLLECTION).unwrap();
    value["gpus"][0]["adapter_type"] = json!("virtual");

    assert!(serde_json::from_value::<Collection>(value).is_err());
}

#[test]
fn duplicate_device_instance_ids_are_rejected() {
    let mut value: Value = serde_json::from_str(GPU_COLLECTION).unwrap();
    let duplicate = value["gpus"][0].clone();
    value["gpus"].as_array_mut().unwrap().push(duplicate);
    let collection: Collection = serde_json::from_value(value).unwrap();

    assert!(collection.validate().is_err());
}

#[test]
fn software_gpu_nulls_are_valid_when_marked_not_applicable() {
    let mut value: Value = serde_json::from_str(GPU_COLLECTION).unwrap();
    value["gpus"][0]["adapter_type"] = json!("software");
    for pointer in [
        "/gpus/0/device_instance_id",
        "/gpus/0/driver/version",
        "/gpus/0/driver/date",
        "/gpus/0/device_state/enabled",
        "/gpus/0/device_state/problem_code",
    ] {
        *value.pointer_mut(pointer).unwrap() = Value::Null;
    }
    let collection: Collection = serde_json::from_value(value).unwrap();
    let fields: Vec<Value> = [
        "/gpus/0/device_instance_id",
        "/gpus/0/driver/version",
        "/gpus/0/driver/date",
        "/gpus/0/device_state/enabled",
        "/gpus/0/device_state/problem_code",
    ]
    .into_iter()
    .map(|path| {
        json!({
            "path": path,
            "status": "not_applicable",
            "code": "not_applicable"
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
                "name": "gpu",
                "status": "success",
                "duration_ms": 1,
                "messages": [],
                "fields": fields
            }
        ]
    }))
    .unwrap();

    collection.validate_with_status(&status).unwrap();
}

#[test]
fn empty_gpu_array_means_enumeration_succeeded_without_a_gpu() {
    let mut value: Value = serde_json::from_str(GPU_COLLECTION).unwrap();
    value["gpus"] = json!([]);

    let collection: Collection = serde_json::from_value(value).unwrap();
    assert_eq!(collection.gpus, Some(vec![]));
}

#[test]
fn null_gpu_array_means_enumeration_did_not_produce_a_result() {
    let mut value: Value = serde_json::from_str(GPU_COLLECTION).unwrap();
    value["gpus"] = Value::Null;

    let collection: Collection = serde_json::from_value(value).unwrap();
    assert_eq!(collection.gpus, None);
}

#[test]
fn missing_gpu_array_is_rejected() {
    let mut value: Value = serde_json::from_str(GPU_COLLECTION).unwrap();
    value.as_object_mut().unwrap().remove("gpus");

    assert!(serde_json::from_value::<Collection>(value).is_err());
}

#[test]
fn missing_gpu_field_is_rejected_but_explicit_null_is_allowed() {
    let mut missing: Value = serde_json::from_str(GPU_COLLECTION).unwrap();
    missing["gpus"][0]["memory"]
        .as_object_mut()
        .unwrap()
        .remove("dedicated_video_bytes");
    assert!(serde_json::from_value::<Collection>(missing).is_err());

    let mut explicit_null: Value = serde_json::from_str(GPU_COLLECTION).unwrap();
    explicit_null["gpus"][0]["memory"]["dedicated_video_bytes"] = Value::Null;
    let collection: Collection = serde_json::from_value(explicit_null).unwrap();
    assert_eq!(
        collection.gpus.unwrap()[0].memory.dedicated_video_bytes,
        None
    );
}
