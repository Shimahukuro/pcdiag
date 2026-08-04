use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::{
    Collection, ConnectedDevice, Criterion, Diagnosis, DiagnosisSummary, EvaluationCounts,
    EvaluationReason, EventLogEntry, Evidence, FindingCounts, Gpu, GpuAdapterType, MeasurementUnit,
    Recommendation, RuleEvaluation, RuleEvaluationStatus, RuleSetInfo, Severity, SmartProtocol,
};

const MEMORY_AVAILABLE_THRESHOLD_PERCENT: f64 = 10.0;
const VOLUME_FREE_THRESHOLD_PERCENT: f64 = 10.0;
const VOLUME_FREE_THRESHOLD_BYTES: u64 = 10 * 1024 * 1024 * 1024;

pub fn diagnose_collection(collection: &Collection) -> Diagnosis {
    let mut evaluations = vec![evaluate_memory_available_ratio(collection)];
    evaluations.extend(evaluate_gpus(collection));
    evaluations.extend(evaluate_devices(collection));
    evaluations.extend(evaluate_event_logs(collection));
    evaluations.extend(evaluate_storage(collection));
    let summary = summarize(&evaluations);
    Diagnosis {
        rule_set: RuleSetInfo {
            name: "pcdiag_builtin".into(),
            version: "0.8.0".into(),
        },
        summary,
        evaluations,
    }
}

fn evaluate_event_logs(collection: &Collection) -> Vec<RuleEvaluation> {
    let mut evaluations = Vec::new();
    let mut groups: BTreeMap<(String, String), Vec<(usize, &EventLogEntry)>> = BTreeMap::new();
    for (field, events) in [
        ("system", collection.event_logs.system.as_deref()),
        ("application", collection.event_logs.application.as_deref()),
        ("security", collection.event_logs.security.as_deref()),
    ] {
        let Some(events) = events else {
            evaluations.push(RuleEvaluation {
                rule_id: format!("event_log.{field}.availability"),
                rule_version: "1.0.0".into(),
                category: "event_log".into(),
                status: RuleEvaluationStatus::Triggered,
                severity: Some(Severity::Warning),
                summary: format!(
                    "{field}イベントログを取得できませんでした。収集状態で権限、サービスまたはログ破損の理由を確認してください"
                ),
                evidence: Vec::new(),
                criterion: None,
                reason: None,
                recommendation: Some(Recommendation {
                    code: "restore_event_log_collection".into(),
                }),
            });
            continue;
        };
        for (index, event) in events.iter().enumerate() {
            if let Some(kind) = diagnostic_event_kind(event) {
                groups
                    .entry((field.into(), kind.into()))
                    .or_default()
                    .push((index, event));
            }
        }
    }
    evaluations.extend(groups.into_iter().map(|((field, kind), events)| {
        aggregated_event_log_evaluation(&field, &kind, &events, collection.event_logs.lookback_days)
    }));
    evaluations
}

fn diagnostic_event_kind(event: &EventLogEntry) -> Option<&'static str> {
    match (event.log_name.as_str(), event.event_id) {
        ("System", 41 | 6008) => Some("unexpected_shutdown"),
        ("System", 7 | 11 | 51 | 55 | 129 | 153) => Some("storage_io_failure"),
        ("System", 7000 | 7001 | 7009 | 7011 | 7023 | 7024 | 7031 | 7034) => {
            Some("service_failure")
        }
        ("Application", 1000..=1002) => Some("application_failure"),
        ("Security", 1102) => Some("audit_log_cleared"),
        ("Security", 4625) => Some("failed_logon"),
        ("Security", 4719) => Some("audit_policy_changed"),
        _ => None,
    }
}

fn aggregated_event_log_evaluation(
    field: &str,
    kind: &str,
    events: &[(usize, &EventLogEntry)],
    lookback_days: u32,
) -> RuleEvaluation {
    let &(index, latest) = events
        .iter()
        .max_by_key(|(_, event)| event.occurred_at.as_str())
        .expect("event groups are never empty");
    let (severity, recommendation, label) = match kind {
        "unexpected_shutdown" => (
            Severity::Critical,
            "investigate_unexpected_shutdown",
            "予期しないシャットダウン",
        ),
        "storage_io_failure" => (
            Severity::Error,
            "review_storage_io_failure",
            "ストレージI/O障害",
        ),
        "service_failure" => (
            Severity::Error,
            "investigate_service_failure",
            "Windowsサービス障害",
        ),
        "application_failure" => (
            Severity::Error,
            "investigate_application_failure",
            "アプリケーション異常終了",
        ),
        "audit_log_cleared" => (
            Severity::Critical,
            "investigate_audit_log_clearance",
            "監査ログの消去",
        ),
        "failed_logon" => (Severity::Warning, "review_failed_logons", "ログオン失敗"),
        "audit_policy_changed" => (
            Severity::Error,
            "review_audit_policy_change",
            "監査ポリシー変更",
        ),
        _ => unreachable!("only classified event kinds are grouped"),
    };
    let occurrence = if kind == "unexpected_shutdown" {
        format!("推定{}回", correlated_unexpected_shutdown_count(events, 60))
    } else {
        format!("{}件", events.len())
    };
    RuleEvaluation {
        rule_id: format!("event_log.{field}.{kind}"),
        rule_version: "1.0.0".into(),
        category: "event_log".into(),
        status: RuleEvaluationStatus::Triggered,
        severity: Some(severity),
        summary: format!(
            "{label}を過去{lookback_days}日間に{occurrence}検出しました。最新: {}: {} (event {})",
            latest.occurred_at, latest.summary, latest.event_id
        ),
        evidence: vec![Evidence::Collected {
            path: format!("/event_logs/{field}/{index}"),
            value: json!(latest),
        }],
        criterion: None,
        reason: None,
        recommendation: Some(Recommendation {
            code: recommendation.into(),
        }),
    }
}

