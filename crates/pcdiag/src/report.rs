use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
    time::Instant,
};

use pcdiag_core::{
    ArtifactFile, ArtifactInput, ArtifactManifest, ArtifactStatus, ArtifactType, Collection,
    CollectionStatus, Diagnosis, DiskSmart, Evidence, LoadedCollectionArtifact,
    LoadedDiagnosisArtifact, RuleEvaluationStatus, Severity, SmartProtocol, ToolInfo,
    WindowsUpdateHistoryEntry, WindowsUpdateResult, load_collection_artifact,
    load_diagnosis_artifact, sha256_hex,
};

use crate::bundle::{self, pretty_json, write_new};

pub fn generate_report(session_directory: &Path) -> Result<PathBuf, ReportError> {
    let started = Instant::now();
    let started_at = bundle::platform::utc_timestamp()?;
    let observed_utc_offset_minutes = bundle::platform::utc_offset_minutes()?;
    let collection = load_collection_artifact(&session_directory.join("collection"))?;
    let diagnosis = load_diagnosis_artifact(&session_directory.join("diagnosis"), &collection)?;
    let artifact_id = unique_artifact_id(&collection, &diagnosis)?;
    let html = render_html(&collection, &diagnosis);
    let completed_at = bundle::platform::utc_timestamp()?;
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    write_report(
        session_directory,
        collection,
        diagnosis,
        html.as_bytes(),
        ReportTiming {
            artifact_id,
            started_at,
            completed_at,
            observed_utc_offset_minutes,
            duration_ms,
        },
    )
}

fn unique_artifact_id(
    collection: &LoadedCollectionArtifact,
    diagnosis: &LoadedDiagnosisArtifact,
) -> Result<String, ReportError> {
    for _ in 0..16 {
        let id = bundle::platform::uuid_v4()?;
        if id != collection.manifest.session_id
            && id != collection.manifest.artifact_id
            && id != diagnosis.manifest.artifact_id
        {
            return Ok(id);
        }
    }
    Err(ReportError::ArtifactIdCollision)
}

struct ReportTiming {
    artifact_id: String,
    started_at: String,
    completed_at: String,
    observed_utc_offset_minutes: i32,
    duration_ms: u64,
}

