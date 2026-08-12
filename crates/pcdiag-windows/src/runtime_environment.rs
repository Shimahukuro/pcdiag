use std::time::Instant;

#[cfg(any(windows, test))]
use pcdiag_core::{
    CategoryCollection, InstalledApplication, RunningProcess, ScheduledTask, Service,
    StartupApplication,
};
use pcdiag_core::{
    CollectionMessage, CollectorName, CollectorResult, CollectorStatus, FieldCollectionResult,
    FieldCollectionStatus, RuntimeEnvironmentCollection,
};
#[cfg(any(windows, test))]
use serde::{Deserialize, de::DeserializeOwned};
#[cfg(any(windows, test))]
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeEnvironmentCollectionResult {
    pub collection: RuntimeEnvironmentCollection,
    pub status: CollectorResult,
}

struct Response {
    collection: RuntimeEnvironmentCollection,
    errors: Vec<CategoryError>,
}

struct CategoryError {
    path: String,
    code: String,
    status: FieldCollectionStatus,
    message: Option<String>,
}

#[cfg(any(windows, test))]
#[derive(Debug, Deserialize)]
struct RawResponse {
    collection: Value,
    #[serde(default)]
    errors: Vec<ScriptError>,
}

#[cfg(any(windows, test))]
#[derive(Debug, Deserialize)]
struct ScriptError {
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
                    status: error.status,
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
                    message: error.message,
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
                    message: Some(failure.message),
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
    message: String,
    status: FieldCollectionStatus,
}

#[cfg(any(windows, test))]
fn parse_response(json: &[u8]) -> Result<Response, serde_json::Error> {
    let raw: RawResponse = serde_json::from_slice(json)?;
    let mut errors = raw
        .errors
        .into_iter()
        .map(|error| {
            let message = script_error_message(&error.code, error.permission_denied);
            CategoryError {
                path: error.path,
                code: error.code,
                status: if error.permission_denied {
                    FieldCollectionStatus::PermissionDenied
                } else {
                    FieldCollectionStatus::Failed
                },
                message,
            }
        })
        .collect::<Vec<_>>();
    let collection = RuntimeEnvironmentCollection {
        services: parse_category::<Service>(&raw.collection, "services", &mut errors),
        startup_applications: parse_category::<StartupApplication>(
            &raw.collection,
            "startup_applications",
            &mut errors,
        ),
        installed_applications: parse_category::<InstalledApplication>(
            &raw.collection,
            "installed_applications",
            &mut errors,
        ),
        running_processes: parse_category::<RunningProcess>(
            &raw.collection,
            "running_processes",
            &mut errors,
        ),
        scheduled_tasks: parse_category::<ScheduledTask>(
            &raw.collection,
            "scheduled_tasks",
            &mut errors,
        ),
    };
    Ok(Response { collection, errors })
}

#[cfg(any(windows, test))]
fn script_error_message(code: &str, permission_denied: bool) -> Option<String> {
    match code {
        "appx_query_failed" if permission_denied => Some(
            "全ユーザーのMicrosoft Storeアプリを列挙する権限がないため、取得できたデスクトップアプリのみを保存しました"
                .into(),
        ),
        _ => None,
    }
}

#[cfg(any(windows, test))]
fn parse_category<T: DeserializeOwned>(
    collection: &Value,
    name: &str,
    errors: &mut Vec<CategoryError>,
) -> CategoryCollection<T> {
    let base = format!("/runtime_environment/{name}");
    let Some(category) = collection.get(name).and_then(Value::as_object) else {
        errors.push(invalid_error(base, "runtime_environment_category_invalid"));
        return CategoryCollection::default();
    };
    let truncated = match category.get("truncated") {
        Some(Value::Bool(value)) => *value,
        None => false,
        Some(_) => {
            errors.push(invalid_error(
                format!("{base}/truncated"),
                "runtime_environment_truncated_invalid",
            ));
            false
        }
    };
    let items = match category.get("items") {
        Some(Value::Null) | None => None,
        Some(Value::Array(values)) => {
            let mut items = Vec::with_capacity(values.len());
            for (index, value) in values.iter().cloned().enumerate() {
                match serde_path_to_error::deserialize(value.clone()) {
                    Ok(item) => items.push(item),
                    Err(error) => {
                        let field_path = error.path().to_string();
                        let pointer_suffix = serde_path_to_pointer(&field_path);
                        let actual = json_type_at_path(&value, &field_path);
                        let expected = safe_expected_type(error.inner());
                        errors.push(CategoryError {
                            path: format!("{base}/items/{index}{pointer_suffix}"),
                            code: "runtime_environment_item_invalid".into(),
                            status: FieldCollectionStatus::InvalidValue,
                            message: Some(format!(
                                "項目のデータ型が不正です（フィールド: {field_path}, 実際: {actual}, 期待: {expected}）"
                            )),
                        });
                    }
                }
            }
            Some(items)
        }
        Some(_) => {
            errors.push(invalid_error(
                format!("{base}/items"),
                "runtime_environment_items_invalid",
            ));
            None
        }
    };
    CategoryCollection { items, truncated }
}

#[cfg(any(windows, test))]
fn invalid_error(path: String, code: &str) -> CategoryError {
    CategoryError {
        path,
        code: code.into(),
        status: FieldCollectionStatus::InvalidValue,
        message: None,
    }
}