fn correlated_unexpected_shutdown_count(
    events: &[(usize, &EventLogEntry)],
    correlation_seconds: i64,
) -> usize {
    let mut event_6008: Vec<_> = events
        .iter()
        .filter(|(_, event)| event.event_id == 6008)
        .filter_map(|(_, event)| utc_seconds(&event.occurred_at).map(|time| (time, false)))
        .collect();
    let mut paired = 0;
    for (_, event) in events.iter().filter(|(_, event)| event.event_id == 41) {
        let Some(time) = utc_seconds(&event.occurred_at) else {
            continue;
        };
        if let Some((_, used)) = event_6008
            .iter_mut()
            .filter(|(candidate, used)| !*used && (time - *candidate).abs() <= correlation_seconds)
            .min_by_key(|(candidate, _)| (time - *candidate).abs())
        {
            *used = true;
            paired += 1;
        }
    }
    events.len().saturating_sub(paired)
}

fn utc_seconds(value: &str) -> Option<i64> {
    let (date, time) = value.strip_suffix('Z')?.split_once('T')?;
    let mut date = date.split('-').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    if date.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut time = time.split(':');
    let hour = time.next()?.parse::<i64>().ok()?;
    let minute = time.next()?.parse::<i64>().ok()?;
    let second = time.next()?.split('.').next()?.parse::<i64>().ok()?;
    if time.next().is_some()
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }
    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let adjusted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days_since_epoch = era * 146_097 + day_of_era - 719_468;
    Some(days_since_epoch * 86_400 + hour * 3_600 + minute * 60 + second)
}

fn evaluate_storage(collection: &Collection) -> Vec<RuleEvaluation> {
    vec![
        evaluate_smart_failure_prediction(collection),
        evaluate_nvme_critical_warning(collection),
        evaluate_nvme_percentage_used(collection),
        evaluate_volume_free_space(collection),
    ]
}

fn evaluate_smart_failure_prediction(collection: &Collection) -> RuleEvaluation {
    let Some(smart) = collection.storage.smart.as_deref() else {
        return unavailable_storage_evaluation(
            "storage.smart_failure_prediction",
            "SMART故障予測を評価できませんでした",
            vec!["/storage/smart".into()],
        );
    };
    let candidates: Vec<_> = smart
        .iter()
        .enumerate()
        .filter(|(_, value)| value.protocol == SmartProtocol::FailurePrediction)
        .collect();
    if candidates.is_empty() {
        return if smart
            .iter()
            .any(|value| value.protocol == SmartProtocol::Unknown)
        {
            unavailable_storage_evaluation(
                "storage.smart_failure_prediction",
                "SMART故障予測を取得できませんでした",
                vec!["/storage/smart".into()],
            )
        } else {
            inapplicable_storage_evaluation(
                "storage.smart_failure_prediction",
                "故障予測方式のSMART対象ディスクがありません",
            )
        };
    }
    let triggered = candidates
        .iter()
        .any(|(_, value)| value.predict_failure == Some(true));
    let missing_paths: Vec<_> = candidates
        .iter()
        .filter(|(_, value)| value.predict_failure.is_none())
        .map(|(index, _)| format!("/storage/smart/{index}/predict_failure"))
        .collect();
    let evidence: Vec<_> = candidates
        .iter()
        .filter_map(|(index, value)| {
            value
                .predict_failure
                .map(|predict_failure| Evidence::Collected {
                    path: format!("/storage/smart/{index}/predict_failure"),
                    value: json!(predict_failure),
                })
        })
        .collect();
    storage_evaluation(
        "storage.smart_failure_prediction",
        triggered,
        Severity::Critical,
        if triggered {
            "ディスクが故障を予測しています"
        } else {
            "SMART故障予測に異常はありません"
        },
        evidence,
        missing_paths,
        Criterion {
            operator: "equal".into(),
            threshold: json!(true),
            unit: None,
        },
        "backup_and_replace_disk",
    )
}

fn evaluate_nvme_critical_warning(collection: &Collection) -> RuleEvaluation {
    let Some(smart) = collection.storage.smart.as_deref() else {
        return unavailable_storage_evaluation(
            "storage.nvme_critical_warning",
            "NVMe Critical Warningを評価できませんでした",
            vec!["/storage/smart".into()],
        );
    };
    let candidates: Vec<_> = smart
        .iter()
        .enumerate()
        .filter(|(_, value)| value.protocol == SmartProtocol::Nvme)
        .collect();
    if candidates.is_empty() {
        return nvme_absent_or_unavailable(
            collection,
            "storage.nvme_critical_warning",
            "NVMe Critical Warningを取得できませんでした",
        );
    }
    let triggered = candidates
        .iter()
        .any(|(_, value)| value.critical_warning.is_some_and(|warning| warning != 0));
    let missing_paths: Vec<_> = candidates
        .iter()
        .filter(|(_, value)| value.critical_warning.is_none())
        .map(|(index, _)| format!("/storage/smart/{index}/critical_warning"))
        .collect();
    let evidence: Vec<_> = candidates
        .iter()
        .filter_map(|(index, value)| {
            value.critical_warning.map(|warning| Evidence::Collected {
                path: format!("/storage/smart/{index}/critical_warning"),
                value: json!(warning),
            })
        })
        .collect();
    storage_evaluation(
        "storage.nvme_critical_warning",
        triggered,
        Severity::Error,
        if triggered {
            "NVMeデバイスがCritical Warningを報告しています"
        } else {
            "NVMe Critical Warningに異常はありません"
        },
        evidence,
        missing_paths,
        Criterion {
            operator: "not_equal".into(),
            threshold: json!(0),
            unit: None,
        },
        "review_nvme_health",
    )
}

