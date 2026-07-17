use std::{collections::HashSet, fmt};

use serde_json::Value;

use crate::{
    Collection, CollectionStatus, CollectorName, CollectorResult, CollectorStatus, Diagnosis,
    Evidence, RuleEvaluationStatus,
};

const MEMORY_PATHS: [&str; 7] = [
    "/memory/physical/total_bytes",
    "/memory/physical/available_bytes",
    "/memory/physical/load_percent",
    "/memory/commit/limit_bytes",
    "/memory/commit/available_bytes",
    "/memory/virtual/total_bytes",
    "/memory/virtual/available_bytes",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationErrors(Vec<ValidationError>);

impl ValidationErrors {
    pub fn errors(&self) -> &[ValidationError] {
        &self.0
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} validation error(s)", self.0.len())
    }
}

impl std::error::Error for ValidationErrors {}

impl Collection {
    pub fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = Vec::new();
        let physical = &self.memory.physical;

        if let Some(load) = physical.load_percent
            && (!load.is_finite() || !(0.0..=100.0).contains(&load))
        {
            push_error(
                &mut errors,
                "/memory/physical/load_percent",
                "must be a finite number between 0 and 100",
            );
        }

        validate_available_not_greater_than_total(
            &mut errors,
            "/memory/physical",
            physical.available_bytes,
            physical.total_bytes,
        );

        if let Some(gpus) = &self.gpus {
            let mut instance_ids = HashSet::new();
            for (index, gpu) in gpus.iter().enumerate() {
                if let Some(instance_id) = &gpu.device_instance_id
                    && !instance_ids.insert(instance_id)
                {
                    push_error(
                        &mut errors,
                        format!("/gpus/{index}/device_instance_id"),
                        "must be unique within the GPU collection",
                    );
                }
            }
        }
        if let Some(devices) = &self.devices {
            let mut instance_ids = HashSet::new();
            for (index, device) in devices.iter().enumerate() {
                if let Some(instance_id) = &device.device_instance_id
                    && !instance_ids.insert(instance_id)
                {
                    push_error(
                        &mut errors,
                        format!("/devices/{index}/device_instance_id"),
                        "must be unique within the device collection",
                    );
                }
            }
        }
        validate_available_not_greater_than_total(
            &mut errors,
            "/memory/commit",
            self.memory.commit.available_bytes,
            self.memory.commit.limit_bytes,
        );
        validate_available_not_greater_than_total(
            &mut errors,
            "/memory/virtual",
            self.memory.virtual_memory.available_bytes,
            self.memory.virtual_memory.total_bytes,
        );

        finish(errors)
    }

    pub fn validate_with_status(&self, status: &CollectionStatus) -> Result<(), ValidationErrors> {
        let mut errors = self.validate().err().map_or_else(Vec::new, |e| e.0);
        let memory_collectors: Vec<_> = status
            .collectors
            .iter()
            .filter(|collector| collector.name == CollectorName::Memory)
            .collect();

        if memory_collectors.len() != 1 {
            push_error(
                &mut errors,
                "/collectors",
                "must contain exactly one memory collector result",
            );
            return finish(errors);
        }

        validate_memory_status(self, memory_collectors[0], &mut errors);
        let gpu_collectors: Vec<_> = status
            .collectors
            .iter()
            .filter(|collector| collector.name == CollectorName::Gpu)
            .collect();
        if gpu_collectors.len() > 1 {
            push_error(
                &mut errors,
                "/collectors",
                "must not contain more than one GPU collector result",
            );
        } else if let Some(collector) = gpu_collectors.first() {
            validate_gpu_status(self, collector, &mut errors);
        }
        let device_collectors: Vec<_> = status
            .collectors
            .iter()
            .filter(|collector| collector.name == CollectorName::Devices)
            .collect();
        if device_collectors.len() > 1 {
            push_error(
                &mut errors,
                "/collectors",
                "must not contain more than one device collector result",
            );
        } else if let Some(collector) = device_collectors.first() {
            validate_device_status(self, collector, &mut errors);
        }
        finish(errors)
    }
}

