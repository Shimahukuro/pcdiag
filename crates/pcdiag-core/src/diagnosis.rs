use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnosis {
    pub rule_set: RuleSetInfo,
    pub summary: DiagnosisSummary,
    pub evaluations: Vec<RuleEvaluation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSetInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosisSummary {
    pub overall_severity: Option<Severity>,
    pub evaluations: EvaluationCounts,
    pub findings: FindingCounts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationCounts {
    pub passed: u64,
    pub triggered: u64,
    pub not_applicable: u64,
    pub not_evaluated: u64,
    pub failed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingCounts {
    pub critical: u64,
    pub error: u64,
    pub warning: u64,
    pub information: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleEvaluationStatus {
    Passed,
    Triggered,
    NotApplicable,
    NotEvaluated,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    Error,
    Warning,
    Information,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleEvaluation {
    pub rule_id: String,
    pub rule_version: String,
    pub category: String,
    pub status: RuleEvaluationStatus,
    pub severity: Option<Severity>,
    pub summary: String,
    pub evidence: Vec<Evidence>,
    pub criterion: Option<Criterion>,
    pub reason: Option<EvaluationReason>,
    pub recommendation: Option<Recommendation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Evidence {
    Collected {
        path: String,
        value: Value,
    },
    Derived {
        name: String,
        value: Value,
        unit: Option<MeasurementUnit>,
        source_paths: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementUnit {
    Bytes,
    Percent,
    Milliseconds,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Criterion {
    pub operator: String,
    pub threshold: Value,
    pub unit: Option<MeasurementUnit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationReason {
    pub code: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recommendation {
    pub code: String,
}