fn evaluate_nvme_percentage_used(collection: &Collection) -> RuleEvaluation {
    let Some(smart) = collection.storage.smart.as_deref() else {
        return unavailable_storage_evaluation(
            "storage.nvme_percentage_used",
            "NVMe寿命使用率を評価できませんでした",
            vec!["/storage/smart".into()],
        );
    };
    let candidates: Vec<_> = smart
        .iter()
        .enumerate()
        .filter(|(_, value)| value.protocol == SmartProtocol::Nvme)
        .collect();
    if candidates.is_empty() {
        return nvme_absent_or_unavailable(
            collection,
            "storage.nvme_percentage_used",
            "NVMe寿命使用率を取得できませんでした",
        );
    }
    let triggered = candidates
        .iter()
        .any(|(_, value)| value.percentage_used.is_some_and(|used| used >= 100));
    let missing_paths: Vec<_> = candidates
        .iter()
        .filter(|(_, value)| value.percentage_used.is_none())
        .map(|(index, _)| format!("/storage/smart/{index}/percentage_used"))
        .collect();
    let evidence: Vec<_> = candidates
        .iter()
        .filter_map(|(index, value)| {
            value.percentage_used.map(|used| Evidence::Collected {
                path: format!("/storage/smart/{index}/percentage_used"),
                value: json!(used),
            })
        })
        .collect();
    storage_evaluation(
        "storage.nvme_percentage_used",
        triggered,
        Severity::Warning,
        if triggered {
            "NVMeデバイスの推定寿命使用率が100%以上です"
        } else {
            "NVMeデバイスの推定寿命使用率は100%未満です"
        },
        evidence,
        missing_paths,
        Criterion {
            operator: "greater_than_or_equal".into(),
            threshold: json!(100),
            unit: Some(MeasurementUnit::Percent),
        },
        "plan_nvme_replacement",
    )
}

fn evaluate_volume_free_space(collection: &Collection) -> RuleEvaluation {
    let Some(volumes) = collection.storage.volumes.as_deref() else {
        return unavailable_storage_evaluation(
            "storage.volume_free_space",
            "ボリューム空き容量を評価できませんでした",
            vec!["/storage/volumes".into()],
        );
    };
    let mut evidence = Vec::new();
    let mut missing_paths = Vec::new();
    let mut evaluated = 0_u64;
    let mut triggered = false;
    for (index, volume) in volumes.iter().enumerate() {
        let Some(mount_points) = &volume.mount_points else {
            missing_paths.push(format!("/storage/volumes/{index}/mount_points"));
            continue;
        };
        if mount_points.is_empty() {
            continue;
        }
        let (Some(capacity), Some(free)) = (volume.capacity_bytes, volume.free_bytes) else {
            if volume.capacity_bytes.is_none() {
                missing_paths.push(format!("/storage/volumes/{index}/capacity_bytes"));
            }
            if volume.free_bytes.is_none() {
                missing_paths.push(format!("/storage/volumes/{index}/free_bytes"));
            }
            continue;
        };
        if capacity == 0 {
            missing_paths.push(format!("/storage/volumes/{index}/capacity_bytes"));
            continue;
        }
        evaluated += 1;
        let capacity_path = format!("/storage/volumes/{index}/capacity_bytes");
        let free_path = format!("/storage/volumes/{index}/free_bytes");
        let free_percent = free as f64 / capacity as f64 * 100.0;
        triggered |=
            free_percent < VOLUME_FREE_THRESHOLD_PERCENT && free < VOLUME_FREE_THRESHOLD_BYTES;
        evidence.extend([
            Evidence::Collected {
                path: capacity_path.clone(),
                value: json!(capacity),
            },
            Evidence::Collected {
                path: free_path.clone(),
                value: json!(free),
            },
            Evidence::Derived {
                name: format!("volume_{index}_free_percent"),
                value: json!(free_percent),
                unit: Some(MeasurementUnit::Percent),
                source_paths: vec![capacity_path, free_path],
            },
        ]);
    }
    if evaluated == 0 {
        return if missing_paths.is_empty() {
            inapplicable_storage_evaluation(
                "storage.volume_free_space",
                "ドライブ文字付きボリュームがありません",
            )
        } else {
            unavailable_storage_evaluation(
                "storage.volume_free_space",
                "ボリューム空き容量を評価できませんでした",
                missing_paths,
            )
        };
    }
    storage_evaluation(
        "storage.volume_free_space",
        triggered,
        Severity::Warning,
        if triggered {
            "空き容量が少ないボリュームがあります"
        } else {
            "ボリューム空き容量に問題はありません"
        },
        evidence,
        missing_paths,
        Criterion {
            operator: "all".into(),
            threshold: json!({
                "free_percent_less_than": VOLUME_FREE_THRESHOLD_PERCENT,
                "free_bytes_less_than": VOLUME_FREE_THRESHOLD_BYTES
            }),
            unit: None,
        },
        "free_volume_space",
    )
}

fn nvme_absent_or_unavailable(
    collection: &Collection,
    rule_id: &str,
    unavailable_summary: &str,
) -> RuleEvaluation {
    let has_nvme_disk = collection.storage.disks.as_deref().is_some_and(|disks| {
        disks
            .iter()
            .any(|disk| disk.bus_type == Some(crate::DiskBusType::Nvme))
    });
    if has_nvme_disk {
        unavailable_storage_evaluation(rule_id, unavailable_summary, vec!["/storage/smart".into()])
    } else {
        inapplicable_storage_evaluation(rule_id, "NVMeディスクがありません")
    }
}

#[allow(clippy::too_many_arguments)]
fn storage_evaluation(
    rule_id: &str,
    triggered: bool,
    severity: Severity,
    summary: &str,
    evidence: Vec<Evidence>,
    missing_paths: Vec<String>,
    criterion: Criterion,
    recommendation_code: &str,
) -> RuleEvaluation {
    if !triggered && !missing_paths.is_empty() {
        return RuleEvaluation {
            rule_id: rule_id.into(),
            rule_version: "1.0".into(),
            category: "storage".into(),
            status: RuleEvaluationStatus::NotEvaluated,
            severity: None,
            summary: format!("{summary}（一部の必要値を取得できませんでした）"),
            evidence,
            criterion: Some(criterion),
            reason: Some(EvaluationReason {
                code: "required_collection_value_unavailable".into(),
                paths: missing_paths,
            }),
            recommendation: None,
        };
    }
    RuleEvaluation {
        rule_id: rule_id.into(),
        rule_version: "1.0".into(),
        category: "storage".into(),
        status: status(triggered),
        severity: triggered.then_some(severity),
        summary: summary.into(),
        evidence,
        criterion: Some(criterion),
        reason: (!missing_paths.is_empty()).then(|| EvaluationReason {
            code: "required_collection_value_unavailable".into(),
            paths: missing_paths,
        }),
        recommendation: triggered.then(|| Recommendation {
            code: recommendation_code.into(),
        }),
    }
}