fn validate_device_status(
    collection: &Collection,
    collector: &CollectorResult,
    errors: &mut Vec<ValidationError>,
) {
    let collection_value = serde_json::to_value(collection).expect("collection must serialize");

    match collector.status {
        CollectorStatus::Success | CollectorStatus::Partial => {
            let Some(devices) = &collection.devices else {
                push_error(
                    errors,
                    "/devices",
                    "successful or partial device collectors require a device array",
                );
                return;
            };

            for field in &collector.fields {
                match collection_value.pointer(&field.path) {
                    Some(Value::Null) => {}
                    Some(_) => push_error(
                        errors,
                        &field.path,
                        "field collection status must refer to a null value",
                    ),
                    None => push_error(
                        errors,
                        &field.path,
                        "field collection status refers to an unknown path",
                    ),
                }
            }

            for path in device_null_paths(devices) {
                if !collector.fields.iter().any(|field| field.path == path) {
                    push_error(
                        errors,
                        path,
                        "null device value must have a field collection status",
                    );
                }
            }

            for (index, device) in devices.iter().enumerate() {
                if device.device_state.present == Some(false) {
                    for suffix in ["enabled", "problem_code"] {
                        let path = format!("/devices/{index}/device_state/{suffix}");
                        if collection_value.pointer(&path) == Some(&Value::Null)
                            && let Some(field) =
                                collector.fields.iter().find(|field| field.path == path)
                            && field.status != crate::FieldCollectionStatus::NotApplicable
                        {
                            push_error(
                                errors,
                                path,
                                "state unavailable because a device is absent must be not_applicable",
                            );
                        }
                    }
                }
            }

            if collector.status == CollectorStatus::Success
                && collector
                    .fields
                    .iter()
                    .any(|field| field.status != crate::FieldCollectionStatus::NotApplicable)
            {
                push_error(
                    errors,
                    "/collectors/devices/status",
                    "success may only contain not_applicable field statuses",
                );
            }
        }
        CollectorStatus::Skipped | CollectorStatus::Failed => {
            if collection.devices.is_some() {
                push_error(
                    errors,
                    "/devices",
                    "skipped or failed device collectors require a null device collection",
                );
            }
            if collector.messages.is_empty() {
                push_error(
                    errors,
                    "/collectors/devices/messages",
                    "skipped or failed device collectors must include a reason",
                );
            }
        }
    }
}

fn validate_gpu_status(
    collection: &Collection,
    collector: &CollectorResult,
    errors: &mut Vec<ValidationError>,
) {
    let collection_value = serde_json::to_value(collection).expect("collection must serialize");

    match collector.status {
        CollectorStatus::Success | CollectorStatus::Partial => {
            if collection.gpus.is_none() {
                push_error(
                    errors,
                    "/gpus",
                    "successful or partial GPU collectors require a GPU array",
                );
                return;
            }

            for field in &collector.fields {
                match collection_value.pointer(&field.path) {
                    Some(Value::Null) => {}
                    Some(_) => push_error(
                        errors,
                        &field.path,
                        "field collection status must refer to a null value",
                    ),
                    None => push_error(
                        errors,
                        &field.path,
                        "field collection status refers to an unknown path",
                    ),
                }
            }

            for path in gpu_null_paths(collection) {
                if !collector.fields.iter().any(|field| field.path == path) {
                    push_error(
                        errors,
                        path,
                        "null GPU value must have a field collection status",
                    );
                }
            }

            if collector.status == CollectorStatus::Success
                && collector
                    .fields
                    .iter()
                    .any(|field| field.status != crate::FieldCollectionStatus::NotApplicable)
            {
                push_error(
                    errors,
                    "/collectors/gpu/status",
                    "success may only contain not_applicable field statuses",
                );
            }
        }
        CollectorStatus::Skipped | CollectorStatus::Failed => {
            if collection.gpus.is_some() {
                push_error(
                    errors,
                    "/gpus",
                    "skipped or failed GPU collectors require a null GPU collection",
                );
            }
            if collector.messages.is_empty() {
                push_error(
                    errors,
                    "/collectors/gpu/messages",
                    "skipped or failed GPU collectors must include a reason",
                );
            }
        }
    }
}

fn gpu_null_paths(collection: &Collection) -> Vec<String> {
    let Some(gpus) = &collection.gpus else {
        return vec![];
    };
    let value = serde_json::to_value(gpus).expect("GPU collection must serialize");
    let mut paths = Vec::new();
    collect_null_paths(&value, "/gpus", &mut paths);
    paths
}

