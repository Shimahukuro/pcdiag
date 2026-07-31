use std::time::Instant;

use pcdiag_core::{
    CollectionMessage, CollectorName, CollectorResult, CollectorStatus, FieldCollectionResult,
    FieldCollectionStatus, WindowsUpdateCollection, WindowsUpdateHistoryEntry,
    WindowsUpdateOperation, WindowsUpdateResult,
};
use serde::Deserialize;

const HISTORY_PATH: &str = "/windows_updates/history";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsUpdateCollectionOptions {
    pub lookback_days: Option<u32>,
    pub max_entries: Option<u32>,
}

impl Default for WindowsUpdateCollectionOptions {
    fn default() -> Self {
        Self {
            lookback_days: Some(180),
            max_entries: Some(1_000),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsUpdateCollectionResult {
    pub collection: WindowsUpdateCollection,
    pub status: CollectorResult,
}

pub fn collect_windows_updates(
    options: WindowsUpdateCollectionOptions,
) -> WindowsUpdateCollectionResult {
    let started = Instant::now();
    match platform::query(options) {
        Ok(response) => {
            let mut messages = Vec::new();
            if response.truncated_by_date {
                messages.push(CollectionMessage {
                    code: "windows_update_history_truncated_by_date".into(),
                    native_code: None,
                    message: Some(
                        "指定された取得期間より古いWindows Update履歴を省略しました".into(),
                    ),
                });
            }
            if response.truncated_by_count {
                messages.push(CollectionMessage {
                    code: "windows_update_history_truncated_by_count".into(),
                    native_code: None,
                    message: Some(
                        "指定された最大件数を超えるWindows Update履歴を省略しました".into(),
                    ),
                });
            }
            WindowsUpdateCollectionResult {
                collection: WindowsUpdateCollection {
                    lookback_days: options.lookback_days,
                    max_entries: options.max_entries,
                    history: Some(response.items.into_iter().map(Into::into).collect()),
                },
                status: CollectorResult {
                    name: CollectorName::WindowsUpdates,
                    status: CollectorStatus::Success,
                    duration_ms: elapsed_ms(started),
                    messages,
                    fields: vec![],
                },
            }
        }
        Err(failure) => WindowsUpdateCollectionResult {
            collection: WindowsUpdateCollection {
                lookback_days: options.lookback_days,
                max_entries: options.max_entries,
                history: None,
            },
            status: CollectorResult {
                name: CollectorName::WindowsUpdates,
                status: CollectorStatus::Failed,
                duration_ms: elapsed_ms(started),
                messages: vec![CollectionMessage {
                    code: failure.code.into(),
                    native_code: failure.native_code,
                    message: Some(failure.message.into()),
                }],
                fields: vec![FieldCollectionResult {
                    path: HISTORY_PATH.into(),
                    status: failure.status,
                    code: failure.code.into(),
                    native_code: failure.native_code,
                }],
            },
        },
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct HistoryResponse {
    items: Vec<PowerShellHistoryEntry>,
    truncated_by_date: bool,
    truncated_by_count: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PowerShellHistoryEntry {
    occurred_at: String,
    title: Option<String>,
    kb_ids: Vec<String>,
    operation_code: i32,
    result_code: i32,
    #[serde(rename = "HResult")]
    hresult: i64,
    update_id: Option<String>,
    revision_number: Option<i32>,
    support_url: Option<String>,
    client_application_id: Option<String>,
}

impl From<PowerShellHistoryEntry> for WindowsUpdateHistoryEntry {
    fn from(entry: PowerShellHistoryEntry) -> Self {
        Self {
            occurred_at: entry.occurred_at,
            title: entry.title,
            kb_ids: entry.kb_ids,
            operation: operation(entry.operation_code),
            operation_code: entry.operation_code,
            result: result(entry.result_code),
            result_code: entry.result_code,
            hresult: entry.hresult,
            update_id: entry.update_id,
            revision_number: entry.revision_number,
            support_url: entry.support_url,
            client_application_id: entry.client_application_id,
        }
    }
}

fn operation(value: i32) -> WindowsUpdateOperation {
    match value {
        1 => WindowsUpdateOperation::Installation,
        2 => WindowsUpdateOperation::Uninstallation,
        _ => WindowsUpdateOperation::Unknown,
    }
}

fn result(value: i32) -> WindowsUpdateResult {
    match value {
        0 => WindowsUpdateResult::NotStarted,
        1 => WindowsUpdateResult::InProgress,
        2 => WindowsUpdateResult::Succeeded,
        3 => WindowsUpdateResult::SucceededWithErrors,
        4 => WindowsUpdateResult::Failed,
        5 => WindowsUpdateResult::Aborted,
        _ => WindowsUpdateResult::Unknown,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CollectionFailure {
    code: &'static str,
    native_code: Option<i64>,
    message: &'static str,
    status: FieldCollectionStatus,
}

#[cfg(any(windows, test))]
fn parse_history(json: &[u8]) -> Result<HistoryResponse, serde_json::Error> {
    serde_json::from_slice(json)
}

#[cfg(any(windows, test))]
fn powershell_script(options: WindowsUpdateCollectionOptions) -> String {
    let lookback = options
        .lookback_days
        .map_or_else(|| "$null".into(), |value| value.to_string());
    let maximum = options
        .max_entries
        .map_or_else(|| "$null".into(), |value| value.to_string());
    SCRIPT
        .replace("__LOOKBACK_DAYS__", &lookback)
        .replace("__MAX_ENTRIES__", &maximum)
}

#[cfg(any(windows, test))]
const SCRIPT: &str = r#"
$ErrorActionPreference='Stop'
$lookbackDays=__LOOKBACK_DAYS__
$maxEntries=__MAX_ENTRIES__
$cutoff=if ($null -eq $lookbackDays) {$null} else {[DateTime]::UtcNow.AddDays(-$lookbackDays)}
$session=New-Object -ComObject Microsoft.Update.Session
$searcher=$session.CreateUpdateSearcher()
$total=[int]$searcher.GetTotalHistoryCount()
$items=[System.Collections.Generic.List[object]]::new()
$offset=0
$truncatedByDate=$false
$truncatedByCount=$false
while ($offset -lt $total) {
  if (($null -ne $maxEntries) -and ($items.Count -ge $maxEntries)) {
    $truncatedByCount=$true
    break
  }
  $remaining=$total-$offset
  $take=[Math]::Min(100,$remaining)
  if ($null -ne $maxEntries) {$take=[Math]::Min($take,$maxEntries-$items.Count)}
  if ($take -le 0) {$truncatedByCount=$true; break}
  $batch=$searcher.QueryHistory($offset,$take)
  for ($index=0; $index -lt $batch.Count; $index++) {
    $entry=$batch.Item($index)
    $date=$entry.Date.ToUniversalTime()
    if (($null -ne $cutoff) -and ($date -lt $cutoff)) {
      $truncatedByDate=$true
      break
    }
    $title=if ([string]::IsNullOrWhiteSpace($entry.Title)) {$null} else {$entry.Title}
    $kbIds=@()
    if ($null -ne $title) {
      $kbIds=@([regex]::Matches($title,'(?i)KB\d{6,8}') | ForEach-Object {$_.Value.ToUpperInvariant()} | Select-Object -Unique)
    }
    $identity=$entry.UpdateIdentity
    $items.Add([pscustomobject]@{
      OccurredAt=$date.ToString('o')
      Title=$title
      KbIds=$kbIds
      OperationCode=[int]$entry.Operation
      ResultCode=[int]$entry.ResultCode
      HResult=[int64]$entry.HResult
      UpdateId=if ($null -eq $identity) {$null} else {$identity.UpdateID}
      RevisionNumber=if ($null -eq $identity) {$null} else {[int]$identity.RevisionNumber}
      SupportUrl=if ([string]::IsNullOrWhiteSpace($entry.SupportUrl)) {$null} else {$entry.SupportUrl}
      ClientApplicationId=if ([string]::IsNullOrWhiteSpace($entry.ClientApplicationID)) {$null} else {$entry.ClientApplicationID}
    })
  }
  if ($truncatedByDate) {break}
  $offset+=$batch.Count
  if ($batch.Count -eq 0) {break}
}
if (($null -ne $maxEntries) -and ($items.Count -ge $maxEntries) -and ($offset -lt $total)) {$truncatedByCount=$true}
$response=[pscustomobject]@{Items=@($items);TruncatedByDate=$truncatedByDate;TruncatedByCount=$truncatedByCount}
$json=ConvertTo-Json -InputObject $response -Depth 5 -Compress
$bytes=[System.Text.UTF8Encoding]::new($false).GetBytes($json)
$stdout=[Console]::OpenStandardOutput()
$stdout.Write($bytes,0,$bytes.Length)
"#;

#[cfg(windows)]
mod platform {
    use std::process::Command;

    use super::{
        CollectionFailure, FieldCollectionStatus, HistoryResponse, WindowsUpdateCollectionOptions,
        parse_history, powershell_script,
    };

    pub(super) fn query(
        options: WindowsUpdateCollectionOptions,
    ) -> Result<HistoryResponse, CollectionFailure> {
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &powershell_script(options),
            ])
            .output()
            .map_err(|error| CollectionFailure {
                code: "windows_update_process_failed",
                native_code: error.raw_os_error().map(i64::from),
                message: "Windows PowerShellを開始できませんでした",
                status: FieldCollectionStatus::Failed,
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let permission =
                stderr.contains("Access is denied") || stderr.contains("アクセスが拒否");
            return Err(CollectionFailure {
                code: if permission {
                    "windows_update_permission_denied"
                } else {
                    "windows_update_query_failed"
                },
                native_code: output.status.code().map(i64::from),
                message: if permission {
                    "Windows Update履歴を読み取る権限がありません"
                } else {
                    "Windows Update Agentから更新履歴を取得できませんでした"
                },
                status: if permission {
                    FieldCollectionStatus::PermissionDenied
                } else {
                    FieldCollectionStatus::Failed
                },
            });
        }
        parse_history(&output.stdout).map_err(|_| CollectionFailure {
            code: "windows_update_invalid_output",
            native_code: None,
            message: "Windows Update Agentの応答をJSONとして解析できませんでした",
            status: FieldCollectionStatus::InvalidValue,
        })
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{
        CollectionFailure, FieldCollectionStatus, HistoryResponse, WindowsUpdateCollectionOptions,
    };

    pub(super) fn query(
        _options: WindowsUpdateCollectionOptions,
    ) -> Result<HistoryResponse, CollectionFailure> {
        Err(CollectionFailure {
            code: "windows_update_unsupported_platform",
            native_code: None,
            message: "Windows以外ではWindows Update履歴を収集できません",
            status: FieldCollectionStatus::Unsupported,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_history_and_maps_known_and_unknown_values() {
        let response = parse_history(br#"{"Items":[{"OccurredAt":"2026-07-01T00:00:00.0000000Z","Title":"Security Update (KB5060001)","KbIds":["KB5060001"],"OperationCode":1,"ResultCode":2,"HResult":0,"UpdateId":"id","RevisionNumber":1,"SupportUrl":null,"ClientApplicationId":"UpdateOrchestrator"},{"OccurredAt":"2026-06-01T00:00:00Z","Title":null,"KbIds":[],"OperationCode":99,"ResultCode":99,"HResult":-1,"UpdateId":null,"RevisionNumber":null,"SupportUrl":null,"ClientApplicationId":null}],"TruncatedByDate":false,"TruncatedByCount":true}"#).unwrap();
        let items: Vec<WindowsUpdateHistoryEntry> =
            response.items.into_iter().map(Into::into).collect();
        assert_eq!(items[0].operation, WindowsUpdateOperation::Installation);
        assert_eq!(items[0].result, WindowsUpdateResult::Succeeded);
        assert_eq!(items[1].operation, WindowsUpdateOperation::Unknown);
        assert_eq!(items[1].result, WindowsUpdateResult::Unknown);
        assert!(response.truncated_by_count);
    }

    #[test]
    fn script_uses_defaults_and_all_history_values() {
        let default_script = powershell_script(WindowsUpdateCollectionOptions::default());
        assert!(default_script.contains("$lookbackDays=180"));
        assert!(default_script.contains("$maxEntries=1000"));
        assert!(default_script.contains("QueryHistory($offset,$take)"));

        let all_script = powershell_script(WindowsUpdateCollectionOptions {
            lookback_days: None,
            max_entries: None,
        });
        assert!(all_script.contains("$lookbackDays=$null"));
        assert!(all_script.contains("$maxEntries=$null"));
    }
}
