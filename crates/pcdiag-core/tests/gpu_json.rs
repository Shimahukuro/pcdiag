use pcdiag_core::{Collection, GpuAdapterType};
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