fn device_null_paths(devices: &[crate::ConnectedDevice]) -> Vec<String> {
    let value = serde_json::to_value(devices).expect("device collection must serialize");
    let mut paths = Vec::new();
    collect_null_paths(&value, "/devices", &mut paths);
    paths
}

fn collect_null_paths(value: &Value, path: &str, paths: &mut Vec<String>) {
    match value {
        Value::Null => paths.push(path.into()),
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                collect_null_paths(value, &format!("{path}/{index}"), paths);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                collect_null_paths(value, &format!("{path}/{key}"), paths);
            }
        }
        _ => {}
    }
}

impl Diagnosis {
    pub fn validate_against(&self, collection: &Collection) -> Result<(), ValidationErrors> {
        let mut errors = Vec::new();

        for (evaluation_index, evaluation) in self.evaluations.iter().enumerate() {
            let base = format!("/evaluations/{evaluation_index}");

            if evaluation.status == RuleEvaluationStatus::Triggered && evaluation.severity.is_none()
            {
                push_error(
                    &mut errors,
                    format!("{base}/severity"),
                    "triggered evaluations must have a severity",
                );
            }

            if evaluation.status == RuleEvaluationStatus::NotEvaluated
                && evaluation.reason.is_none()
            {
                push_error(
                    &mut errors,
                    format!("{base}/reason"),
                    "not_evaluated evaluations must have a reason",
                );
            }

            for (evidence_index, evidence) in evaluation.evidence.iter().enumerate() {
                let evidence_path = format!("{base}/evidence/{evidence_index}");
                match evidence {
                    Evidence::Collected { path, value } => {
                        validate_collected_evidence(
                            collection,
                            path,
                            value,
                            &evidence_path,
                            &mut errors,
                        );
                    }
                    Evidence::Derived { source_paths, .. } => {
                        for source_path in source_paths {
                            match memory_value(collection, source_path) {
                                Some(Value::Null) | None => push_error(
                                    &mut errors,
                                    &evidence_path,
                                    format!(
                                        "derived evidence source does not resolve to a value: {source_path}"
                                    ),
                                ),
                                Some(_) => {}
                            }
                        }
                    }
                }
            }
        }

        finish(errors)
    }
}

fn validate_memory_status(
    collection: &Collection,
    collector: &CollectorResult,
    errors: &mut Vec<ValidationError>,
) {
    let null_paths: Vec<_> = MEMORY_PATHS
        .iter()
        .copied()
        .filter(|path| memory_value(collection, path) == Some(Value::Null))
        .collect();

    match collector.status {
        CollectorStatus::Success => {
            if !null_paths.is_empty() {
                push_error(
                    errors,
                    "/collectors/memory/status",
                    "success cannot contain null memory values",
                );
            }
            if !collector.fields.is_empty() {
                push_error(
                    errors,
                    "/collectors/memory/fields",
                    "success cannot contain field failures",
                );
            }
        }
        CollectorStatus::Partial => {
            for path in &null_paths {
                if !collector.fields.iter().any(|field| field.path == *path) {
                    push_error(
                        errors,
                        *path,
                        "null value must have a field collection result",
                    );
                }
            }

            for field in &collector.fields {
                match memory_value(collection, &field.path) {
                    Some(Value::Null) => {}
                    Some(_) => push_error(
                        errors,
                        &field.path,
                        "field collection failure must refer to a null value",
                    ),
                    None => push_error(
                        errors,
                        &field.path,
                        "field collection failure refers to an unknown path",
                    ),
                }
            }
        }
        CollectorStatus::Skipped | CollectorStatus::Failed => {
            if collector.messages.is_empty() {
                push_error(
                    errors,
                    "/collectors/memory/messages",
                    "skipped or failed collectors must include a reason",
                );
            }
            if null_paths.len() != MEMORY_PATHS.len() {
                push_error(
                    errors,
                    "/memory",
                    "skipped or failed memory collectors require all memory values to be null",
                );
            }
        }
    }
}

