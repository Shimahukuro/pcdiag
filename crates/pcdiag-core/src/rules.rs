use serde_json::json;

use crate::{
    Collection, Criterion, Diagnosis, DiagnosisSummary, EvaluationCounts, EvaluationReason,
    Evidence, FindingCounts, MeasurementUnit, Recommendation, RuleEvaluation, RuleEvaluationStatus,
    RuleSetInfo, Severity,
};

const MEMORY_AVAILABLE_THRESHOLD_PERCENT: f64 = 10.0;

pub fn diagnose_collection(collection: &Collection) -> Diagnosis {
    let evaluation = evaluate_memory_available_ratio(collection);
    let summary = summarize(std::slice::from_ref(&evaluation));
    Diagnosis {
        rule_set: RuleSetInfo {
            name: "pcdiag_builtin".into(),
            version: "0.1.0".into(),
        },
        summary,
        evaluations: vec![evaluation],
    }
}

fn evaluate_memory_available_ratio(collection: &Collection) -> RuleEvaluation {
    let total_path = "/memory/physical/total_bytes";
    let available_path = "/memory/physical/available_bytes";
    let criterion = Some(Criterion {
        operator: "less_than".into(),
        threshold: json!(MEMORY_AVAILABLE_THRESHOLD_PERCENT),
        unit: Some(MeasurementUnit::Percent),
    });
    let (Some(total), Some(available)) = (
        collection.memory.physical.total_bytes,
        collection.memory.physical.available_bytes,
    ) else {
        let mut paths = Vec::new();
        if collection.memory.physical.total_bytes.is_none() {
            paths.push(total_path.into());
        }
        if collection.memory.physical.available_bytes.is_none() {
            paths.push(available_path.into());
        }
        return RuleEvaluation {
            rule_id: "memory.available_ratio".into(),
            rule_version: "1.0".into(),
            category: "memory".into(),
            status: RuleEvaluationStatus::NotEvaluated,
            severity: None,
            summary: "物理メモリの利用可能割合を評価できませんでした".into(),
            evidence: vec![],
            criterion,
            reason: Some(EvaluationReason {
                code: "required_collection_value_unavailable".into(),
                paths,
            }),
            recommendation: None,
        };
    };
    if total == 0 {
        return RuleEvaluation {
            rule_id: "memory.available_ratio".into(),
            rule_version: "1.0".into(),
            category: "memory".into(),
            status: RuleEvaluationStatus::NotEvaluated,
            severity: None,
            summary: "物理メモリの利用可能割合を評価できませんでした".into(),
            evidence: vec![Evidence::Collected {
                path: total_path.into(),
                value: json!(total),
            }],
            criterion,
            reason: Some(EvaluationReason {
                code: "required_collection_value_invalid".into(),
                paths: vec![total_path.into()],
            }),
            recommendation: None,
        };
    }

    let available_percent = available as f64 / total as f64 * 100.0;
    let triggered = available_percent < MEMORY_AVAILABLE_THRESHOLD_PERCENT;
    RuleEvaluation {
        rule_id: "memory.available_ratio".into(),
        rule_version: "1.0".into(),
        category: "memory".into(),
        status: if triggered {
            RuleEvaluationStatus::Triggered
        } else {
            RuleEvaluationStatus::Passed
        },
        severity: triggered.then_some(Severity::Warning),
        summary: if triggered {
            "使用可能な物理メモリが少なくなっています"
        } else {
            "物理メモリの利用可能割合に問題はありません"
        }
        .into(),
        evidence: vec![
            Evidence::Collected {
                path: total_path.into(),
                value: json!(total),
            },
            Evidence::Collected {
                path: available_path.into(),
                value: json!(available),
            },
            Evidence::Derived {
                name: "available_percent".into(),
                value: json!(available_percent),
                unit: Some(MeasurementUnit::Percent),
                source_paths: vec![total_path.into(), available_path.into()],
            },
        ],
        criterion,
        reason: None,
        recommendation: triggered.then(|| Recommendation {
            code: "review_memory_consumption".into(),
        }),
    }
}

fn summarize(evaluations: &[RuleEvaluation]) -> DiagnosisSummary {
    let mut counts = EvaluationCounts {
        passed: 0,
        triggered: 0,
        not_applicable: 0,
        not_evaluated: 0,
        failed: 0,
    };
    let mut findings = FindingCounts {
        critical: 0,
        error: 0,
        warning: 0,
        information: 0,
    };
    let mut overall = None;
    for evaluation in evaluations {
        match evaluation.status {
            RuleEvaluationStatus::Passed => counts.passed += 1,
            RuleEvaluationStatus::Triggered => counts.triggered += 1,
            RuleEvaluationStatus::NotApplicable => counts.not_applicable += 1,
            RuleEvaluationStatus::NotEvaluated => counts.not_evaluated += 1,
            RuleEvaluationStatus::Failed => counts.failed += 1,
        }
        if evaluation.status == RuleEvaluationStatus::Triggered
            && let Some(severity) = evaluation.severity
        {
            match severity {
                Severity::Critical => findings.critical += 1,
                Severity::Error => findings.error += 1,
                Severity::Warning => findings.warning += 1,
                Severity::Information => findings.information += 1,
            }
            if match overall {
                Some(current) => severity_rank(severity) > severity_rank(current),
                None => true,
            } {
                overall = Some(severity);
            }
        }
    }
    DiagnosisSummary {
        overall_severity: overall,
        evaluations: counts,
        findings,
    }
}

fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Information => 1,
        Severity::Warning => 2,
        Severity::Error => 3,
        Severity::Critical => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collection() -> Collection {
        serde_json::from_str(include_str!(
            "../tests/fixtures/memory-success-collection.json"
        ))
        .unwrap()
    }

    #[test]
    fn triggers_warning_below_ten_percent() {
        let diagnosis = diagnose_collection(&collection());

        assert_eq!(
            diagnosis.evaluations[0].status,
            RuleEvaluationStatus::Triggered
        );
        assert_eq!(diagnosis.summary.overall_severity, Some(Severity::Warning));
        diagnosis.validate_against(&collection()).unwrap();
    }

    #[test]
    fn passes_at_or_above_ten_percent() {
        let mut collection = collection();
        collection.memory.physical.available_bytes = collection
            .memory
            .physical
            .total_bytes
            .map(|total| total / 2);
        let diagnosis = diagnose_collection(&collection);

        assert_eq!(
            diagnosis.evaluations[0].status,
            RuleEvaluationStatus::Passed
        );
        assert_eq!(diagnosis.summary.overall_severity, None);
        diagnosis.validate_against(&collection).unwrap();
    }

    #[test]
    fn records_unavailable_paths_when_not_evaluated() {
        let mut collection = collection();
        collection.memory.physical.available_bytes = None;
        let diagnosis = diagnose_collection(&collection);

        assert_eq!(
            diagnosis.evaluations[0].status,
            RuleEvaluationStatus::NotEvaluated
        );
        assert_eq!(
            diagnosis.evaluations[0].reason.as_ref().unwrap().paths,
            vec!["/memory/physical/available_bytes"]
        );
    }
}