fn write_report(
    session_directory: &Path,
    collection: LoadedCollectionArtifact,
    diagnosis: LoadedDiagnosisArtifact,
    html: &[u8],
    timing: ReportTiming,
) -> Result<PathBuf, ReportError> {
    let final_directory = session_directory.join("report");
    let incomplete_directory = session_directory.join("report.incomplete");
    if final_directory.exists() {
        return Err(ReportError::AlreadyExists(final_directory));
    }
    if incomplete_directory.exists() {
        return Err(ReportError::IncompleteExists(incomplete_directory));
    }
    fs::create_dir(&incomplete_directory)?;
    write_new(&incomplete_directory.join("report.html"), html)?;
    let manifest = ArtifactManifest {
        manifest_schema_version: "1.0".into(),
        artifact_schema_version: "2.0".into(),
        session_id: collection.manifest.session_id,
        artifact_id: timing.artifact_id,
        artifact_type: ArtifactType::Report,
        status: ArtifactStatus::Complete,
        started_at: timing.started_at,
        completed_at: timing.completed_at,
        observed_utc_offset_minutes: timing.observed_utc_offset_minutes,
        duration_ms: timing.duration_ms,
        tool: ToolInfo {
            name: "pcdiag".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        inputs: vec![
            ArtifactInput {
                artifact_id: collection.manifest.artifact_id,
                artifact_type: ArtifactType::Collection,
            },
            ArtifactInput {
                artifact_id: diagnosis.manifest.artifact_id,
                artifact_type: ArtifactType::Diagnosis,
            },
        ],
        files: vec![ArtifactFile {
            path: "report.html".into(),
            media_type: "text/html; charset=utf-8".into(),
            size_bytes: u64::try_from(html.len()).unwrap_or(u64::MAX),
            sha256: sha256_hex(html),
        }],
    };
    manifest.validate()?;
    write_new(
        &incomplete_directory.join("manifest.json"),
        &pretty_json(&manifest)?,
    )?;
    fs::rename(&incomplete_directory, &final_directory)?;
    Ok(final_directory)
}

fn render_html(
    collection: &LoadedCollectionArtifact,
    diagnosis: &LoadedDiagnosisArtifact,
) -> String {
    let data = &collection.collection;
    let result = &diagnosis.diagnosis;
    let mut html = String::from(
        "<!doctype html><html lang=\"ja\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>pcdiag 診断レポート</title><style>\
        :root{color-scheme:light;--bg:#f4f6f8;--card:#fff;--ink:#17202a;--muted:#607080;--line:#dce2e7;--ok:#18794e;--info:#1f6feb;--warn:#9a6700;--error:#cf222e;--critical:#7a0019}\
        *{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:15px/1.55 system-ui,-apple-system,\"Segoe UI\",sans-serif}main{max-width:1180px;margin:auto;padding:32px 20px 64px}h1{margin:0}h2{margin-top:0}section{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:20px;margin-top:18px}table{width:100%;border-collapse:collapse}th,td{text-align:left;vertical-align:top;border-bottom:1px solid var(--line);padding:8px}th{color:var(--muted);font-weight:600}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:12px}.metric{border:1px solid var(--line);border-radius:8px;padding:12px}.metric b{display:block;font-size:1.45rem}.muted{color:var(--muted)}.badge{display:inline-block;border-radius:999px;padding:2px 9px;font-weight:700}.passed{color:var(--ok)}.information{color:var(--info)}.warning{color:var(--warn)}.error{color:var(--error)}.critical{color:var(--critical)}code{overflow-wrap:anywhere}.finding{border-left:5px solid var(--line);padding:10px 14px;margin:12px 0}.finding.warning{border-color:var(--warn)}.finding.error{border-color:var(--error)}.finding.critical{border-color:var(--critical)}.finding.information{border-color:var(--info)}.artifact-notice{background:#fff8c5;border:1px solid #d4a72c;border-radius:10px;padding:20px;margin-top:18px}.artifact-notice h2{font-size:1.1rem}.artifact-notice p{margin-bottom:0}@media print{body{background:#fff}main{max-width:none;padding:0}section,.artifact-notice{break-inside:avoid;border-color:#777}}\
        details>summary{cursor:pointer;font-size:1.35rem;font-weight:700}details[open]>summary{margin-bottom:12px}.accordion-stack{display:grid;gap:10px}.accordion-stack details{border:1px solid var(--line);border-radius:8px;padding:10px 12px}.accordion-stack details>summary{font-size:1rem}.table-wrap{overflow-x:auto}.update-title{min-width:300px}.nowrap{white-space:nowrap}\
        </style></head><body><main>",
    );
    let collection_is_partial = collection.manifest.status == ArtifactStatus::Partial;
    let severity = result
        .summary
        .overall_severity
        .map(severity_text)
        .unwrap_or(if collection_is_partial {
            "診断範囲では異常所見なし"
        } else {
            "異常所見なし"
        });
    write!(
        html,
        "<header><h1>pcdiag 診断レポート</h1><p class=\"muted\">セッション <code>{}</code> / 規則セット {} {}</p></header>",
        escape(&collection.manifest.session_id),
        escape(&result.rule_set.name),
        escape(&result.rule_set.version)
    )
    .unwrap();
    write!(html, "<section><h2>診断概要</h2><div class=\"grid\"><div class=\"metric\"><span>総合判定</span><b class=\"{}\">{severity}</b></div>", severity_class(result.summary.overall_severity)).unwrap();
    for (label, count) in [
        ("Critical", result.summary.findings.critical),
        ("Error", result.summary.findings.error),
        ("Warning", result.summary.findings.warning),
        ("Information", result.summary.findings.information),
    ] {
        write!(
            html,
            "<div class=\"metric\"><span>{label}</span><b>{count}</b></div>"
        )
        .unwrap();
    }
    html.push_str("</div></section>");
    if collection_is_partial {
        html.push_str("<section class=\"finding warning\"><h2>情報収集に関する注意</h2><p>一部の情報を取得できなかったため、このレポートには未評価の情報が含まれる可能性があります。診断結果は、取得できた情報と現在の診断規則の範囲に基づきます。</p></section>");
    }
    render_findings(&mut html, result);
    render_event_logs(&mut html, data, result);
    render_windows_updates(&mut html, data);
    render_system(&mut html, data);
    render_gpu(&mut html, data);
    render_storage(&mut html, data);
    render_devices(&mut html, data);
    render_collection_status(&mut html, &collection.status);
    write!(html, "<section><h2>成果物情報</h2><table><tr><th>収集</th><td><code>{}</code> ({:?})</td></tr><tr><th>診断</th><td><code>{}</code> ({:?})</td></tr><tr><th>レポート生成ツール</th><td>pcdiag {}</td></tr></table></section>", escape(&collection.manifest.artifact_id), collection.manifest.status, escape(&diagnosis.manifest.artifact_id), diagnosis.manifest.status, env!("CARGO_PKG_VERSION")).unwrap();
    html.push_str("<footer class=\"artifact-notice\"><h2>成果物の取り扱いに関する注意</h2><p>この診断成果物には、診断に必要な端末情報、アカウント識別情報、ネットワーク情報、ファイルパス、イベント内容などが含まれる場合があります。保存先、共有範囲、保管期間、廃棄は担当者が管理してください。pcdiagは成果物を自動削除しません。</p></footer>");
    html.push_str("</main></body></html>\n");
    html
}

fn render_windows_updates(html: &mut String, data: &Collection) {
    let Some(history) = &data.windows_updates.history else {
        html.push_str(
            "<section><h2>Windows Update</h2><p>更新履歴を取得できませんでした。</p></section>",
        );
        return;
    };
    let mut succeeded: Vec<_> = history
        .iter()
        .filter(|entry| entry.result == WindowsUpdateResult::Succeeded)
        .collect();
    let mut failed: Vec<_> = history
        .iter()
        .filter(|entry| entry.result == WindowsUpdateResult::Failed)
        .collect();
    let mut aborted: Vec<_> = history
        .iter()
        .filter(|entry| entry.result == WindowsUpdateResult::Aborted)
        .collect();
    for entries in [&mut succeeded, &mut failed, &mut aborted] {
        entries.sort_by(|left, right| right.occurred_at.cmp(&left.occurred_at));
    }
    succeeded.truncate(10);

    write!(
        html,
        "<section><h2>Windows Update</h2><p class=\"muted\">収集履歴: {}件 / 対象期間: {} / 最大件数: {}</p><div class=\"accordion-stack\">",
        history.len(),
        data.windows_updates
            .lookback_days
            .map_or_else(|| "全期間".into(), |days| format!("過去{days}日")),
        data.windows_updates
            .max_entries
            .map_or_else(|| "制限なし".into(), |count| format!("{count}件")),
    )
    .unwrap();
    render_windows_update_group(
        html,
        "最近成功した更新",
        &succeeded,
        "passed",
        "該当する成功履歴はありません。",
    );
    render_windows_update_group(
        html,
        "失敗した更新",
        &failed,
        "error",
        "失敗した更新はありません。",
    );
    render_windows_update_group(
        html,
        "中断された更新",
        &aborted,
        "warning",
        "中断された更新はありません。",
    );
    html.push_str("</div></section>");
}

fn render_windows_update_group(
    html: &mut String,
    label: &str,
    entries: &[&WindowsUpdateHistoryEntry],
    class: &str,
    empty_message: &str,
) {
    write!(
        html,
        "<details><summary><span class=\"{}\">{}</span>（{}件）</summary>",
        class,
        escape(label),
        entries.len()
    )
    .unwrap();
    if entries.is_empty() {
        write!(html, "<p class=\"muted\">{}</p>", escape(empty_message)).unwrap();
    } else {
        html.push_str("<div class=\"table-wrap\"><table><thead><tr><th>実行日時</th><th>更新内容</th><th>KB</th><th>結果</th><th>HResult</th><th>実行元</th></tr></thead><tbody>");
        for entry in entries {
            let kb_ids = if entry.kb_ids.is_empty() {
                "—".into()
            } else {
                entry.kb_ids.join(", ")
            };
            write!(
                html,
                "<tr><td class=\"nowrap\">{}</td><td class=\"update-title\">{}</td><td>{}</td><td class=\"{}\">{}</td><td><code>{} (0x{:08X})</code></td><td>{}</td></tr>",
                escape(&format_jst(&entry.occurred_at)),
                escape(entry.title.as_deref().unwrap_or("タイトルなし")),
                escape(&kb_ids),
                class,
                windows_update_result_text(entry.result),
                entry.hresult,
                entry.hresult as u32,
                escape(entry.client_application_id.as_deref().unwrap_or("—")),
            )
            .unwrap();
        }
        html.push_str("</tbody></table></div>");
    }
    html.push_str("</details>");
}

fn windows_update_result_text(result: WindowsUpdateResult) -> &'static str {
    match result {
        WindowsUpdateResult::NotStarted => "未開始",
        WindowsUpdateResult::InProgress => "処理中",
        WindowsUpdateResult::Succeeded => "成功",
        WindowsUpdateResult::SucceededWithErrors => "一部エラーあり",
        WindowsUpdateResult::Failed => "失敗",
        WindowsUpdateResult::Aborted => "中断",
        WindowsUpdateResult::Unknown => "不明",
    }
}

