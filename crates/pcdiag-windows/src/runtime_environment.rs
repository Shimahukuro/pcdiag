use std::time::Instant;

use pcdiag_core::{
    CollectionMessage, CollectorName, CollectorResult, CollectorStatus, FieldCollectionResult,
    FieldCollectionStatus, RuntimeEnvironmentCollection,
};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeEnvironmentCollectionResult {
    pub collection: RuntimeEnvironmentCollection,
    pub status: CollectorResult,
}

#[derive(Debug, Deserialize)]
struct Response {
    collection: RuntimeEnvironmentCollection,
    errors: Vec<CategoryError>,
}

#[derive(Debug, Deserialize)]
struct CategoryError {
    path: String,
    code: String,
    permission_denied: bool,
}

pub fn collect_runtime_environment() -> RuntimeEnvironmentCollectionResult {
    let started = Instant::now();
    match platform::query() {
        Ok(response) => {
            let partial = !response.errors.is_empty()
                || [
                    response.collection.services.truncated,
                    response.collection.startup_applications.truncated,
                    response.collection.installed_applications.truncated,
                    response.collection.running_processes.truncated,
                    response.collection.scheduled_tasks.truncated,
                ]
                .into_iter()
                .any(|value| value);
            let fields: Vec<FieldCollectionResult> = response
                .errors
                .iter()
                .map(|error| FieldCollectionResult {
                    path: error.path.clone(),
                    status: if error.permission_denied {
                        FieldCollectionStatus::PermissionDenied
                    } else {
                        FieldCollectionStatus::Failed
                    },
                    code: error.code.clone(),
                    native_code: None,
                })
                .collect();
            let mut messages = response
                .errors
                .into_iter()
                .map(|error| CollectionMessage {
                    code: error.code,
                    native_code: None,
                    message: None,
                })
                .collect::<Vec<_>>();
            if partial && fields.is_empty() {
                messages.push(CollectionMessage {
                    code: "runtime_environment_truncated".into(),
                    native_code: None,
                    message: Some(
                        "大量の項目を実用的な成果物サイズに収めるため一部を省略しました".into(),
                    ),
                });
            }
            RuntimeEnvironmentCollectionResult {
                collection: response.collection,
                status: CollectorResult {
                    name: CollectorName::RuntimeEnvironment,
                    status: if partial {
                        CollectorStatus::Partial
                    } else {
                        CollectorStatus::Success
                    },
                    duration_ms: elapsed_ms(started),
                    messages,
                    fields,
                },
            }
        }
        Err(failure) => RuntimeEnvironmentCollectionResult {
            collection: Default::default(),
            status: CollectorResult {
                name: CollectorName::RuntimeEnvironment,
                status: CollectorStatus::Failed,
                duration_ms: elapsed_ms(started),
                messages: vec![CollectionMessage {
                    code: failure.code.into(),
                    native_code: failure.native_code,
                    message: Some(failure.message.into()),
                }],
                fields: vec![FieldCollectionResult {
                    path: "/runtime_environment".into(),
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

#[derive(Debug)]
struct Failure {
    code: &'static str,
    native_code: Option<i64>,
    message: &'static str,
    status: FieldCollectionStatus,
}

#[cfg(any(windows, test))]
fn parse_response(json: &[u8]) -> Result<Response, serde_json::Error> {
    serde_json::from_slice(json)
}

#[cfg(any(windows, test))]
const SCRIPT: &str = include_str!("runtime_environment.ps1");

#[cfg(windows)]
mod platform {
    use super::*;
    use std::process::Command;
    pub(super) fn query() -> Result<Response, Failure> {
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                SCRIPT,
            ])
            .output()
            .map_err(|error| Failure {
                code: "runtime_environment_process_failed",
                native_code: error.raw_os_error().map(i64::from),
                message: "Windows PowerShellを開始できませんでした",
                status: FieldCollectionStatus::Failed,
            })?;
        if !output.status.success() {
            return Err(Failure {
                code: "runtime_environment_query_failed",
                native_code: output.status.code().map(i64::from),
                message: "常駐・自動実行環境を取得できませんでした",
                status: FieldCollectionStatus::Failed,
            });
        }
        parse_response(&output.stdout).map_err(|_| Failure {
            code: "runtime_environment_invalid_output",
            native_code: None,
            message: "常駐・自動実行環境の応答を解析できませんでした",
            status: FieldCollectionStatus::InvalidValue,
        })
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    pub(super) fn query() -> Result<Response, Failure> {
        Err(Failure {
            code: "runtime_environment_unsupported_platform",
            native_code: None,
            message: "Windows以外では常駐・自動実行環境を収集できません",
            status: FieldCollectionStatus::Unsupported,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_empty_categories_distinct_from_unavailable() {
        let response = parse_response(br#"{"collection":{"services":{"items":[],"truncated":false},"startup_applications":{"items":null,"truncated":false},"installed_applications":{"items":[],"truncated":false},"running_processes":{"items":[],"truncated":false},"scheduled_tasks":{"items":[],"truncated":false}},"errors":[{"path":"/runtime_environment/startup_applications/items","code":"startup_query_failed","permission_denied":true}]}"#).unwrap();
        assert_eq!(response.collection.services.items, Some(vec![]));
        assert!(response.collection.startup_applications.items.is_none());
        assert!(response.errors[0].permission_denied);
    }
    #[test]
    fn script_uses_read_only_sources_and_redacts_secret_arguments() {
        assert!(SCRIPT.contains("Win32_Service"));
        assert!(SCRIPT.contains("Win32_StartupCommand"));
        assert!(SCRIPT.contains("Get-ScheduledTask"));
        assert!(SCRIPT.contains("Get-AppxPackage"));
        assert!(SCRIPT.contains("Redact-Command"));
        assert!(!SCRIPT.contains("Win32_Product"));
    }
}
