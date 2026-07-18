use serde::{Deserialize, Serialize};

use super::memory::deserialize_required_nullable;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectedDevice {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub name: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub manufacturer: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub class: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub class_guid: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub device_instance_id: Option<String>,
    pub device_state: DeviceState,
    pub driver: DeviceDriver,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceState {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub present: Option<bool>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub started: Option<bool>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub problem_code: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceDriver {
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub version: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub date: Option<String>,
}