fn format_jst(value: &str) -> String {
    let Some(utc) = value.strip_suffix('Z') else {
        return value.into();
    };
    let Some((date, time)) = utc.split_once('T') else {
        return value.into();
    };
    let date_parts: Vec<_> = date.split('-').collect();
    let time_parts: Vec<_> = time.split(':').collect();
    if date_parts.len() != 3 || time_parts.len() != 3 {
        return value.into();
    }
    let seconds_text = time_parts[2]
        .split_once('.')
        .map_or(time_parts[2], |(seconds, _)| seconds);
    let (Ok(mut year), Ok(mut month), Ok(mut day), Ok(mut hour), Ok(minute), Ok(second)) = (
        date_parts[0].parse::<u32>(),
        date_parts[1].parse::<u32>(),
        date_parts[2].parse::<u32>(),
        time_parts[0].parse::<u32>(),
        time_parts[1].parse::<u32>(),
        seconds_text.parse::<u32>(),
    ) else {
        return value.into();
    };
    if year == 0
        || !(1..=12).contains(&month)
        || day == 0
        || day > report_days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return value.into();
    }
    hour += 9;
    if hour >= 24 {
        hour -= 24;
        day += 1;
        if day > report_days_in_month(year, month) {
            day = 1;
            month += 1;
            if month > 12 {
                month = 1;
                year += 1;
            }
        }
    }
    format!(
        "{year:04}-{month:02}-{day:02} {hour:02}:{}:{} JST",
        time_parts[1], time_parts[2]
    )
}

fn report_days_in_month(year: u32, month: u32) -> u32 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100)) => {
            29
        }
        2 => 28,
        _ => 31,
    }
}