fn validate_collected_evidence(
    collection: &Collection,
    path: &str,
    expected: &Value,
    evidence_path: &str,
    errors: &mut Vec<ValidationError>,
) {
    match memory_value(collection, path) {
        Some(Value::Null) => push_error(
            errors,
            evidence_path,
            format!("collected evidence refers to a null value: {path}"),
        ),
        Some(actual) if &actual != expected => push_error(
            errors,
            evidence_path,
            format!("collected evidence value does not match {path}"),
        ),
        Some(_) => {}
        None => push_error(
            errors,
            evidence_path,
            format!("collected evidence refers to an unknown path: {path}"),
        ),
    }
}

fn memory_value(collection: &Collection, path: &str) -> Option<Value> {
    let memory = &collection.memory;
    match path {
        "/memory/physical/total_bytes" => option_value(memory.physical.total_bytes),
        "/memory/physical/available_bytes" => option_value(memory.physical.available_bytes),
        "/memory/physical/load_percent" => option_value(memory.physical.load_percent),
        "/memory/commit/limit_bytes" => option_value(memory.commit.limit_bytes),
        "/memory/commit/available_bytes" => option_value(memory.commit.available_bytes),
        "/memory/virtual/total_bytes" => option_value(memory.virtual_memory.total_bytes),
        "/memory/virtual/available_bytes" => option_value(memory.virtual_memory.available_bytes),
        _ => None,
    }
}

fn option_value<T: serde::Serialize>(value: Option<T>) -> Option<Value> {
    Some(serde_json::to_value(value).expect("supported memory value must serialize"))
}

fn validate_available_not_greater_than_total(
    errors: &mut Vec<ValidationError>,
    base_path: &str,
    available: Option<u64>,
    total: Option<u64>,
) {
    if let (Some(available), Some(total)) = (available, total)
        && available > total
    {
        push_error(
            errors,
            format!("{base_path}/available_bytes"),
            "must not be greater than the corresponding total or limit",
        );
    }
}

fn push_error(
    errors: &mut Vec<ValidationError>,
    path: impl Into<String>,
    message: impl Into<String>,
) {
    errors.push(ValidationError {
        path: path.into(),
        message: message.into(),
    });
}