fn unavailable_storage_evaluation(
    rule_id: &str,
    summary: &str,
    paths: Vec<String>,
) -> RuleEvaluation {
    RuleEvaluation {
        rule_id: rule_id.into(),
        rule_version: "1.0".into(),
        category: "storage".into(),
        status: RuleEvaluationStatus::NotEvaluated,
        severity: None,
        summary: summary.into(),
        evidence: vec![],
        criterion: None,
        reason: Some(EvaluationReason {
            code: "storage_collection_value_unavailable".into(),
            paths,
        }),
        recommendation: None,
    }
}

fn inapplicable_storage_evaluation(rule_id: &str, summary: &str) -> RuleEvaluation {
    RuleEvaluation {
        rule_id: rule_id.into(),
        rule_version: "1.0".into(),
        category: "storage".into(),
        status: RuleEvaluationStatus::NotApplicable,
        severity: None,
        summary: summary.into(),
        evidence: vec![],
        criterion: None,
        reason: Some(EvaluationReason {
            code: "storage_rule_not_applicable".into(),
            paths: vec!["/storage".into()],
        }),
        recommendation: None,
    }
}

fn evaluate_devices(collection: &Collection) -> Vec<RuleEvaluation> {
    let Some(devices) = collection.devices.as_deref() else {
        return vec![unavailable_device_evaluation(
            "device.device_problem",
            "デバイスの問題コードを評価できませんでした",
        )];
    };
    let present: Vec<(usize, &ConnectedDevice)> = devices
        .iter()
        .enumerate()
        .filter(|(_, device)| device.device_state.present == Some(true))
        .collect();
    if present.is_empty() {
        return vec![inapplicable_device_evaluation(
            "device.device_problem",
            "現在接続中のデバイスがありません",
        )];
    }
    vec![evaluate_device_problem_codes(&present)]
}

fn evaluate_device_problem_codes(devices: &[(usize, &ConnectedDevice)]) -> RuleEvaluation {
    let problem_codes: Vec<_> = devices
        .iter()
        .filter_map(|(_, device)| device.device_state.problem_code.filter(|code| *code != 0))
        .collect();
    let triggered = !problem_codes.is_empty();
    let disabled_only = triggered && problem_codes.iter().all(|code| *code == 22);
    let missing_paths: Vec<_> = devices
        .iter()
        .filter(|(_, device)| device.device_state.problem_code.is_none())
        .map(|(index, _)| format!("/devices/{index}/device_state/problem_code"))
        .collect();
    let evidence: Vec<_> = devices
        .iter()
        .filter_map(|(index, device)| {
            device
                .device_state
                .problem_code
                .map(|problem_code| Evidence::Collected {
                    path: format!("/devices/{index}/device_state/problem_code"),
                    value: json!(problem_code),
                })
        })
        .collect();
    if !triggered && !missing_paths.is_empty() {
        return incomplete_device_evaluation(
            "device.device_problem",
            "デバイスの問題コードを評価できませんでした",
            evidence,
            missing_paths,
            Criterion {
                operator: "not_equal".into(),
                threshold: json!(0),
                unit: None,
            },
        );
    }
    RuleEvaluation {
        rule_id: "device.device_problem".into(),
        rule_version: "1.1".into(),
        category: "device".into(),
        status: status(triggered),
        severity: triggered.then_some(if disabled_only {
            Severity::Warning
        } else {
            Severity::Error
        }),
        summary: if disabled_only {
            "Windowsで無効化されている接続デバイスがあります"
        } else if triggered {
            "Windowsが接続デバイスの問題を報告しています"
        } else {
            "接続デバイスの問題コードに異常はありません"
        }
        .into(),
        evidence,
        criterion: Some(Criterion {
            operator: "not_equal".into(),
            threshold: json!(0),
            unit: None,
        }),
        reason: (!missing_paths.is_empty()).then(|| EvaluationReason {
            code: "required_collection_value_unavailable".into(),
            paths: missing_paths,
        }),
        recommendation: triggered.then(|| Recommendation {
            code: if disabled_only {
                "enable_device"
            } else {
                "review_device_problem"
            }
            .into(),
        }),
    }
}

fn unavailable_device_evaluation(rule_id: &str, summary: &str) -> RuleEvaluation {
    RuleEvaluation {
        rule_id: rule_id.into(),
        rule_version: "1.1".into(),
        category: "device".into(),
        status: RuleEvaluationStatus::NotEvaluated,
        severity: None,
        summary: summary.into(),
        evidence: vec![],
        criterion: None,
        reason: Some(EvaluationReason {
            code: "device_collection_unavailable".into(),
            paths: vec!["/devices".into()],
        }),
        recommendation: None,
    }
}

fn inapplicable_device_evaluation(rule_id: &str, summary: &str) -> RuleEvaluation {
    RuleEvaluation {
        rule_id: rule_id.into(),
        rule_version: "1.1".into(),
        category: "device".into(),
        status: RuleEvaluationStatus::NotApplicable,
        severity: None,
        summary: summary.into(),
        evidence: vec![],
        criterion: None,
        reason: Some(EvaluationReason {
            code: "no_present_device".into(),
            paths: vec!["/devices".into()],
        }),
        recommendation: None,
    }
}

fn incomplete_device_evaluation(
    rule_id: &str,
    summary: &str,
    evidence: Vec<Evidence>,
    paths: Vec<String>,
    criterion: Criterion,
) -> RuleEvaluation {
    RuleEvaluation {
        rule_id: rule_id.into(),
        rule_version: "1.1".into(),
        category: "device".into(),
        status: RuleEvaluationStatus::NotEvaluated,
        severity: None,
        summary: summary.into(),
        evidence,
        criterion: Some(criterion),
        reason: Some(EvaluationReason {
            code: "required_collection_value_unavailable".into(),
            paths,
        }),
        recommendation: None,
    }
}