fn render_event_logs(html: &mut String, data: &Collection, diagnosis: &Diagnosis) {
    let evaluations: Vec<_> = diagnosis
        .evaluations
        .iter()
        .filter(|item| {
            item.category == "event_log" && item.status == RuleEvaluationStatus::Triggered
        })
        .collect();
    html.push_str("<section><details open>");
    write!(
        html,
        "<summary>Windowsイベントログ（検出 {}件）</summary><p class=\"muted\">収集期間: 過去{}日（各ログ最大1000件） / System: {} / Application: {} / Security: {}</p>",
        evaluations.len(),
        data.event_logs.lookback_days,
        count_or_unavailable(data.event_logs.system.as_ref().map(Vec::len)),
        count_or_unavailable(data.event_logs.application.as_ref().map(Vec::len)),
        count_or_unavailable(data.event_logs.security.as_ref().map(Vec::len)),
    )
    .unwrap();
    html.push_str("<table><thead><tr><th>重大度</th><th>発生時刻</th><th>概要</th><th>ログ名</th><th>対処の目安</th></tr></thead><tbody>");
    for evaluation in &evaluations {
        let severity = evaluation.severity.unwrap_or(Severity::Information);
        let event = evaluation
            .evidence
            .iter()
            .find_map(|evidence| match evidence {
                Evidence::Collected { value, .. } => Some(value),
                Evidence::Derived { .. } => None,
            });
        let occurred_at = event
            .and_then(|value| value.get("occurred_at"))
            .and_then(|value| value.as_str())
            .unwrap_or("—");
        let log_name = event
            .and_then(|value| value.get("log_name"))
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| event_log_name_from_rule(&evaluation.rule_id));
        let recommendation = evaluation
            .recommendation
            .as_ref()
            .map(|value| event_recommendation_text(&value.code))
            .unwrap_or("イベントの詳細を確認してください。");
        write!(
            html,
            "<tr><td class=\"{}\">{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            severity_class(Some(severity)),
            severity_text(severity),
            escape(occurred_at),
            escape(&evaluation.summary),
            escape(log_name),
            recommendation,
        )
        .unwrap();
    }
    if evaluations.is_empty() {
        html.push_str("<tr><td colspan=\"5\" class=\"passed\">取得できた範囲に高優先度イベントはありません。</td></tr>");
    }
    html.push_str("</tbody></table></details></section>");
}

fn count_or_unavailable(value: Option<usize>) -> String {
    value.map_or_else(|| "未取得".into(), |count| format!("{count}件"))
}

fn event_log_name_from_rule(rule_id: &str) -> &'static str {
    if rule_id.starts_with("event_log.system.") {
        "System"
    } else if rule_id.starts_with("event_log.application.") {
        "Application"
    } else {
        "Security"
    }
}

fn event_recommendation_text(code: &str) -> &'static str {
    match code {
        "restore_event_log_collection" => {
            "権限、イベントログサービス、監査設定、ログの状態を確認してください。"
        }
        "investigate_audit_log_clearance" => "監査ログ消去の実施者と経緯を確認してください。",
        "review_failed_logons" => "失敗したログオンの対象・発生元・頻度を確認してください。",
        "review_audit_policy_change" => "監査ポリシー変更が承認済みか確認してください。",
        "investigate_unexpected_shutdown" => {
            "電源、温度、ドライバー、直前の操作を確認してください。"
        }
        "review_storage_io_failure" => "ストレージ、ケーブル、ドライバーを確認してください。",
        "investigate_service_failure" => "対象サービスと依存サービスを確認してください。",
        "investigate_application_failure" => {
            "障害アプリケーションとモジュールを確認し、更新または修復してください。"
        }
        _ => "イベントの詳細と同時刻の関連イベントを確認してください。",
    }
}

fn render_findings(html: &mut String, diagnosis: &Diagnosis) {
    html.push_str("<section><h2>検出事項</h2>");
    let mut found = false;
    for evaluation in diagnosis.evaluations.iter().filter(|item| {
        item.status == RuleEvaluationStatus::Triggered && item.category != "event_log"
    }) {
        found = true;
        let severity = evaluation.severity.unwrap_or(Severity::Information);
        write!(html, "<article class=\"finding {}\"><strong>{}</strong> <span class=\"badge {}\">{}</span><p>{}</p>", severity_class(Some(severity)), escape(&evaluation.rule_id), severity_class(Some(severity)), severity_text(severity), escape(&evaluation.summary)).unwrap();
        if !evaluation.evidence.is_empty() {
            html.push_str("<ul>");
            for evidence in &evaluation.evidence {
                let (name, value) = match evidence {
                    Evidence::Collected { path, value } => (path.as_str(), value),
                    Evidence::Derived { name, value, .. } => (name.as_str(), value),
                };
                write!(
                    html,
                    "<li><code>{}</code>: <code>{}</code></li>",
                    escape(name),
                    escape(&value.to_string())
                )
                .unwrap();
            }
            html.push_str("</ul>");
        }
        if let Some(recommendation) = &evaluation.recommendation {
            write!(
                html,
                "<p>推奨事項コード: <code>{}</code></p>",
                escape(&recommendation.code)
            )
            .unwrap();
        }
        html.push_str("</article>");
    }
    if !found {
        html.push_str(
            "<p class=\"passed\">イベントログ以外の診断規則による異常所見はありません。</p>",
        );
    }
    let counts = &diagnosis.summary.evaluations;
    write!(html, "<p class=\"muted\">評価: passed {} / triggered {} / not applicable {} / not evaluated {} / failed {}</p></section>", counts.passed, counts.triggered, counts.not_applicable, counts.not_evaluated, counts.failed).unwrap();
}

fn render_system(html: &mut String, data: &Collection) {
    let cpu_model = data
        .cpu
        .packages
        .as_ref()
        .and_then(|items| items.first())
        .and_then(|item| item.model.as_deref());
    html.push_str("<section><h2>システム概要</h2><table>");
    row(
        html,
        "Windows",
        &join_options(&[
            data.windows.edition.as_deref(),
            data.windows.version.as_deref(),
        ]),
    );
    row(html, "ビルド", &option_display(data.windows.build_number));
    row(html, "CPU", &text_or_unknown(cpu_model));
    row(
        html,
        "物理コア / 論理プロセッサ",
        &format!(
            "{} / {}",
            option_display(data.cpu.topology.physical_cores),
            option_display(data.cpu.topology.logical_processors)
        ),
    );
    row(
        html,
        "物理メモリ",
        &format_bytes(data.memory.physical.total_bytes),
    );
    row(
        html,
        "メモリ使用率",
        &data
            .memory
            .physical
            .load_percent
            .map(|v| format!("{v:.1}%"))
            .unwrap_or_else(unknown),
    );
    row(
        html,
        "ファームウェア",
        &join_options(&[
            data.firmware.vendor.as_deref(),
            data.firmware.version.as_deref(),
        ]),
    );
    row(
        html,
        "Secure Boot",
        &option_bool(data.firmware.secure_boot_enabled),
    );
    html.push_str("</table></section>");
}

