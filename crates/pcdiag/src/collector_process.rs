use std::{
    ffi::OsString,
    io::Read,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use pcdiag_core::{
    Collection, CollectionMessage, CollectionStatus, CollectorName, CollectorResult,
    CollectorStatus,
};
use pcdiag_windows::{CompleteCollectionResult, WindowsUpdateCollectionOptions};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const MIN_TIMEOUT_SECONDS: u64 = 1;
const MAX_TIMEOUT_SECONDS: u64 = 3_600;

pub(crate) const COLLECTOR_ORDER: [CollectorName; 13] = [
    CollectorName::Windows,
    CollectorName::WindowsUpdates,
    CollectorName::Clock,
    CollectorName::Cpu,
    CollectorName::Firmware,
    CollectorName::Memory,
    CollectorName::Gpu,
    CollectorName::Devices,
    CollectorName::EventLogs,
    CollectorName::PhysicalDisks,
    CollectorName::Partitions,
    CollectorName::Volumes,
    CollectorName::Smart,
];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CollectorTimeouts {
    overrides: Vec<(CollectorName, Duration)>,
}

impl CollectorTimeouts {
    pub(crate) fn set_from_cli(&mut self, value: &OsString) -> Result<(), String> {
        let value = value
            .to_str()
            .ok_or_else(|| "--collector-timeoutにはUnicode文字列を指定してください".to_string())?;
        let (name, seconds) = value.split_once('=').ok_or_else(|| {
            "--collector-timeoutは<collector>=<秒>の形式で指定してください".to_string()
        })?;
        let name = parse_collector_name(name).ok_or_else(|| {
            format!(
                "未対応のコレクターです: {name}（利用可能: {}）",
                collector_names().join(", ")
            )
        })?;
        let seconds = seconds
            .parse::<u64>()
            .ok()
            .filter(|seconds| (MIN_TIMEOUT_SECONDS..=MAX_TIMEOUT_SECONDS).contains(seconds))
            .ok_or_else(|| {
                format!(
                    "--collector-timeoutの秒数には{MIN_TIMEOUT_SECONDS}から{MAX_TIMEOUT_SECONDS}の整数を指定してください"
                )
            })?;
        if self.overrides.iter().any(|(existing, _)| *existing == name) {
            return Err(format!(
                "--collector-timeoutで{}を複数回指定できません",
                collector_name(name)
            ));
        }
        self.overrides.push((name, Duration::from_secs(seconds)));
        Ok(())
    }

    pub(crate) fn timeout_for(&self, name: CollectorName) -> Duration {
        self.overrides
            .iter()
            .find_map(|(candidate, timeout)| (*candidate == name).then_some(*timeout))
            .unwrap_or_else(|| default_timeout(name))
    }
}

fn default_timeout(name: CollectorName) -> Duration {
    Duration::from_secs(match name {
        CollectorName::Windows
        | CollectorName::Clock
        | CollectorName::Cpu
        | CollectorName::Firmware
        | CollectorName::Memory => 10,
        CollectorName::EventLogs | CollectorName::WindowsUpdates => 120,
        CollectorName::Gpu
        | CollectorName::Devices
        | CollectorName::PhysicalDisks
        | CollectorName::Partitions
        | CollectorName::Volumes
        | CollectorName::Smart => 30,
    })
}

pub(crate) fn collector_name(name: CollectorName) -> &'static str {
    match name {
        CollectorName::Windows => "windows",
        CollectorName::WindowsUpdates => "windows_updates",
        CollectorName::Clock => "clock",
        CollectorName::Cpu => "cpu",
        CollectorName::Firmware => "firmware",
        CollectorName::Memory => "memory",
        CollectorName::Gpu => "gpu",
        CollectorName::Devices => "devices",
        CollectorName::EventLogs => "event_logs",
        CollectorName::PhysicalDisks => "physical_disks",
        CollectorName::Partitions => "partitions",
        CollectorName::Volumes => "volumes",
        CollectorName::Smart => "smart",
    }
}

pub(crate) fn parse_collector_name(value: &str) -> Option<CollectorName> {
    COLLECTOR_ORDER
        .into_iter()
        .find(|name| collector_name(*name) == value)
}

fn collector_names() -> Vec<&'static str> {
    COLLECTOR_ORDER.into_iter().map(collector_name).collect()
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WorkerOutput {
    name: CollectorName,
    collection: Value,
    status: CollectorResult,
}