fn evaluate_gpus(collection: &Collection) -> Vec<RuleEvaluation> {
    let Some(gpus) = collection.gpus.as_deref() else {
        return vec![
            unavailable_gpu_evaluation(
                "gpu.device_problem",
                "1.1",
                "GPUの問題コードを評価できませんでした",
            ),
            unavailable_gpu_evaluation(
                "gpu.adapter_started",
                "1.0",
                "GPUの開始状態を評価できませんでした",
            ),
            unavailable_gpu_evaluation(
                "gpu.driver_version_available",
                "1.1",
                "GPUドライバー情報を評価できませんでした",
            ),
            unavailable_gpu_evaluation(
                "gpu.device_instance_id_unique",
                "1.0",
                "GPUのデバイスインスタンスIDを評価できませんでした",
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
            inapplicable_gpu_evaluation(
                "gpu.device_problem",
                "1.1",
                "評価対象となる物理GPUがありません",
            ),
            inapplicable_gpu_evaluation(
                "gpu.adapter_started",
                "1.0",
                "評価対象となる物理GPUがありません",
            ),
            inapplicable_gpu_evaluation(
                "gpu.driver_version_available",
                "1.1",
                "評価対象となる物理GPUがありません",
            ),
            inapplicable_gpu_evaluation(
                "gpu.device_instance_id_unique",
                "1.0",
                "評価対象となる物理GPUがありません",
            ),
        ];
    }
    vec![
        evaluate_gpu_problem_codes(&hardware),
        evaluate_gpu_started(&hardware),
        evaluate_gpu_driver_versions(&hardware),
        evaluate_gpu_instance_id_uniqueness(&hardware),
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
            "1.1",
            "GPUの問題コードを評価できませんでした",
            &missing,
            "device_state/problem_code",
        );
    }
    let is_triggered = !triggered.is_empty();
    RuleEvaluation {
        rule_id: "gpu.device_problem".into(),
        rule_version: "1.1".into(),
        category: "gpu".into(),
        status: status(is_triggered),
        severity: is_triggered.then_some(Severity::Error),
        summary: if is_triggered {
            "WindowsがGPUのデバイス問題を報告しています"
        } else {
            "GPUのデバイス問題コードに異常はありません"
        }
        .into(),
        evidence: gpus
            .iter()
            .filter_map(|(index, gpu)| {
                gpu.device_state
                    .problem_code
                    .map(|problem_code| Evidence::Collected {
                        path: format!("/gpus/{index}/device_state/problem_code"),
                        value: json!(problem_code),
                    })
            })
            .collect(),
        criterion: Some(Criterion {
            operator: "not_equal".into(),
            threshold: json!(0),
            unit: None,
        }),
        reason: (!missing.is_empty()).then(|| EvaluationReason {
            code: "required_collection_value_unavailable".into(),
            paths: missing
                .into_iter()
                .map(|(index, _)| format!("/gpus/{index}/device_state/problem_code"))
                .collect(),
        }),
        recommendation: is_triggered.then(|| Recommendation {
            code: "review_gpu_device_problem".into(),
        }),
    }
}

fn evaluate_gpu_started(gpus: &[(usize, &Gpu)]) -> RuleEvaluation {
    let triggered: Vec<_> = gpus
        .iter()
        .filter(|(_, gpu)| gpu.device_state.started == Some(false))
        .collect();
    let missing: Vec<_> = gpus
        .iter()
        .filter(|(_, gpu)| gpu.device_state.started.is_none())
        .collect();
    if triggered.is_empty() && !missing.is_empty() {
        return missing_gpu_value_evaluation(
            "gpu.adapter_started",
            "1.0",
            "GPUの開始状態を評価できませんでした",
            &missing,
            "device_state/started",
        );
    }
    let is_triggered = !triggered.is_empty();
    RuleEvaluation {
        rule_id: "gpu.adapter_started".into(),
        rule_version: "1.0".into(),
        category: "gpu".into(),
        status: status(is_triggered),
        severity: is_triggered.then_some(Severity::Warning),
        summary: if is_triggered {
            "開始されていない物理GPUがあります"
        } else {
            "物理GPUは開始されています"
        }
        .into(),
        evidence: gpus
            .iter()
            .filter_map(|(index, gpu)| {
                gpu.device_state.started.map(|started| Evidence::Collected {
                    path: format!("/gpus/{index}/device_state/started"),
                    value: json!(started),
                })
            })
            .collect(),
        criterion: Some(Criterion {
            operator: "equal".into(),
            threshold: json!(false),
            unit: None,
        }),
        reason: (!missing.is_empty()).then(|| EvaluationReason {
            code: "required_collection_value_unavailable".into(),
            paths: missing
                .into_iter()
                .map(|(index, _)| format!("/gpus/{index}/device_state/started"))
                .collect(),
        }),
        recommendation: is_triggered.then(|| Recommendation {
            code: "review_gpu_start_state".into(),
        }),
    }
}

