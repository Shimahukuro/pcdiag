use serde_json::{Value, json};

use crate::{
    Collection, Criterion, Diagnosis, DiagnosisSummary, EvaluationCounts, EvaluationReason,
    Evidence, FindingCounts, Gpu, GpuAdapterType, MeasurementUnit, Recommendation, RuleEvaluation,
    RuleEvaluationStatus, RuleSetInfo, Severity,
};

const MEMORY_AVAILABLE_THRESHOLD_PERCENT: f64 = 10.0;

pub fn diagnose_collection(collection: &Collection) -> Diagnosis {
    let mut evaluations = vec![evaluate_memory_available_ratio(collection)];
    evaluations.extend(evaluate_gpus(collection));
    let summary = summarize(&evaluations);
    Diagnosis {
        rule_set: RuleSetInfo {
            name: "pcdiag_builtin".into(),
            version: "0.2.0".into(),
        },
        summary,
        evaluations,
    }
}

fn evaluate_gpus(collection: &Collection) -> Vec<RuleEvaluation> {
    let Some(gpus) = collection.gpus.as_deref() else {
        return vec![
            unavailable_gpu_evaluation(
                "gpu.device_problem",
                "GPUの問題コードを評価できませんでした",
            ),
            unavailable_gpu_evaluation(
                "gpu.adapter_enabled",
                "GPUの有効状態を評価できませんでした",
            ),
            unavailable_gpu_evaluation(
                "gpu.driver_version_available",
                "GPUドライバー情報を評価できませんでした",
            ),
        ];
    };
    let hardware: Vec<(usize, &Gpu)> = gpus
        .iter()
        .enumerate()
        .filter(|(_, gpu)| {
            gpu.adapter_type == GpuAdapterType::Hardware && gpu.device_state.present == Some(true)
        })
        .collect();
    if hardware.is_empty() {
        return vec![
            inapplicable_gpu_evaluation("gpu.device_problem", "評価対象となる物理GPUがありません"),
            inapplicable_gpu_evaluation("gpu.adapter_enabled", "評価対象となる物理GPUがありません"),
            inapplicable_gpu_evaluation(
                "gpu.driver_version_available",
                "評価対象となる物理GPUがありません",
            ),
        ];
    }
    vec![
        evaluate_gpu_problem_codes(&hardware),
        evaluate_gpu_enabled(&hardware),
        evaluate_gpu_driver_versions(&hardware),
    ]
}

fn evaluate_gpu_problem_codes(gpus: &[(usize, &Gpu)]) -> RuleEvaluation {
    let triggered: Vec<_> = gpus
        .iter()
        .filter(|(_, gpu)| gpu.device_state.problem_code.is_some_and(|code| code != 0))
        .collect();
    let missing: Vec<_> = gpus
        .iter()
        .filter(|(_, gpu)| gpu.device_state.problem_code.is_none())
        .collect();
    if triggered.is_empty() && !missing.is_empty() {
        return missing_gpu_value_evaluation(
            "gpu.device_problem",
            "GPUの問題コードを評価できませんでした",
            &missing,
            "device_state/problem_code",
        );
    }
    let is_triggered = !triggered.is_empty();
    RuleEvaluation {
        rule_id: "gpu.device_problem".into(),
        rule_version: "1.0".into(),
        category: "gpu".into(),
        status: status(is_triggered),
        severity: is_triggered.then_some(Severity::Error),
        summary: if is_triggered {
            "WindowsがGPUのデバイス問題を報告しています"
        } else {
            "GPUのデバイス問題コードに異常はありません"
        }
        .into(),
        evidence: triggered
            .into_iter()
            .map(|(index, gpu)| Evidence::Collected {
                path: format!("/gpus/{index}/device_state/problem_code"),
                value: json!(gpu.device_state.problem_code),
            })
            .collect(),
        criterion: Some(Criterion {
            operator: "not_equal".into(),
            threshold: json!(0),
            unit: None,
        }),
        reason: None,
        recommendation: is_triggered.then(|| Recommendation {
            code: "review_gpu_device_problem".into(),
        }),
    }
}

fn evaluate_gpu_enabled(gpus: &[(usize, &Gpu)]) -> RuleEvaluation {
    let triggered: Vec<_> = gpus
        .iter()
        .filter(|(_, gpu)| gpu.device_state.enabled == Some(false))
        .collect();
    let missing: Vec<_> = gpus
        .iter()
        .filter(|(_, gpu)| gpu.device_state.enabled.is_none())
        .collect();
    if triggered.is_empty() && !missing.is_empty() {
        return missing_gpu_value_evaluation(
            "gpu.adapter_enabled",
            "GPUの有効状態を評価できませんでした",
            &missing,
            "device_state/enabled",
        );
    }
    let is_triggered = !triggered.is_empty();
    RuleEvaluation {
        rule_id: "gpu.adapter_enabled".into(),
        rule_version: "1.0".into(),
        category: "gpu".into(),
        status: status(is_triggered),
        severity: is_triggered.then_some(Severity::Warning),
        summary: if is_triggered {
            "無効化されている物理GPUがあります"
        } else {
            "物理GPUは有効です"
        }
        .into(),
        evidence: triggered
            .into_iter()
            .map(|(index, gpu)| Evidence::Collected {
                path: format!("/gpus/{index}/device_state/enabled"),
                value: json!(gpu.device_state.enabled),
            })
            .collect(),
        criterion: Some(Criterion {
            operator: "equal".into(),
            threshold: json!(false),
            unit: None,
        }),
        reason: None,
        recommendation: is_triggered.then(|| Recommendation {
            code: "enable_gpu_adapter".into(),
        }),
    }
}