pub(crate) fn collect_one(
    name: CollectorName,
    event_log_days: u32,
    windows_updates: WindowsUpdateCollectionOptions,
) -> Result<WorkerOutput, serde_json::Error> {
    macro_rules! output {
        ($result:expr) => {{
            let result = $result;
            WorkerOutput {
                name,
                collection: serde_json::to_value(result.collection)?,
                status: result.status,
            }
        }};
    }
    Ok(match name {
        CollectorName::Windows => output!(pcdiag_windows::collect_windows_info()),
        CollectorName::WindowsUpdates => {
            output!(pcdiag_windows::collect_windows_updates(windows_updates))
        }
        CollectorName::Clock => output!(pcdiag_windows::collect_clock()),
        CollectorName::Cpu => output!(pcdiag_windows::collect_cpu()),
        CollectorName::Firmware => output!(pcdiag_windows::collect_firmware()),
        CollectorName::Memory => output!(pcdiag_windows::collect_memory()),
        CollectorName::Gpu => output!(pcdiag_windows::collect_gpus()),
        CollectorName::Devices => output!(pcdiag_windows::collect_devices()),
        CollectorName::EventLogs => output!(pcdiag_windows::collect_event_logs(event_log_days)),
        CollectorName::PhysicalDisks => output!(pcdiag_windows::collect_physical_disks()),
        CollectorName::Partitions => output!(pcdiag_windows::collect_partitions()),
        CollectorName::Volumes => output!(pcdiag_windows::collect_volumes()),
        CollectorName::Smart => output!(pcdiag_windows::collect_smart()),
    })
}