fn evaluate_gpu_instance_id_uniqueness(gpus: &[(usize, &Gpu)]) -> RuleEvaluation {
    let mut occurrences: BTreeMap<String, Vec<(usize, &str)>> = BTreeMap::new();
    let mut missing_paths = Vec::new();
    for (index, gpu) in gpus {
        match gpu.device_instance_id.as_deref().map(str::trim) {
            Some(instance_id) if !instance_id.is_empty() => occurrences
                .entry(instance_id.to_ascii_uppercase())
                .or_default()
                .push((*index, instance_id)),
            _ => missing_paths.push(format!("/gpus/{index}/device_instance_id")),
        }
    }
    let duplicate_indices: Vec<_> = occurrences
        .values()
        .filter(|items| items.len() > 1)
        .flat_map(|items| items.iter().copied())
        .collect();
    if duplicate_indices.is_empty() && !missing_paths.is_empty() {
        return RuleEvaluation {
            rule_id: "gpu.device_instance_id_unique".into(),
            rule_version: "1.0".into(),
            category: "gpu".into(),
            status: RuleEvaluationStatus::NotEvaluated,
            severity: None,
            summary: "GPUのデバイスインスタンスIDの一意性を評価できませんでした".into(),
            evidence: occurrences
                .values()
                .flatten()
                .map(|(index, instance_id)| Evidence::Collected {
                    path: format!("/gpus/{index}/device_instance_id"),
                    value: json!(instance_id),
                })
                .collect(),
            criterion: Some(Criterion {
                operator: "unique".into(),
                threshold: json!(true),
                unit: None,
            }),
            reason: Some(EvaluationReason {
                code: "required_collection_value_unavailable".into(),
                paths: missing_paths,
            }),
            recommendation: None,
        };
    }
    let triggered = !duplicate_indices.is_empty();
    RuleEvaluation {
        rule_id: "gpu.device_instance_id_unique".into(),
        rule_version: "1.0".into(),
        category: "gpu".into(),
        status: status(triggered),
        severity: triggered.then_some(Severity::Warning),
        summary: if triggered {
            "重複したGPUデバイスインスタンスIDがあります"
        } else {
            "GPUのデバイスインスタンスIDは一意です"
        }
        .into(),
        evidence: if triggered {
            duplicate_indices
                .into_iter()
                .map(|(index, instance_id)| Evidence::Collected {
                    path: format!("/gpus/{index}/device_instance_id"),
                    value: json!(instance_id),
                })
                .collect()
        } else {
            occurrences
                .values()
                .flatten()
                .map(|(index, instance_id)| Evidence::Collected {
                    path: format!("/gpus/{index}/device_instance_id"),
                    value: json!(instance_id),
                })
                .collect()
        },
        criterion: Some(Criterion {
            operator: "unique".into(),
            threshold: json!(true),
            unit: None,
        }),
        reason: (!missing_paths.is_empty()).then(|| EvaluationReason {
            code: "required_collection_value_unavailable".into(),
            paths: missing_paths,
        }),
        recommendation: triggered.then(|| Recommendation {
            code: "review_gpu_enumeration".into(),
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
        rule_version: "1.1".into(),
        category: "gpu".into(),
        status: status(is_triggered),
        severity: is_triggered.then_some(Severity::Warning),
        summary: if is_triggered {
            "ドライバーバージョンを確認できない物理GPUがあります"
        } else {
            "物理GPUのドライバーバージョンを確認できました"
        }
        .into(),
        evidence: gpus
            .iter()
            .filter_map(|(index, gpu)| {
                gpu.driver
                    .version
                    .as_ref()
                    .map(|version| Evidence::Collected {
                        path: format!("/gpus/{index}/driver/version"),
                        value: json!(version),
                    })
            })
            .collect(),
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

fn unavailable_gpu_evaluation(rule_id: &str, rule_version: &str, summary: &str) -> RuleEvaluation {
    RuleEvaluation {
        rule_id: rule_id.into(),
        rule_version: rule_version.into(),
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

fn inapplicable_gpu_evaluation(rule_id: &str, rule_version: &str, summary: &str) -> RuleEvaluation {
    RuleEvaluation {
        rule_id: rule_id.into(),
        rule_version: rule_version.into(),
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
    rule_version: &str,
    summary: &str,
    gpus: &[&(usize, &Gpu)],
    suffix: &str,
) -> RuleEvaluation {
    RuleEvaluation {
        rule_id: rule_id.into(),
        rule_version: rule_version.into(),
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
    let mut overall: Option<Severity> = None;
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
                Some(current) => severity.rank() > current.rank(),
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

    fn device_collection() -> Collection {
        let mut collection = gpu_collection();
        collection.devices = Some(vec![
            serde_json::from_value(json!({
                "name": "Present Device",
                "manufacturer": "Example Vendor",
                "class": "USB",
                "class_guid": "{00000000-0000-0000-0000-000000000001}",
                "device_instance_id": "USB\\VID_1234&PID_5678\\PRESENT",
                "device_state": {
                    "present": true,
                    "started": true,
                    "problem_code": 0
                },
                "driver": {
                    "version": "1.2.3.4",
                    "date": "2026-07-18"
                }
            }))
            .unwrap(),
            serde_json::from_value(json!({
                "name": "Past Device",
                "manufacturer": "Example Vendor",
                "class": "USB",
                "class_guid": "{00000000-0000-0000-0000-000000000002}",
                "device_instance_id": "USB\\VID_1234&PID_5678\\PAST",
                "device_state": {
                    "present": false,
                    "started": null,
                    "problem_code": null
                },
                "driver": {
                    "version": null,
                    "date": null
                }
            }))
            .unwrap(),
        ]);
        collection
    }

    fn storage_collection() -> Collection {
        let mut collection = gpu_collection();
        collection.storage = serde_json::from_value(json!({
            "disks": [
                {
                    "number": 0,
                    "model": "Example NVMe",
                    "manufacturer": "Example",
                    "firmware_revision": "1.0",
                    "bus_type": "nvme",
                    "capacity_bytes": 1000000000000_u64,
                    "logical_sector_size_bytes": 512,
                    "removable": false
                },
                {
                    "number": 1,
                    "model": "Example SATA",
                    "manufacturer": "Example",
                    "firmware_revision": "1.0",
                    "bus_type": "sata",
                    "capacity_bytes": 500000000000_u64,
                    "logical_sector_size_bytes": 512,
                    "removable": false
                }
            ],
            "partitions": [],
            "volumes": [{
                "mount_points": ["C:\\"],
                "file_system": "NTFS",
                "capacity_bytes": 100000000000_u64,
                "free_bytes": 20000000000_u64,
                "extents": []
            }],
            "smart": [
                {
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
                },
                {
                    "disk_number": 1,
                    "protocol": "failure_prediction",
                    "predict_failure": false,
                    "critical_warning": null,
                    "temperature_celsius": null,
                    "available_spare_percent": null,
                    "percentage_used": null,
                    "power_on_hours": null,
                    "unsafe_shutdowns": null,
                    "media_errors": null
                }
            ]
        }))
        .unwrap();
        collection
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

        assert_eq!(diagnosis.rule_set.version, "0.8.0");
        assert!(diagnosis.evaluations[1..5].iter().all(|evaluation| {
            evaluation.category == "gpu" && evaluation.status == RuleEvaluationStatus::Passed
        }));
        assert_eq!(diagnosis.evaluations[1].evidence.len(), 1);
        assert_eq!(diagnosis.evaluations[2].evidence.len(), 1);
        assert_eq!(diagnosis.evaluations[3].evidence.len(), 1);
        assert_eq!(diagnosis.evaluations[4].evidence.len(), 1);
        assert!(matches!(
            &diagnosis.evaluations[1].evidence[0],
            Evidence::Collected { path, value }
                if path == "/gpus/0/device_state/problem_code" && value == &json!(0)
        ));
        assert!(matches!(
            &diagnosis.evaluations[2].evidence[0],
            Evidence::Collected { path, value }
                if path == "/gpus/0/device_state/started" && value == &json!(true)
        ));
        assert!(matches!(
            &diagnosis.evaluations[3].evidence[0],
            Evidence::Collected { path, value }
                if path == "/gpus/0/driver/version" && value == &json!("1.2.3.4")
        ));
        assert!(matches!(
            &diagnosis.evaluations[4].evidence[0],
            Evidence::Collected { path, value }
                if path == "/gpus/0/device_instance_id"
                    && value == &json!("PCI\\VEN_1234&DEV_5678&SUBSYS_00000000&REV_01\\TEST")
        ));
        diagnosis.validate_against(&collection).unwrap();
    }

    #[test]
    fn evaluates_only_present_connected_devices() {
        let collection = device_collection();
        let diagnosis = diagnose_collection(&collection);
        let evaluation = &diagnosis.evaluations[5];

        assert_eq!(evaluation.category, "device");
        assert_eq!(evaluation.status, RuleEvaluationStatus::Passed);
        assert_eq!(evaluation.evidence.len(), 1);
        assert!(matches!(
            &evaluation.evidence[0],
            Evidence::Collected { path, value }
                if path == "/devices/0/device_state/problem_code" && value == &json!(0)
        ));
        diagnosis.validate_against(&collection).unwrap();
    }

    #[test]
    fn reports_problem_code_22_as_disabled_device_warning() {
        let mut collection = device_collection();
        let device = &mut collection.devices.as_mut().unwrap()[0];
        device.device_state.problem_code = Some(22);
        device.device_state.started = Some(false);
        device.driver.version = None;
        let diagnosis = diagnose_collection(&collection);
        let evaluation = &diagnosis.evaluations[5];

        assert_eq!(evaluation.status, RuleEvaluationStatus::Triggered);
        assert_eq!(evaluation.severity, Some(Severity::Warning));
        assert_eq!(
            evaluation.recommendation.as_ref().unwrap().code,
            "enable_device"
        );
        diagnosis.validate_against(&collection).unwrap();
    }

    #[test]
    fn reports_other_device_problem_codes_as_errors() {
        let mut collection = device_collection();
        collection.devices.as_mut().unwrap()[0]
            .device_state
            .problem_code = Some(28);
        let diagnosis = diagnose_collection(&collection);
        let evaluation = &diagnosis.evaluations[5];

        assert_eq!(evaluation.status, RuleEvaluationStatus::Triggered);
        assert_eq!(evaluation.severity, Some(Severity::Error));
        assert_eq!(
            evaluation.recommendation.as_ref().unwrap().code,
            "review_device_problem"
        );
        diagnosis.validate_against(&collection).unwrap();
    }

    #[test]
    fn passes_all_storage_rules_for_healthy_values() {
        let collection = storage_collection();
        let diagnosis = diagnose_collection(&collection);
        let evaluations = &diagnosis.evaluations[6..10];

        assert!(evaluations.iter().all(|evaluation| {
            evaluation.category == "storage" && evaluation.status == RuleEvaluationStatus::Passed
        }));
        assert_eq!(evaluations[0].evidence.len(), 1);
        assert_eq!(evaluations[1].evidence.len(), 1);
        assert_eq!(evaluations[2].evidence.len(), 1);
        assert_eq!(evaluations[3].evidence.len(), 3);
        diagnosis.validate_against(&collection).unwrap();
    }

    #[test]
    fn reports_all_initial_storage_findings() {
        let mut collection = storage_collection();
        let smart = collection.storage.smart.as_mut().unwrap();
        smart[0].critical_warning = Some(1);
        smart[0].percentage_used = Some(100);
        smart[1].predict_failure = Some(true);
        let volume = &mut collection.storage.volumes.as_mut().unwrap()[0];
        volume.free_bytes = Some(5 * 1024 * 1024 * 1024);
        let diagnosis = diagnose_collection(&collection);
        let evaluations = &diagnosis.evaluations[6..10];

        assert!(
            evaluations
                .iter()
                .all(|evaluation| evaluation.status == RuleEvaluationStatus::Triggered)
        );
        assert_eq!(evaluations[0].severity, Some(Severity::Critical));
        assert_eq!(evaluations[1].severity, Some(Severity::Error));
        assert_eq!(evaluations[2].severity, Some(Severity::Warning));
        assert_eq!(evaluations[3].severity, Some(Severity::Warning));
        assert_eq!(diagnosis.summary.overall_severity, Some(Severity::Critical));
        diagnosis.validate_against(&collection).unwrap();
    }

    #[test]
    fn volume_free_space_requires_both_thresholds() {
        let mut collection = storage_collection();
        let volume = &mut collection.storage.volumes.as_mut().unwrap()[0];
        volume.capacity_bytes = Some(1_000_000_000_000);
        volume.free_bytes = Some(50_000_000_000);
        let diagnosis = diagnose_collection(&collection);

        assert_eq!(
            diagnosis.evaluations[9].status,
            RuleEvaluationStatus::Passed
        );
        diagnosis.validate_against(&collection).unwrap();
    }

    #[test]
    fn records_unavailable_and_inapplicable_storage_rules() {
        let mut collection = storage_collection();
        collection.storage.smart = None;
        collection.storage.volumes.as_mut().unwrap()[0].mount_points = Some(vec![]);
        let diagnosis = diagnose_collection(&collection);

        assert!(diagnosis.evaluations[6..9].iter().all(|evaluation| {
            evaluation.status == RuleEvaluationStatus::NotEvaluated && evaluation.reason.is_some()
        }));
        assert_eq!(
            diagnosis.evaluations[9].status,
            RuleEvaluationStatus::NotApplicable
        );
        diagnosis.validate_against(&collection).unwrap();
    }

    #[test]
    fn does_not_pass_when_present_device_problem_code_is_unavailable() {
        let mut collection = device_collection();
        collection.devices.as_mut().unwrap()[0]
            .device_state
            .problem_code = None;
        let diagnosis = diagnose_collection(&collection);
        let evaluation = &diagnosis.evaluations[5];

        assert_eq!(evaluation.status, RuleEvaluationStatus::NotEvaluated);
        assert_eq!(
            evaluation.reason.as_ref().unwrap().paths,
            vec!["/devices/0/device_state/problem_code"]
        );
        diagnosis.validate_against(&collection).unwrap();
    }

    #[test]
    fn reports_gpu_problem_disabled_adapter_and_missing_driver() {
        let mut collection = gpu_collection();
        let gpu = &mut collection.gpus.as_mut().unwrap()[0];
        gpu.device_state.problem_code = Some(22);
        gpu.device_state.started = Some(false);
        gpu.driver.version = None;
        let diagnosis = diagnose_collection(&collection);

        assert_eq!(diagnosis.summary.evaluations.triggered, 4);
        assert_eq!(diagnosis.summary.findings.error, 1);
        assert_eq!(diagnosis.summary.findings.warning, 3);
        assert_eq!(diagnosis.summary.overall_severity, Some(Severity::Error));
        diagnosis.validate_against(&collection).unwrap();
    }

    #[test]
    fn reports_duplicate_gpu_instance_ids_case_insensitively() {
        let mut collection = gpu_collection();
        let mut duplicate = collection.gpus.as_ref().unwrap()[0].clone();
        duplicate.device_instance_id = duplicate
            .device_instance_id
            .as_ref()
            .map(|instance_id| instance_id.to_ascii_lowercase());
        collection.gpus.as_mut().unwrap().push(duplicate);
        let diagnosis = diagnose_collection(&collection);
        let evaluation = &diagnosis.evaluations[4];

        assert_eq!(evaluation.status, RuleEvaluationStatus::Triggered);
        assert_eq!(evaluation.severity, Some(Severity::Warning));
        assert_eq!(evaluation.evidence.len(), 2);
        assert_eq!(
            evaluation.recommendation.as_ref().unwrap().code,
            "review_gpu_enumeration"
        );
        diagnosis.validate_against(&collection).unwrap();
    }

    #[test]
    fn diagnoses_high_priority_windows_events() {
        let mut collection = collection();
        collection.event_logs.security = Some(vec![EventLogEntry {
            occurred_at: "2026-07-30T12:00:00Z".into(),
            log_name: "Security".into(),
            provider: "Microsoft-Windows-Eventlog".into(),
            event_id: 1102,
            level: crate::EventLogLevel::Information,
            summary: "監査ログが消去されました".into(),
        }]);
        let diagnosis = diagnose_collection(&collection);
        let finding = diagnosis
            .evaluations
            .iter()
            .find(|evaluation| evaluation.rule_id == "event_log.security.audit_log_cleared")
            .unwrap();

        assert_eq!(finding.status, RuleEvaluationStatus::Triggered);
        assert_eq!(finding.severity, Some(Severity::Critical));
        assert_eq!(
            finding.recommendation.as_ref().unwrap().code,
            "investigate_audit_log_clearance"
        );
        diagnosis.validate_against(&collection).unwrap();
    }

    #[test]
    fn filters_noisy_events_and_aggregates_repeated_findings() {
        let mut collection = collection();
        collection.event_logs.system = Some(vec![EventLogEntry {
            occurred_at: "2026-07-30T10:00:00Z".into(),
            log_name: "System".into(),
            provider: "Microsoft-Windows-DistributedCOM".into(),
            event_id: 10016,
            level: crate::EventLogLevel::Warning,
            summary: "DCOM permission warning".into(),
        }]);
        collection.event_logs.application = Some(vec![
            EventLogEntry {
                occurred_at: "2026-07-29T10:00:00Z".into(),
                log_name: "Application".into(),
                provider: "Application Error".into(),
                event_id: 1000,
                level: crate::EventLogLevel::Error,
                summary: "first crash".into(),
            },
            EventLogEntry {
                occurred_at: "2026-07-30T10:00:00Z".into(),
                log_name: "Application".into(),
                provider: "Application Error".into(),
                event_id: 1000,
                level: crate::EventLogLevel::Error,
                summary: "latest crash".into(),
            },
        ]);

        let diagnosis = diagnose_collection(&collection);
        let event_findings: Vec<_> = diagnosis
            .evaluations
            .iter()
            .filter(|evaluation| evaluation.category == "event_log")
            .collect();

        assert_eq!(event_findings.len(), 1);
        assert_eq!(
            event_findings[0].rule_id,
            "event_log.application.application_failure"
        );
        assert!(event_findings[0].summary.contains("2件"));
        assert!(event_findings[0].summary.contains("latest crash"));
        assert_eq!(event_findings[0].evidence.len(), 1);
        diagnosis.validate_against(&collection).unwrap();
    }

    #[test]
    fn correlates_shutdown_event_pairs_into_estimated_occurrences() {
        let mut collection = collection();
        collection.event_logs.system = Some(vec![
            shutdown_event(41, "2026-07-30T23:59:50Z"),
            shutdown_event(6008, "2026-07-31T00:00:03Z"),
            shutdown_event(41, "2026-07-31T02:00:00Z"),
            shutdown_event(6008, "2026-07-31T04:00:00Z"),
        ]);

        let diagnosis = diagnose_collection(&collection);
        let finding = diagnosis
            .evaluations
            .iter()
            .find(|evaluation| evaluation.rule_id == "event_log.system.unexpected_shutdown")
            .unwrap();

        assert!(finding.summary.contains("推定3回"));
        diagnosis.validate_against(&collection).unwrap();
    }

    fn shutdown_event(event_id: u32, occurred_at: &str) -> EventLogEntry {
        EventLogEntry {
            occurred_at: occurred_at.into(),
            log_name: "System".into(),
            provider: if event_id == 41 {
                "Microsoft-Windows-Kernel-Power"
            } else {
                "EventLog"
            }
            .into(),
            event_id,
            level: crate::EventLogLevel::Critical,
            summary: format!("shutdown event {event_id}"),
        }
    }
}