#[cfg(any(windows, test))]
fn serde_path_to_pointer(path: &str) -> String {
    if path.is_empty() || path == "." {
        return String::new();
    }
    let mut pointer = String::new();
    let mut segment = String::new();
    for character in path.chars() {
        match character {
            '.' => {
                if !segment.is_empty() {
                    pointer.push('/');
                    pointer.push_str(&segment);
                    segment.clear();
                }
            }
            '[' | ']' => {
                if !segment.is_empty() {
                    pointer.push('/');
                    pointer.push_str(&segment);
                    segment.clear();
                }
            }
            _ => segment.push(character),
        }
    }
    if !segment.is_empty() {
        pointer.push('/');
        pointer.push_str(&segment);
    }
    pointer
}

#[cfg(any(windows, test))]
fn json_type_at_path(value: &Value, path: &str) -> &'static str {
    let mut current = value;
    let normalized = serde_path_to_pointer(path);
    for segment in normalized.split('/').filter(|segment| !segment.is_empty()) {
        current = match current {
            Value::Object(object) => match object.get(segment) {
                Some(value) => value,
                None => return "missing",
            },
            Value::Array(array) => match segment.parse::<usize>().ok().and_then(|i| array.get(i)) {
                Some(value) => value,
                None => return "missing",
            },
            _ => return json_type(current),
        };
    }
    json_type(current)
}

#[cfg(any(windows, test))]
fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_u64() => "unsigned integer",
        Value::Number(number) if number.is_i64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(any(windows, test))]
fn safe_expected_type(error: &serde_json::Error) -> String {
    let text = error.to_string();
    if text.starts_with("missing field") {
        return "required field".into();
    }
    text.rsplit_once(", expected ").map_or_else(
        || "schema-compatible value".into(),
        |(_, expected)| expected.into(),
    )
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
                message: "Windows PowerShellを開始できませんでした".into(),
                status: FieldCollectionStatus::Failed,
            })?;
        if !output.status.success() {
            return Err(Failure {
                code: "runtime_environment_query_failed",
                native_code: output.status.code().map(i64::from),
                message: "常駐・自動実行環境を取得できませんでした".into(),
                status: FieldCollectionStatus::Failed,
            });
        }
        parse_response(&output.stdout).map_err(|error| Failure {
            code: "runtime_environment_invalid_output",
            native_code: None,
            message: format!(
                "常駐・自動実行環境のJSON文書を解析できませんでした（{:?}, {}行{}列）",
                error.classify(),
                error.line(),
                error.column()
            ),
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
            message: "Windows以外では常駐・自動実行環境を収集できません".into(),
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
        assert_eq!(
            response.errors[0].status,
            FieldCollectionStatus::PermissionDenied
        );
    }

    #[test]
    fn preserves_valid_items_and_other_categories_when_one_item_is_invalid() {
        let response = parse_response(br#"{"collection":{"services":{"items":[{"name":"valid","display_name":null,"description":null,"state":"Running","start_mode":"Auto","command_line":null,"account":null,"process_id":10,"dependencies":[],"binary":{"executable_path":null,"publisher":null,"signature_status":null,"sha256":null,"file_exists":null}},{"name":"invalid","display_name":null,"description":null,"state":"Running","start_mode":"Auto","command_line":null,"account":null,"process_id":"not-a-number","dependencies":[],"binary":{"executable_path":null,"publisher":null,"signature_status":null,"sha256":null,"file_exists":null}}],"truncated":false},"startup_applications":{"items":[],"truncated":false},"installed_applications":{"items":[],"truncated":false},"running_processes":{"items":[],"truncated":false},"scheduled_tasks":{"items":[],"truncated":false}},"errors":[]}"#).unwrap();

        let services = response.collection.services.items.unwrap();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "valid");
        assert_eq!(response.collection.startup_applications.items, Some(vec![]));
        assert!(response.errors.iter().any(|error| {
            error.path == "/runtime_environment/services/items/1/process_id"
                && error.status == FieldCollectionStatus::InvalidValue
                && error.message.as_deref().is_some_and(|message| {
                    message.contains("フィールド: process_id")
                        && message.contains("実際: string")
                        && !message.contains("not-a-number")
                })
        }));
    }

    #[test]
    fn explains_appx_permission_denial_without_native_error_text() {
        let response = parse_response(br#"{"collection":{"services":{"items":[],"truncated":false},"startup_applications":{"items":[],"truncated":false},"installed_applications":{"items":[],"truncated":false},"running_processes":{"items":[],"truncated":false},"scheduled_tasks":{"items":[],"truncated":false}},"errors":[{"path":"/runtime_environment/installed_applications/items","code":"appx_query_failed","permission_denied":true}]}"#).unwrap();
        let error = &response.errors[0];
        assert_eq!(error.status, FieldCollectionStatus::PermissionDenied);
        assert_eq!(
            error.message.as_deref(),
            Some(
                "全ユーザーのMicrosoft Storeアプリを列挙する権限がないため、取得できたデスクトップアプリのみを保存しました"
            )
        );
    }

    #[test]
    fn script_uses_read_only_sources_and_redacts_secret_arguments() {
        assert!(SCRIPT.contains("Win32_Service"));
        assert!(SCRIPT.contains("Win32_StartupCommand"));
        assert!(SCRIPT.contains("Get-ScheduledTask"));
        assert!(SCRIPT.contains("Get-AppxPackage"));
        assert!(SCRIPT.contains("Redact-Command"));
        assert!(SCRIPT.contains("Where-Object {$null -ne $_"));
        assert!(!SCRIPT.contains("Win32_Product"));
    }
}