fn render_gpu(html: &mut String, data: &Collection) {
    html.push_str("<section><h2>GPU</h2><table><tr><th>名称</th><th>種別</th><th>ドライバー</th><th>状態</th></tr>");
    if let Some(items) = &data.gpus {
        for gpu in items {
            write!(html, "<tr><td>{}</td><td>{:?}</td><td>{}</td><td>present: {} / started: {} / problem: {}</td></tr>", escape(&text_or_unknown(gpu.name.as_deref())), gpu.adapter_type, escape(&text_or_unknown(gpu.driver.version.as_deref())), option_bool(gpu.device_state.present), option_bool(gpu.device_state.started), option_display(gpu.device_state.problem_code)).unwrap();
        }
    } else {
        html.push_str("<tr><td colspan=\"4\">取得不能</td></tr>");
    }
    html.push_str("</table></section>");
}

fn render_storage(html: &mut String, data: &Collection) {
    html.push_str("<section><h2>ストレージ</h2><table><tr><th>ディスク</th><th>モデル</th><th>接続</th><th>容量</th><th>SMART</th></tr>");
    if let Some(disks) = &data.storage.disks {
        for disk in disks {
            let smart = data
                .storage
                .smart
                .as_ref()
                .and_then(|items| items.iter().find(|item| item.disk_number == disk.number));
            let smart_text = smart.map(format_smart).unwrap_or_else(|| "未取得".into());
            write!(
                html,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                disk.number,
                escape(&text_or_unknown(disk.model.as_deref())),
                disk.bus_type
                    .map(|v| format!("{v:?}"))
                    .unwrap_or_else(unknown),
                format_bytes(disk.capacity_bytes),
                escape(&smart_text)
            )
            .unwrap();
        }
    } else {
        html.push_str("<tr><td colspan=\"5\">取得不能</td></tr>");
    }
    html.push_str("</table></section>");
}

fn render_devices(html: &mut String, data: &Collection) {
    let Some(devices) = &data.devices else {
        html.push_str("<section><h2>接続デバイス</h2><p>取得不能</p></section>");
        return;
    };
    let present = devices
        .iter()
        .filter(|item| item.device_state.present == Some(true))
        .count();
    let past = devices
        .iter()
        .filter(|item| item.device_state.present == Some(false))
        .count();
    let problems = devices
        .iter()
        .filter(|item| {
            item.device_state.present == Some(true)
                && item.device_state.problem_code.unwrap_or(0) != 0
        })
        .count();
    write!(html, "<section><h2>接続デバイス</h2><div class=\"grid\"><div class=\"metric\"><span>現在接続</span><b>{present}</b></div><div class=\"metric\"><span>過去の接続記録</span><b>{past}</b></div><div class=\"metric\"><span>問題コードあり</span><b>{problems}</b></div></div></section>").unwrap();
}

fn render_collection_status(html: &mut String, status: &CollectionStatus) {
    html.push_str("<section><h2>情報収集状況</h2><table><tr><th>収集項目</th><th>状態</th><th>時間</th><th>補足</th></tr>");
    for collector in &status.collectors {
        let details = collection_details(collector);
        write!(
            html,
            "<tr><td>{:?}</td><td>{:?}</td><td>{} ms</td><td>{}</td></tr>",
            collector.name,
            collector.status,
            collector.duration_ms,
            escape(&details)
        )
        .unwrap();
    }
    html.push_str("</table></section>");
}

fn collection_details(collector: &pcdiag_core::CollectorResult) -> String {
    let mut counts = BTreeMap::<(String, String), usize>::new();
    for field in &collector.fields {
        *counts
            .entry((format!("{:?}", field.status), field.code.clone()))
            .or_default() += 1;
    }
    let mut details = collector
        .messages
        .iter()
        .map(|message| format!("message: {}", message.code))
        .collect::<Vec<_>>();
    details.extend(
        counts
            .into_iter()
            .map(|((status, code), count)| format!("{status}: {code} ({count}件)")),
    );
    details.join(" / ")
}

fn format_smart(smart: &DiskSmart) -> String {
    let mut values = vec![format!("プロトコル: {:?}", smart.protocol)];
    match smart.protocol {
        SmartProtocol::FailurePrediction => values.push(format!(
            "障害予測: {}",
            match smart.predict_failure {
                Some(true) => "あり",
                Some(false) => "なし",
                None => "取得不能",
            }
        )),
        SmartProtocol::Nvme => {
            values.push(format!(
                "重大警告: {}",
                smart
                    .critical_warning
                    .map(|value| if value == 0 {
                        "なし (0x00)".into()
                    } else {
                        format!("あり (0x{value:02x})")
                    })
                    .unwrap_or_else(unknown)
            ));
            values.push(format!(
                "予備領域: {}",
                format_percent(smart.available_spare_percent)
            ));
            values.push(format!("使用率: {}", format_percent(smart.percentage_used)));
            values.push(format!(
                "メディアエラー: {}",
                option_display(smart.media_errors)
            ));
            values.push(format!(
                "温度: {}",
                smart
                    .temperature_celsius
                    .map(|value| format!("{value} °C"))
                    .unwrap_or_else(unknown)
            ));
        }
        SmartProtocol::Unknown => {
            values.push(format!(
                "障害予測: {}",
                smart
                    .predict_failure
                    .map(|value| if value { "あり" } else { "なし" }.into())
                    .unwrap_or_else(unknown)
            ));
            if let Some(value) = smart.temperature_celsius {
                values.push(format!("温度: {value} °C"));
            }
        }
    }
    if smart.protocol == SmartProtocol::Nvme {
        values.push(format!(
            "稼働時間: {}",
            smart
                .power_on_hours
                .map(|value| format!("{value}時間"))
                .unwrap_or_else(unknown)
        ));
        values.push(format!(
            "安全でないシャットダウン: {}",
            option_display(smart.unsafe_shutdowns)
        ));
    }
    values.join(" / ")
}