fn finish(errors: Vec<ValidationError>) -> Result<(), ValidationErrors> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ValidationErrors(errors))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        CollectionMessage, CollectorStatus, CommitMemory, Criterion, DiagnosisSummary,
        EvaluationCounts, FieldCollectionResult, FieldCollectionStatus, FindingCounts,
        MeasurementUnit, MemoryCollection, PhysicalMemory, Recommendation, RuleEvaluation,
        RuleSetInfo, Severity, VirtualMemory,
    };

    #[test]
    fn collection_round_trips_with_virtual_json_name() {
        let collection = complete_collection();
        let json = serde_json::to_value(&collection).unwrap();

        assert!(json["memory"].get("virtual").is_some());
        assert!(json["memory"].get("virtual_memory").is_none());
        assert_eq!(
            serde_json::from_value::<Collection>(json).unwrap(),
            collection
        );
    }

    #[test]
    fn partial_collection_requires_reasons_for_null_fields() {
        let mut collection = complete_collection();
        collection.memory.commit.limit_bytes = None;

        let status = CollectionStatus {
            collectors: vec![CollectorResult {
                name: CollectorName::Memory,
                status: CollectorStatus::Partial,
                duration_ms: 12,
                messages: vec![],
                fields: vec![],
            }],
        };

        let errors = collection.validate_with_status(&status).unwrap_err();
        assert!(errors.errors().iter().any(|error| {
            error.path == "/memory/commit/limit_bytes"
                && error
                    .message
                    .contains("must have a field collection result")
        }));
    }

    #[test]
    fn partial_collection_accepts_an_explained_null() {
        let mut collection = complete_collection();
        collection.memory.commit.limit_bytes = None;

        let status = CollectionStatus {
            collectors: vec![CollectorResult {
                name: CollectorName::Memory,
                status: CollectorStatus::Partial,
                duration_ms: 12,
                messages: vec![],
                fields: vec![FieldCollectionResult {
                    path: "/memory/commit/limit_bytes".into(),
                    status: FieldCollectionStatus::SourceNull,
                    code: "source_returned_null".into(),
                    native_code: None,
                }],
            }],
        };

        collection.validate_with_status(&status).unwrap();
    }

    #[test]
    fn failed_collector_explains_all_null_memory_values() {
        let collection = null_collection();
        let status = CollectionStatus {
            collectors: vec![CollectorResult {
                name: CollectorName::Memory,
                status: CollectorStatus::Failed,
                duration_ms: 3,
                messages: vec![CollectionMessage {
                    code: "windows_api_failed".into(),
                    native_code: Some(5),
                    message: Some("メモリ情報を取得できませんでした".into()),
                }],
                fields: vec![],
            }],
        };

        collection.validate_with_status(&status).unwrap();
    }

    #[test]
    fn diagnosis_evidence_must_match_the_collection() {
        let collection = complete_collection();
        let mut diagnosis = memory_diagnosis();
        diagnosis.evaluations[0].evidence[0] = Evidence::Collected {
            path: "/memory/physical/total_bytes".into(),
            value: json!(1),
        };

        let errors = diagnosis.validate_against(&collection).unwrap_err();
        assert!(errors.errors().iter().any(|error| {
            error
                .message
                .contains("does not match /memory/physical/total_bytes")
        }));
    }

    #[test]
    fn valid_memory_diagnosis_matches_the_collection() {
        memory_diagnosis()
            .validate_against(&complete_collection())
            .unwrap();
    }

    fn complete_collection() -> Collection {
        Collection {
            memory: MemoryCollection {
                physical: PhysicalMemory {
                    total_bytes: Some(17_179_869_184),
                    available_bytes: Some(536_870_912),
                    load_percent: Some(97.0),
                },
                commit: CommitMemory {
                    limit_bytes: Some(25_769_803_776),
                    available_bytes: Some(9_126_805_504),
                },
                virtual_memory: VirtualMemory {
                    total_bytes: Some(140_737_488_224_256),
                    available_bytes: Some(140_732_881_338_368),
                },
            },
            gpus: Some(vec![]),
            devices: Some(vec![]),
        }
    }

    fn null_collection() -> Collection {
        Collection {
            memory: MemoryCollection {
                physical: PhysicalMemory {
                    total_bytes: None,
                    available_bytes: None,
                    load_percent: None,
                },
                commit: CommitMemory {
                    limit_bytes: None,
                    available_bytes: None,
                },
                virtual_memory: VirtualMemory {
                    total_bytes: None,
                    available_bytes: None,
                },
            },
            gpus: Some(vec![]),
            devices: Some(vec![]),
        }
    }

    fn memory_diagnosis() -> Diagnosis {
        Diagnosis {
            rule_set: RuleSetInfo {
                name: "pcdiag_builtin".into(),
                version: "0.1.0".into(),
            },
            summary: DiagnosisSummary {
                overall_severity: Some(Severity::Warning),
                evaluations: EvaluationCounts {
                    passed: 0,
                    triggered: 1,
                    not_applicable: 0,
                    not_evaluated: 0,
                    failed: 0,
                },
                findings: FindingCounts {
                    critical: 0,
                    error: 0,
                    warning: 1,
                    information: 0,
                },
            },
            evaluations: vec![RuleEvaluation {
                rule_id: "memory.available_ratio".into(),
                rule_version: "1.0".into(),
                category: "memory".into(),
                status: RuleEvaluationStatus::Triggered,
                severity: Some(Severity::Warning),
                summary: "使用可能な物理メモリが少なくなっています".into(),
                evidence: vec![
                    Evidence::Collected {
                        path: "/memory/physical/total_bytes".into(),
                        value: json!(17_179_869_184_u64),
                    },
                    Evidence::Collected {
                        path: "/memory/physical/available_bytes".into(),
                        value: json!(536_870_912_u64),
                    },
                    Evidence::Derived {
                        name: "available_percent".into(),
                        value: json!(3.125),
                        unit: Some(MeasurementUnit::Percent),
                        source_paths: vec![
                            "/memory/physical/total_bytes".into(),
                            "/memory/physical/available_bytes".into(),
                        ],
                    },
                ],
                criterion: Some(Criterion {
                    operator: "less_than".into(),
                    threshold: json!(10.0),
                    unit: Some(MeasurementUnit::Percent),
                }),
                reason: None,
                recommendation: Some(Recommendation {
                    code: "review_memory_consumption".into(),
                }),
            }],
        }
    }
}