pub(crate) fn collect_all<F>(
    event_log_days: u32,
    windows_updates: WindowsUpdateCollectionOptions,
    timeouts: &CollectorTimeouts,
    is_cancelled: F,
) -> Result<CompleteCollectionResult, CollectionRunError>
where
    F: Fn() -> bool,
{
    let mut outputs = Vec::with_capacity(COLLECTOR_ORDER.len());
    for name in COLLECTOR_ORDER {
        if is_cancelled() {
            return Err(CollectionRunError::Cancelled);
        }
        let timeout = timeouts.timeout_for(name);
        eprintln!(
            "pcdiag: {}を収集中です（タイムアウト: {}秒）",
            collector_name(name),
            timeout.as_secs()
        );
        match run_worker(
            name,
            event_log_days,
            windows_updates,
            timeout,
            &is_cancelled,
        ) {
            Ok(output) => outputs.push(output),
            Err(WorkerFailure::Cancelled) => return Err(CollectionRunError::Cancelled),
            Err(WorkerFailure::TimedOut(duration)) => outputs.push(failure_output(
                name,
                event_log_days,
                windows_updates,
                duration,
                "collector_timeout",
                format!(
                    "コレクターが{}秒以内に完了しなかったため終了しました",
                    timeout.as_secs()
                ),
            )),
            Err(WorkerFailure::Failed { duration, message }) => outputs.push(failure_output(
                name,
                event_log_days,
                windows_updates,
                duration,
                "collector_process_failed",
                message,
            )),
        }
    }
    assemble(outputs).map_err(|error| CollectionRunError::Protocol(error.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CollectionRunError {
    Cancelled,
    Protocol(String),
}

enum WorkerFailure {
    Cancelled,
    TimedOut(Duration),
    Failed { duration: Duration, message: String },
}

fn run_worker<F>(
    name: CollectorName,
    event_log_days: u32,
    windows_updates: WindowsUpdateCollectionOptions,
    timeout: Duration,
    is_cancelled: &F,
) -> Result<WorkerOutput, WorkerFailure>
where
    F: Fn() -> bool,
{
    let started = Instant::now();
    let executable = std::env::current_exe().map_err(|error| WorkerFailure::Failed {
        duration: started.elapsed(),
        message: format!("実行ファイルの場所を取得できませんでした: {error}"),
    })?;
    let mut command = Command::new(executable);
    command
        .arg("--internal-collect")
        .arg(collector_name(name))
        .arg("--event-log-days")
        .arg(event_log_days.to_string())
        .arg("--windows-update-days")
        .arg(optional_u32(windows_updates.lookback_days))
        .arg("--windows-update-max-entries")
        .arg(optional_u32(windows_updates.max_entries))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    platform::prepare(&mut command);
    let mut child = command.spawn().map_err(|error| WorkerFailure::Failed {
        duration: started.elapsed(),
        message: format!("コレクタープロセスを起動できませんでした: {error}"),
    })?;
    let mut job = platform::isolate(&child).map_err(|error| {
        let _ = child.kill();
        let _ = child.wait();
        WorkerFailure::Failed {
            duration: started.elapsed(),
            message: format!("コレクタープロセスを隔離できませんでした: {error}"),
        }
    })?;
    if let Err(error) = platform::resume(&child) {
        job.terminate();
        terminate(&mut child);
        return Err(WorkerFailure::Failed {
            duration: started.elapsed(),
            message: format!("隔離したコレクタープロセスを開始できませんでした: {error}"),
        });
    }
    let stdout = child.stdout.take().expect("piped stdout must be available");
    let stderr = child.stderr.take().expect("piped stderr must be available");
    let stdout_reader = thread::spawn(move || read_all(stdout));
    let stderr_reader = thread::spawn(move || read_all(stderr));

    let status = loop {
        if is_cancelled() {
            job.terminate();
            terminate(&mut child);
            join_reader(stdout_reader);
            join_reader(stderr_reader);
            return Err(WorkerFailure::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                job.terminate();
                terminate(&mut child);
                join_reader(stdout_reader);
                join_reader(stderr_reader);
                return Err(WorkerFailure::TimedOut(started.elapsed()));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(error) => {
                job.terminate();
                terminate(&mut child);
                join_reader(stdout_reader);
                join_reader(stderr_reader);
                return Err(WorkerFailure::Failed {
                    duration: started.elapsed(),
                    message: format!("コレクタープロセスを監視できませんでした: {error}"),
                });
            }
        }
    };
    // Closing or terminating the job before joining the pipe readers prevents a
    // descendant process from keeping stdout/stderr open after the worker exits.
    job.terminate();
    let stdout = join_reader(stdout_reader);
    let stderr = join_reader(stderr_reader);
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr).trim().to_string();
        return Err(WorkerFailure::Failed {
            duration: started.elapsed(),
            message: if detail.is_empty() {
                format!("コレクタープロセスが終了コード{status}で失敗しました")
            } else {
                format!("コレクタープロセスが失敗しました: {detail}")
            },
        });
    }
    let output: WorkerOutput =
        serde_json::from_slice(&stdout).map_err(|error| WorkerFailure::Failed {
            duration: started.elapsed(),
            message: format!("コレクタープロセスの出力を解釈できませんでした: {error}"),
        })?;
    if output.name != name || output.status.name != name {
        return Err(WorkerFailure::Failed {
            duration: started.elapsed(),
            message: "コレクタープロセスから異なるコレクターの結果が返されました".into(),
        });
    }
    Ok(output)
}

fn optional_u32(value: Option<u32>) -> String {
    value.map_or_else(|| "all".into(), |value| value.to_string())
}

fn terminate(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn read_all(mut reader: impl Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = reader.read_to_end(&mut bytes);
    bytes
}

fn join_reader(handle: thread::JoinHandle<Vec<u8>>) -> Vec<u8> {
    handle.join().unwrap_or_default()
}

fn failure_output(
    name: CollectorName,
    event_log_days: u32,
    windows_updates: WindowsUpdateCollectionOptions,
    duration: Duration,
    code: &str,
    message: String,
) -> WorkerOutput {
    WorkerOutput {
        name,
        collection: empty_collection(name, event_log_days, windows_updates),
        status: CollectorResult {
            name,
            status: CollectorStatus::Failed,
            duration_ms: u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
            messages: vec![CollectionMessage {
                code: code.into(),
                native_code: None,
                message: Some(message),
            }],
            fields: vec![],
        },
    }
}

fn empty_collection(
    name: CollectorName,
    event_log_days: u32,
    windows_updates: WindowsUpdateCollectionOptions,
) -> Value {
    match name {
        CollectorName::Windows => json!({
            "edition": null, "version": null, "build_number": null,
            "architecture": null, "booted_at": null, "uptime_ms": null, "boot_mode": null
        }),
        CollectorName::WindowsUpdates => json!({
            "lookback_days": windows_updates.lookback_days,
            "max_entries": windows_updates.max_entries,
            "history": null
        }),
        CollectorName::Clock => json!({
            "system_time_utc": null, "utc_offset_minutes": null,
            "windows_time_service": null, "hardware_clock": null
        }),
        CollectorName::Cpu => json!({
            "architecture": null,
            "topology": {"physical_packages": null, "physical_cores": null, "logical_processors": null},
            "packages": null,
            "features": {
                "available_instruction_sets": null,
                "hardware_virtualization_extensions_available": null,
                "virtualization_firmware_enabled": null,
                "hypervisor_present": null
            }
        }),
        CollectorName::Firmware => json!({
            "vendor": null, "version": null, "release_date": null,
            "interface_type": null, "secure_boot_enabled": null, "status": null
        }),
        CollectorName::Memory => json!({
            "physical": {"total_bytes": null, "available_bytes": null, "load_percent": null},
            "commit": {"limit_bytes": null, "available_bytes": null},
            "virtual": {"total_bytes": null, "available_bytes": null}
        }),
        CollectorName::Gpu
        | CollectorName::Devices
        | CollectorName::PhysicalDisks
        | CollectorName::Partitions
        | CollectorName::Volumes
        | CollectorName::Smart => Value::Null,
        CollectorName::EventLogs => json!({
            "lookback_days": event_log_days,
            "system": null, "application": null, "security": null
        }),
    }
}

fn assemble(outputs: Vec<WorkerOutput>) -> Result<CompleteCollectionResult, serde_json::Error> {
    let mut collection = json!({
        "windows": null,
        "windows_updates": null,
        "clock": null,
        "cpu": null,
        "firmware": null,
        "memory": null,
        "gpus": null,
        "devices": null,
        "event_logs": null,
        "storage": {"disks": null, "partitions": null, "volumes": null, "smart": null}
    });
    let mut collectors = Vec::with_capacity(outputs.len());
    for output in outputs {
        let path = match output.name {
            CollectorName::Windows => "/windows",
            CollectorName::WindowsUpdates => "/windows_updates",
            CollectorName::Clock => "/clock",
            CollectorName::Cpu => "/cpu",
            CollectorName::Firmware => "/firmware",
            CollectorName::Memory => "/memory",
            CollectorName::Gpu => "/gpus",
            CollectorName::Devices => "/devices",
            CollectorName::EventLogs => "/event_logs",
            CollectorName::PhysicalDisks => "/storage/disks",
            CollectorName::Partitions => "/storage/partitions",
            CollectorName::Volumes => "/storage/volumes",
            CollectorName::Smart => "/storage/smart",
        };
        *collection
            .pointer_mut(path)
            .expect("collector path must exist in collection template") = output.collection;
        collectors.push(output.status);
    }
    Ok(CompleteCollectionResult {
        collection: serde_json::from_value::<Collection>(collection)?,
        status: CollectionStatus { collectors },
    })
}

#[cfg(windows)]
mod platform {
    use std::{
        io,
        mem::size_of,
        os::windows::{io::AsRawHandle, process::CommandExt},
        ptr,
    };

    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
                Thread32Next,
            },
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject, TerminateJobObject,
            },
            Threading::{CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
        },
    };

    pub(super) struct Job(HANDLE);

    pub(super) fn prepare(command: &mut std::process::Command) {
        command.creation_flags(CREATE_SUSPENDED);
    }

    pub(super) fn isolate(child: &std::process::Child) -> io::Result<Job> {
        // SAFETY: null security attributes and name request a private job object.
        let job = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the structure is a plain Windows ABI value and zero is a valid baseline.
        let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: all pointers and byte lengths refer to live values of the required ABI type.
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw mut information).cast(),
                u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
                    .expect("job information size fits u32"),
            )
        };
        if configured == 0 {
            let error = io::Error::last_os_error();
            // SAFETY: job is a valid owned handle.
            unsafe { CloseHandle(job) };
            return Err(error);
        }
        let process = child.as_raw_handle() as HANDLE;
        // SAFETY: job and process are valid handles owned by this process.
        if unsafe { AssignProcessToJobObject(job, process) } == 0 {
            let error = io::Error::last_os_error();
            // SAFETY: job is a valid owned handle.
            unsafe { CloseHandle(job) };
            return Err(error);
        }
        Ok(Job(job))
    }

    pub(super) fn resume(child: &std::process::Child) -> io::Result<()> {
        // SAFETY: the snapshot handle is owned locally and closed on every return path.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the structure is initialized as required by Thread32First.
        let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
        entry.dwSize =
            u32::try_from(size_of::<THREADENTRY32>()).expect("thread entry size fits u32");
        // SAFETY: snapshot and entry are valid for thread enumeration.
        let mut found = unsafe { Thread32First(snapshot, &raw mut entry) } != 0;
        while found {
            if entry.th32OwnerProcessID == child.id() {
                // SAFETY: the enumerated thread ID belongs to the suspended child.
                let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
                if thread.is_null() {
                    let error = io::Error::last_os_error();
                    // SAFETY: snapshot is a valid owned handle.
                    unsafe { CloseHandle(snapshot) };
                    return Err(error);
                }
                // SAFETY: thread is a valid handle with THREAD_SUSPEND_RESUME access.
                let result = unsafe { ResumeThread(thread) };
                // SAFETY: thread and snapshot are valid owned handles.
                unsafe {
                    CloseHandle(thread);
                    CloseHandle(snapshot);
                }
                return if result == u32::MAX {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                };
            }
            // SAFETY: snapshot and entry remain valid for the next enumeration step.
            found = unsafe { Thread32Next(snapshot, &raw mut entry) } != 0;
        }
        // SAFETY: snapshot is a valid owned handle.
        unsafe { CloseHandle(snapshot) };
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "suspended collector thread was not found",
        ))
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: the tuple field is an owned job handle closed exactly once.
            unsafe { CloseHandle(self.0) };
        }
    }

    impl Job {
        pub(super) fn terminate(&mut self) {
            // SAFETY: the tuple field remains a valid job handle until Drop.
            unsafe { TerminateJobObject(self.0, 1) };
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::io;

    pub(super) struct Job;

    pub(super) fn prepare(_command: &mut std::process::Command) {}

    pub(super) fn isolate(child: &std::process::Child) -> io::Result<Job> {
        if child.id() == 0 {
            Err(io::Error::other("invalid child process id"))
        } else {
            Ok(Job)
        }
    }

    pub(super) fn resume(child: &std::process::Child) -> io::Result<()> {
        if child.id() == 0 {
            Err(io::Error::other("invalid child process id"))
        } else {
            Ok(())
        }
    }

    impl Job {
        pub(super) fn terminate(&mut self) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_collector_cost() {
        let timeouts = CollectorTimeouts::default();
        assert_eq!(timeouts.timeout_for(CollectorName::Memory).as_secs(), 10);
        assert_eq!(timeouts.timeout_for(CollectorName::Devices).as_secs(), 30);
        assert_eq!(
            timeouts.timeout_for(CollectorName::EventLogs).as_secs(),
            120
        );
        assert_eq!(
            timeouts
                .timeout_for(CollectorName::WindowsUpdates)
                .as_secs(),
            120
        );
    }

    #[test]
    fn parses_one_override_per_collector() {
        let mut timeouts = CollectorTimeouts::default();
        timeouts.set_from_cli(&OsString::from("smart=45")).unwrap();
        assert_eq!(timeouts.timeout_for(CollectorName::Smart).as_secs(), 45);
        assert!(timeouts.set_from_cli(&OsString::from("smart=60")).is_err());
        assert!(
            timeouts
                .set_from_cli(&OsString::from("unknown=10"))
                .is_err()
        );
        assert!(timeouts.set_from_cli(&OsString::from("memory=0")).is_err());
    }

    #[test]
    fn timeout_result_is_failed_and_preserves_options() {
        let options = WindowsUpdateCollectionOptions {
            lookback_days: Some(30),
            max_entries: Some(50),
        };
        let output = failure_output(
            CollectorName::WindowsUpdates,
            7,
            options,
            Duration::from_millis(1_005),
            "collector_timeout",
            "timed out".into(),
        );
        assert_eq!(output.status.status, CollectorStatus::Failed);
        assert_eq!(output.status.duration_ms, 1_005);
        assert_eq!(output.status.messages[0].code, "collector_timeout");
        assert_eq!(output.collection["lookback_days"], 30);
        assert_eq!(output.collection["max_entries"], 50);
        assert!(output.collection["history"].is_null());
    }

    #[test]
    fn assembly_keeps_later_collectors_after_a_timeout() {
        let options = WindowsUpdateCollectionOptions::default();
        let outputs = COLLECTOR_ORDER
            .into_iter()
            .map(|name| {
                failure_output(
                    name,
                    30,
                    options,
                    Duration::from_secs(1),
                    if name == CollectorName::Memory {
                        "collector_timeout"
                    } else {
                        "collector_process_failed"
                    },
                    "test".into(),
                )
            })
            .collect();
        let result = assemble(outputs).unwrap();
        result
            .collection
            .validate_with_status(&result.status)
            .unwrap();
        assert_eq!(result.status.collectors.len(), COLLECTOR_ORDER.len());
        assert_eq!(
            result.status.collectors[5].messages[0].code,
            "collector_timeout"
        );
        assert_eq!(result.status.collectors[6].name, CollectorName::Gpu);
    }
}