fn format_percent(value: Option<u8>) -> String {
    value
        .map(|value| format!("{value}%"))
        .unwrap_or_else(unknown)
}

fn row(html: &mut String, label: &str, value: &str) {
    write!(
        html,
        "<tr><th>{}</th><td>{}</td></tr>",
        escape(label),
        escape(value)
    )
    .unwrap();
}

fn escape(value: &str) -> String {
    value.chars().fold(String::new(), |mut output, character| {
        output.push_str(match character {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '\"' => "&quot;",
            '\'' => "&#39;",
            _ => {
                output.push(character);
                return output;
            }
        });
        output
    })
}

fn severity_text(value: Severity) -> &'static str {
    match value {
        Severity::Critical => "重大",
        Severity::Error => "エラー",
        Severity::Warning => "警告",
        Severity::Information => "情報",
    }
}
fn severity_class(value: Option<Severity>) -> &'static str {
    match value {
        Some(Severity::Critical) => "critical",
        Some(Severity::Error) => "error",
        Some(Severity::Warning) => "warning",
        Some(Severity::Information) => "information",
        None => "passed",
    }
}
fn unknown() -> String {
    "取得不能".into()
}
fn text_or_unknown(value: Option<&str>) -> String {
    value.map(str::to_owned).unwrap_or_else(unknown)
}
fn option_display<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map(|v| v.to_string()).unwrap_or_else(unknown)
}
fn option_bool(value: Option<bool>) -> String {
    value
        .map(|v| if v { "はい" } else { "いいえ" }.into())
        .unwrap_or_else(unknown)
}
fn join_options(values: &[Option<&str>]) -> String {
    let value = values
        .iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    if value.is_empty() { unknown() } else { value }
}
fn format_bytes(value: Option<u64>) -> String {
    value
        .map(|bytes| format!("{:.2} GiB ({bytes} bytes)", bytes as f64 / 1_073_741_824.0))
        .unwrap_or_else(unknown)
}

#[derive(Debug)]
pub enum ReportError {
    Io(io::Error),
    Json(serde_json::Error),
    Bundle(bundle::BundleError),
    Artifact(pcdiag_core::ArtifactLoadError),
    Manifest(pcdiag_core::ManifestValidationErrors),
    AlreadyExists(PathBuf),
    IncompleteExists(PathBuf),
    ArtifactIdCollision,
}

