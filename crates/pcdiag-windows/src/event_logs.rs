use std::time::Instant;

#[cfg(any(windows, test))]
use pcdiag_core::EventLogLevel;
use pcdiag_core::{
    CollectionMessage, CollectorName, CollectorResult, CollectorStatus, EventLogCollection,
    EventLogEntry, FieldCollectionResult, FieldCollectionStatus,
};
#[cfg(any(windows, test))]
use serde::Deserialize;

const LOGS: [(&str, &str); 3] = [
    ("System", "/event_logs/system"),
    ("Application", "/event_logs/application"),
    ("Security", "/event_logs/security"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventLogCollectionResult {
    pub collection: EventLogCollection,
    pub status: CollectorResult,
}

pub fn collect_event_logs(lookback_days: u32) -> EventLogCollectionResult {
    let started = Instant::now();
    let mut messages = Vec::new();
    let mut fields = Vec::new();
    let mut values = Vec::new();
    for (log, path) in LOGS {
        match platform::query(log, lookback_days) {
            Ok(events) => values.push(Some(events)),
            Err(failure) => {
                messages.push(CollectionMessage {
                    code: failure.code.into(),
                    native_code: failure.native_code,
                    message: Some(format!("{log}: {}", failure.message)),
                });
                fields.push(FieldCollectionResult {
                    path: path.into(),
                    status: failure.status,
                    code: failure.code.into(),
                    native_code: failure.native_code,
                });
                values.push(None);
            }
        }
    }
    let failed = fields.len();
    EventLogCollectionResult {
        collection: EventLogCollection {
            lookback_days,
            system: values.remove(0),
            application: values.remove(0),
            security: values.remove(0),
        },
        status: CollectorResult {
            name: CollectorName::EventLogs,
            status: match failed {
                0 => CollectorStatus::Success,
                3 => CollectorStatus::Failed,
                _ => CollectorStatus::Partial,
            },
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            messages,
            fields,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CollectionFailure {
    code: &'static str,
    native_code: Option<i64>,
    message: String,
    status: FieldCollectionStatus,
}

#[cfg(any(windows, test))]
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PowerShellEvent {
    time_created: String,
    log_name: String,
    provider_name: String,
    id: u32,
    level: u8,
    message: Option<String>,
}

#[cfg(any(windows, test))]
fn parse_events(json: &str) -> Result<Vec<EventLogEntry>, String> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let events: Vec<PowerShellEvent> =
        serde_json::from_str(json).map_err(|error| error.to_string())?;
    Ok(events
        .into_iter()
        .map(|event| {
            let fallback = format!("{} event {}", event.provider_name, event.id);
            EventLogEntry {
                occurred_at: event.time_created,
                log_name: event.log_name,
                provider: event.provider_name,
                event_id: event.id,
                level: match event.level {
                    1 => EventLogLevel::Critical,
                    2 => EventLogLevel::Error,
                    3 => EventLogLevel::Warning,
                    _ => EventLogLevel::Information,
                },
                summary: event.message.unwrap_or(fallback),
            }
        })
        .collect())
}

#[cfg(windows)]
mod platform {
    use std::process::Command;

    use super::{CollectionFailure, EventLogEntry, FieldCollectionStatus, parse_events};

    pub(super) fn query(
        log: &str,
        lookback_days: u32,
    ) -> Result<Vec<EventLogEntry>, CollectionFailure> {
        let filter = if log == "Security" {
            format!("@{{LogName='{log}'; StartTime=$start; Id=1102,4625,4719}}")
        } else {
            format!("@{{LogName='{log}'; StartTime=$start; Level=1,2,3}}")
        };
        let script = format!(
            "$ErrorActionPreference='Stop'; $start=(Get-Date).AddDays(-{lookback_days}); \
             try {{ $events=Get-WinEvent -FilterHashtable {filter} -MaxEvents 1000 }} \
             catch {{ if ($_.FullyQualifiedErrorId -like 'NoMatchingEventsFound*') {{$events=@()}} else {{throw}} }}; \
             $result=@($events | ForEach-Object {{ [pscustomobject]@{{TimeCreated=$_.TimeCreated.ToUniversalTime().ToString('o');LogName=$_.LogName;ProviderName=$_.ProviderName;Id=$_.Id;Level=[int]$_.Level;Message=$_.Message}} }}); \
             ConvertTo-Json -InputObject $result -Compress"
        );
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .map_err(|error| CollectionFailure {
                code: "event_log_process_failed",
                native_code: error.raw_os_error().map(i64::from),
                message: "Windows PowerShellを開始できませんでした".into(),
                status: FieldCollectionStatus::Failed,
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let permission =
                stderr.contains("Access is denied") || stderr.contains("アクセスが拒否");
            return Err(CollectionFailure {
                code: if permission {
                    "event_log_permission_denied"
                } else {
                    "event_log_query_failed"
                },
                native_code: output.status.code().map(i64::from),
                message: if permission {
                    "イベントログを読み取る権限がありません".into()
                } else {
                    "イベントログを取得できませんでした（サービス停止、監査無効、またはログ破損の可能性があります）".into()
                },
                status: if permission {
                    FieldCollectionStatus::PermissionDenied
                } else {
                    FieldCollectionStatus::Failed
                },
            });
        }
        parse_events(&String::from_utf8_lossy(&output.stdout)).map_err(|_| CollectionFailure {
            code: "event_log_invalid_output",
            native_code: None,
            message: "イベントログの応答を解析できませんでした（ログ破損の可能性があります）"
                .into(),
            status: FieldCollectionStatus::InvalidValue,
        })
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{CollectionFailure, EventLogEntry, FieldCollectionStatus};

    pub(super) fn query(
        _log: &str,
        _lookback_days: u32,
    ) -> Result<Vec<EventLogEntry>, CollectionFailure> {
        Err(CollectionFailure {
            code: "event_log_unsupported_platform",
            native_code: None,
            message: "Windows以外ではイベントログを収集できません".into(),
            status: FieldCollectionStatus::Unsupported,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_powershell_event_array() {
        let events = parse_events(r#"[{"TimeCreated":"2026-07-01T00:00:00.0000000Z","LogName":"System","ProviderName":"Disk","Id":7,"Level":2,"Message":"bad sector"}]"#).unwrap();
        assert_eq!(events[0].event_id, 7);
        assert_eq!(events[0].level, EventLogLevel::Error);
        assert_eq!(events[0].summary, "bad sector");
    }

    #[test]
    fn reports_unsupported_platform_without_panicking() {
        if cfg!(windows) {
            return;
        }
        let result = collect_event_logs(30);
        assert_eq!(result.status.status, CollectorStatus::Failed);
        assert_eq!(result.status.fields.len(), 3);
        assert!(result.collection.system.is_none());
    }
}