fn evaluate_gpu_driver_versions(gpus: &[(usize, &Gpu)]) -> RuleEvaluation {
    let triggered: Vec<_> = gpus
        .iter()
        .filter(|(_, gpu)| gpu.driver.version.as_deref().is_none_or(str::is_empty))
        .collect();
    let is_triggered = !triggered.is_empty();
    RuleEvaluation {
        rule_id: "gpu.driver_version_available".into(),
        rule_version: "1.0".into(),
        category: "gpu".into(),
        status: status(is_triggered),
        severity: is_triggered.then_some(Severity::Warning),
        summary: if is_triggered {
            "ドライバーバージョンを確認できない物理GPUがあります"
        } else {
            "物理GPUのドライバーバージョンを確認できました"
        }
        .into(),
        evidence: vec![],
        criterion: Some(Criterion {
            operator: "unavailable_or_empty".into(),
            threshold: Value::Null,
            unit: None,
        }),
        reason: is_triggered.then(|| EvaluationReason {
            code: "gpu_driver_version_unavailable".into(),
            paths: triggered
                .into_iter()
                .map(|(index, _)| format!("/gpus/{index}/driver/version"))
                .collect(),
        }),
        recommendation: is_triggered.then(|| Recommendation {
            code: "review_gpu_driver_installation".into(),
        }),
    }
}

fn status(triggered: bool) -> RuleEvaluationStatus {
    if triggered {
        RuleEvaluationStatus::Triggered
    } else {
        RuleEvaluationStatus::Passed
    }
}

fn unavailable_gpu_evaluation(rule_id: &str, summary: &str) -> RuleEvaluation {
    RuleEvaluation {
        rule_id: rule_id.into(),
        rule_version: "1.0".into(),
        category: "gpu".into(),
        status: RuleEvaluationStatus::NotEvaluated,
        severity: None,
        summary: summary.into(),
        evidence: vec![],
        criterion: None,
        reason: Some(EvaluationReason {
            code: "gpu_collection_unavailable".into(),
            paths: vec!["/gpus".into()],
        }),
        recommendation: None,
    }
}

fn inapplicable_gpu_evaluation(rule_id: &str, summary: &str) -> RuleEvaluation {
    RuleEvaluation {
        rule_id: rule_id.into(),
        rule_version: "1.0".into(),
        category: "gpu".into(),
        status: RuleEvaluationStatus::NotApplicable,
        severity: None,
        summary: summary.into(),
        evidence: vec![],
        criterion: None,
        reason: Some(EvaluationReason {
            code: "no_present_hardware_gpu".into(),
            paths: vec!["/gpus".into()],
        }),
        recommendation: None,
    }
}

fn missing_gpu_value_evaluation(
    rule_id: &str,
    summary: &str,
    gpus: &[&(usize, &Gpu)],
    suffix: &str,
) -> RuleEvaluation {
    RuleEvaluation {
        rule_id: rule_id.into(),
        rule_version: "1.0".into(),
        category: "gpu".into(),
        status: RuleEvaluationStatus::NotEvaluated,
        severity: None,
        summary: summary.into(),
        evidence: vec![],
        criterion: None,
        reason: Some(EvaluationReason {
            code: "required_collection_value_unavailable".into(),
            paths: gpus
                .iter()
                .map(|(index, _)| format!("/gpus/{index}/{suffix}"))
                .collect(),
        }),
        recommendation: None,
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

    fn gpu_collection() -> Collection {
        serde_json::from_str(include_str!(
            "../tests/fixtures/gpu-success-collection.json"
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

    #[test]
    fn passes_all_gpu_rules_for_healthy_hardware_gpu() {
        let collection = gpu_collection();
        let diagnosis = diagnose_collection(&collection);

        assert_eq!(diagnosis.rule_set.version, "0.2.0");
        assert!(diagnosis.evaluations[1..].iter().all(|evaluation| {
            evaluation.category == "gpu" && evaluation.status == RuleEvaluationStatus::Passed
        }));
        diagnosis.validate_against(&collection).unwrap();
    }

    #[test]
    fn reports_gpu_problem_disabled_adapter_and_missing_driver() {
        let mut collection = gpu_collection();
        let gpu = &mut collection.gpus.as_mut().unwrap()[0];
        gpu.device_state.problem_code = Some(22);
        gpu.device_state.enabled = Some(false);
        gpu.driver.version = None;
        let diagnosis = diagnose_collection(&collection);

        assert_eq!(diagnosis.summary.evaluations.triggered, 4);
        assert_eq!(diagnosis.summary.findings.error, 1);
        assert_eq!(diagnosis.summary.findings.warning, 3);
        assert_eq!(diagnosis.summary.overall_severity, Some(Severity::Error));
        diagnosis.validate_against(&collection).unwrap();
    }
}