impl std::fmt::Display for ReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "ファイル操作に失敗しました: {e}"),
            Self::Json(e) => write!(f, "JSON生成に失敗しました: {e}"),
            Self::Bundle(e) => write!(f, "実行環境情報を取得できませんでした: {e}"),
            Self::Artifact(e) => write!(f, "入力成果物が不正です: {e}"),
            Self::Manifest(e) => write!(f, "レポートマニフェストが不正です: {e}"),
            Self::AlreadyExists(p) => write!(
                f,
                "レポート成果物は既に存在します。上書きしません: {}",
                p.display()
            ),
            Self::IncompleteExists(p) => write!(
                f,
                "未完了のレポートディレクトリが存在します: {}",
                p.display()
            ),
            Self::ArtifactIdCollision => {
                f.write_str("一意なレポート成果物IDを生成できませんでした")
            }
        }
    }
}
impl std::error::Error for ReportError {}
impl From<io::Error> for ReportError {
    fn from(v: io::Error) -> Self {
        Self::Io(v)
    }
}
impl From<serde_json::Error> for ReportError {
    fn from(v: serde_json::Error) -> Self {
        Self::Json(v)
    }
}
impl From<bundle::BundleError> for ReportError {
    fn from(v: bundle::BundleError) -> Self {
        Self::Bundle(v)
    }
}
impl From<pcdiag_core::ArtifactLoadError> for ReportError {
    fn from(v: pcdiag_core::ArtifactLoadError) -> Self {
        Self::Artifact(v)
    }
}
impl From<pcdiag_core::ManifestValidationErrors> for ReportError {
    fn from(v: pcdiag_core::ManifestValidationErrors) -> Self {
        Self::Manifest(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcdiag_core::{ArtifactFile, CollectionStatus, diagnose_collection};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn escapes_html_special_characters() {
        assert_eq!(
            escape("<script a='x'>&\""),
            "&lt;script a=&#39;x&#39;&gt;&amp;&quot;"
        );
    }

    #[test]
    fn formats_utc_windows_update_times_as_jst() {
        assert_eq!(
            format_jst("2026-07-30T23:44:53.0000000Z"),
            "2026-07-31 08:44:53.0000000 JST"
        );
        assert_eq!(
            format_jst("2026-12-31T18:00:00Z"),
            "2027-01-01 03:00:00 JST"
        );
        assert_eq!(
            format_jst("2028-02-29T20:00:00Z"),
            "2028-03-01 05:00:00 JST"
        );
        assert_eq!(format_jst("invalid"), "invalid");
    }

    #[test]
    fn formats_failure_prediction_smart_without_ambiguous_labels() {
        let smart = DiskSmart {
            disk_number: 0,
            protocol: SmartProtocol::FailurePrediction,
            predict_failure: Some(false),
            critical_warning: None,
            temperature_celsius: None,
            available_spare_percent: None,
            percentage_used: None,
            power_on_hours: None,
            unsafe_shutdowns: None,
            media_errors: None,
        };
        assert_eq!(
            format_smart(&smart),
            "プロトコル: FailurePrediction / 障害予測: なし"
        );
    }

    #[test]
    fn formats_nvme_smart_as_named_metrics() {
        let smart = DiskSmart {
            disk_number: 1,
            protocol: SmartProtocol::Nvme,
            predict_failure: None,
            critical_warning: Some(0),
            temperature_celsius: Some(30),
            available_spare_percent: Some(100),
            percentage_used: Some(30),
            power_on_hours: Some(11_212),
            unsafe_shutdowns: Some(414),
            media_errors: Some(0),
        };
        let text = format_smart(&smart);
        assert!(text.contains("重大警告: なし (0x00)"));
        assert!(text.contains("予備領域: 100%"));
        assert!(text.contains("使用率: 30%"));
        assert!(text.contains("温度: 30 °C"));
    }

    #[test]
    fn renders_artifact_handling_notice_in_footer() {
        let (collection, diagnosis) = loaded_inputs();
        let html = render_html(&collection, &diagnosis);

        assert!(html.contains("<footer class=\"artifact-notice\">"));
        assert!(html.contains("成果物の取り扱いに関する注意"));
        assert!(html.contains("pcdiagは成果物を自動削除しません。"));
        assert!(html.contains(".artifact-notice{"));
        assert!(html.contains("section,.artifact-notice{break-inside:avoid"));
    }

    #[test]
    fn renders_event_details_and_escapes_sensitive_text() {
        let (mut collection, mut diagnosis) = loaded_inputs();
        collection.collection.event_logs.application = Some(vec![pcdiag_core::EventLogEntry {
            occurred_at: "2026-07-30T12:00:00Z".into(),
            log_name: "Application".into(),
            provider: "Application Error".into(),
            event_id: 1000,
            level: pcdiag_core::EventLogLevel::Error,
            summary: "<script>alert('x')</script>".into(),
        }]);
        diagnosis.diagnosis = diagnose_collection(&collection.collection);
        let html = render_html(&collection, &diagnosis);

        assert!(html.contains("Windowsイベントログ"));
        assert!(html.contains("<details open>"));
        assert!(html.contains("Windowsイベントログ（検出 1件）"));
        assert!(html.contains("Application: 1件"));
        assert!(html.contains("2026-07-30T12:00:00Z"));
        assert!(html.contains("Application"));
        assert!(html.contains("障害アプリケーションとモジュール"));
        assert!(html.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert"));
    }

    #[test]
    fn distinguishes_unavailable_event_logs_from_no_findings() {
        let (mut collection, mut diagnosis) = loaded_inputs();
        collection.collection.event_logs.system = None;
        collection.collection.event_logs.application = None;
        collection.collection.event_logs.security = None;
        diagnosis.diagnosis = diagnose_collection(&collection.collection);
        let html = render_html(&collection, &diagnosis);

        assert_eq!(html.matches("未取得").count(), 3);
        assert!(!html.contains("取得できた範囲に高優先度イベントはありません。"));
    }

    #[test]
    fn renders_windows_updates_in_accordions_with_requested_limits() {
        let (mut collection, diagnosis) = loaded_inputs();
        let mut history = (0..12)
            .map(|index| {
                update_entry(
                    &format!("2026-07-{:02}T12:00:00Z", index + 1),
                    &format!("success-{index}"),
                    WindowsUpdateResult::Succeeded,
                    0,
                )
            })
            .collect::<Vec<_>>();
        history.push(update_entry(
            "2026-07-30T12:00:00Z",
            "failed-1 <script>alert('x')</script>",
            WindowsUpdateResult::Failed,
            -2_145_124_330,
        ));
        history.push(update_entry(
            "2026-07-29T12:00:00Z",
            "failed-2",
            WindowsUpdateResult::Failed,
            -2_145_124_300,
        ));
        history.push(update_entry(
            "2026-07-28T12:00:00Z",
            "aborted-1",
            WindowsUpdateResult::Aborted,
            -2_145_124_341,
        ));
        collection.collection.windows_updates.history = Some(history);

        let html = render_html(&collection, &diagnosis);

        assert!(html.contains("<div class=\"accordion-stack\">"));
        assert!(html.contains("最近成功した更新</span>（10件）"));
        assert!(html.contains("失敗した更新</span>（2件）"));
        assert!(html.contains("中断された更新</span>（1件）"));
        assert!(html.contains("success-11"));
        assert!(html.contains("success-2"));
        assert!(!html.contains("success-1</td>"));
        assert!(!html.contains("success-0</td>"));
        assert!(html.contains("failed-1 &lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("-2145124330 (0x80240016)"));
        assert!(html.contains("aborted-1"));
        assert!(html.contains("2026-07-30 21:00:00 JST"));
        assert!(!html.contains("2026-07-30T12:00:00Z"));
    }

    #[test]
    fn writes_report_manifest_and_refuses_to_overwrite() {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("pcdiag-report-test-{}-{id}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir(&root).unwrap();
        let (collection, diagnosis) = loaded_inputs();
        let html = render_html(&collection, &diagnosis);

        let directory = write_report(
            &root,
            collection.clone(),
            diagnosis.clone(),
            html.as_bytes(),
            report_timing("43d39e67-c8f1-4c9b-a20e-a65dbba20295"),
        )
        .unwrap();

        assert_eq!(directory, root.join("report"));
        assert_eq!(
            fs::read_to_string(directory.join("report.html")).unwrap(),
            html
        );
        let manifest: ArtifactManifest =
            serde_json::from_slice(&fs::read(directory.join("manifest.json")).unwrap()).unwrap();
        manifest.validate().unwrap();
        assert_eq!(manifest.artifact_type, ArtifactType::Report);
        assert_eq!(manifest.inputs.len(), 2);
        assert_eq!(manifest.files[0].media_type, "text/html; charset=utf-8");

        let error = write_report(
            &root,
            collection,
            diagnosis,
            html.as_bytes(),
            report_timing("55bc50ea-219a-480e-84aa-486c5ce07336"),
        )
        .unwrap_err();
        assert!(matches!(error, ReportError::AlreadyExists(_)));
        fs::remove_dir_all(root).unwrap();
    }

    fn loaded_inputs() -> (LoadedCollectionArtifact, LoadedDiagnosisArtifact) {
        let collection: Collection = serde_json::from_str(include_str!(
            "../../pcdiag-core/tests/fixtures/memory-success-collection.json"
        ))
        .unwrap();
        let status: CollectionStatus = serde_json::from_str(include_str!(
            "../../pcdiag-core/tests/fixtures/memory-success-status.json"
        ))
        .unwrap();
        let diagnosis_value = diagnose_collection(&collection);
        let collection_manifest = ArtifactManifest {
            manifest_schema_version: "1.0".into(),
            artifact_schema_version: "2.0".into(),
            session_id: "a3f17c92-d604-4be8-9ea7-6ab7b92e41c5".into(),
            artifact_id: "831d1074-1145-4a66-bfa2-169903866adb".into(),
            artifact_type: ArtifactType::Collection,
            status: ArtifactStatus::Complete,
            started_at: "2026-07-18T01:00:00.000Z".into(),
            completed_at: "2026-07-18T01:00:01.000Z".into(),
            observed_utc_offset_minutes: 540,
            duration_ms: 1_000,
            tool: ToolInfo {
                name: "pcdiag".into(),
                version: "0.1.0".into(),
            },
            inputs: vec![],
            files: vec![test_file("collection.json"), test_file("status.json")],
        };
        let diagnosis_manifest = ArtifactManifest {
            manifest_schema_version: "1.0".into(),
            artifact_schema_version: "2.0".into(),
            session_id: collection_manifest.session_id.clone(),
            artifact_id: "211444ae-9a5c-4bf7-9349-80af85af3c04".into(),
            artifact_type: ArtifactType::Diagnosis,
            status: ArtifactStatus::Complete,
            started_at: "2026-07-18T01:01:00.000Z".into(),
            completed_at: "2026-07-18T01:01:01.000Z".into(),
            observed_utc_offset_minutes: 540,
            duration_ms: 1_000,
            tool: ToolInfo {
                name: "pcdiag".into(),
                version: "0.1.0".into(),
            },
            inputs: vec![ArtifactInput {
                artifact_id: collection_manifest.artifact_id.clone(),
                artifact_type: ArtifactType::Collection,
            }],
            files: vec![test_file("diagnosis.json")],
        };
        (
            LoadedCollectionArtifact {
                manifest: collection_manifest,
                collection,
                status,
            },
            LoadedDiagnosisArtifact {
                manifest: diagnosis_manifest,
                diagnosis: diagnosis_value,
            },
        )
    }

    fn test_file(path: &str) -> ArtifactFile {
        ArtifactFile {
            path: path.into(),
            media_type: "application/json".into(),
            size_bytes: 1,
            sha256: "0".repeat(64),
        }
    }

    fn update_entry(
        occurred_at: &str,
        title: &str,
        result: WindowsUpdateResult,
        hresult: i64,
    ) -> WindowsUpdateHistoryEntry {
        WindowsUpdateHistoryEntry {
            occurred_at: occurred_at.into(),
            title: Some(title.into()),
            kb_ids: vec!["KB5060001".into()],
            operation: pcdiag_core::WindowsUpdateOperation::Installation,
            operation_code: 1,
            result,
            result_code: match result {
                WindowsUpdateResult::Succeeded => 2,
                WindowsUpdateResult::Failed => 4,
                WindowsUpdateResult::Aborted => 5,
                _ => 99,
            },
            hresult,
            update_id: Some(format!("id-{title}")),
            revision_number: Some(1),
            support_url: None,
            client_application_id: Some("UpdateOrchestrator".into()),
        }
    }

    fn report_timing(artifact_id: &str) -> ReportTiming {
        ReportTiming {
            artifact_id: artifact_id.into(),
            started_at: "2026-07-18T01:02:00.000Z".into(),
            completed_at: "2026-07-18T01:02:01.000Z".into(),
            observed_utc_offset_minutes: 540,
            duration_ms: 1_000,
        }
    }
}
