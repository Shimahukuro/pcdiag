mod bundle;
mod diagnose;
mod interrupt;
mod report;

use pcdiag_windows::WindowsUpdateCollectionOptions;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::ExitCode,
};

fn main() -> ExitCode {
    let command = match parse_args(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("pcdiag: {message}");
            eprintln!("使用方法は pcdiag --help で確認できます。");
            return ExitCode::from(2);
        }
    };
    if command.handles_artifacts() {
        eprintln!("{SENSITIVE_DATA_NOTICE}");
        if let Err(error) = interrupt::install_handler() {
            eprintln!("pcdiag: 中断ハンドラーを登録できませんでした: {error}");
            return ExitCode::from(1);
        }
    }
    match command {
        Command::DefaultPipeline {
            output,
            windows_updates,
        } => match run_default_pipeline(&output, windows_updates) {
            Ok(path) => {
                println!("{}", path.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("pcdiag: {error}");
                exit_code_for_pipeline_error(&error)
            }
        },
        Command::Collect {
            output,
            windows_updates,
        } => match bundle::collect_to_bundle(&output, windows_updates) {
            Ok(path) => {
                println!("{}", path.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("pcdiag: {error}");
                exit_code_for_interrupted(error.is_interrupted())
            }
        },
        Command::Diagnose { output } => match diagnose::diagnose_bundle(&output) {
            Ok(path) => {
                println!("{}", path.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("pcdiag: {error}");
                exit_code_for_interrupted(error.is_interrupted())
            }
        },
        Command::Report { output } => match report::generate_report(&output) {
            Ok(path) => {
                println!("{}", path.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("pcdiag: {error}");
                exit_code_for_interrupted(error.is_interrupted())
            }
        },
        Command::Help => {
            print_help();
            ExitCode::SUCCESS
        }
    }
}

fn exit_code_for_interrupted(interrupted: bool) -> ExitCode {
    ExitCode::from(if interrupted { interrupt::EXIT_CODE } else { 1 })
}

fn exit_code_for_pipeline_error(error: &PipelineError) -> ExitCode {
    exit_code_for_interrupted(matches!(error, PipelineError::Interrupted(_)))
}

const SENSITIVE_DATA_NOTICE: &str = "\
pcdiag: 注意: 診断成果物には、端末や利用者を識別し得る情報が含まれる場合があります。
pcdiag: 注意: 保存先、共有範囲、保管期間、廃棄は担当者が管理してください。成果物は自動削除されません。";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    DefaultPipeline {
        output: PathBuf,
        windows_updates: WindowsUpdateCollectionOptions,
    },
    Collect {
        output: PathBuf,
        windows_updates: WindowsUpdateCollectionOptions,
    },
    Diagnose {
        output: PathBuf,
    },
    Report {
        output: PathBuf,
    },
    Help,
}

impl Command {
    fn handles_artifacts(&self) -> bool {
        !matches!(self, Self::Help)
    }
}

fn parse_args(arguments: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let arguments: Vec<_> = arguments.into_iter().collect();
    let Some(command) = arguments.first() else {
        return Ok(Command::DefaultPipeline {
            output: PathBuf::from("."),
            windows_updates: WindowsUpdateCollectionOptions::default(),
        });
    };
    if command == "--help" || command == "-h" {
        if arguments.len() != 1 {
            return Err("--helpに追加の引数は指定できません".into());
        }
        return Ok(Command::Help);
    }
    if command.to_string_lossy().starts_with("--") {
        let (output, windows_updates) = parse_collection_options(&arguments, false)?;
        return Ok(Command::DefaultPipeline {
            output,
            windows_updates,
        });
    }
    if command != "collect" && command != "diagnose" && command != "report" {
        return Err(format!(
            "未対応のコマンドです: {}",
            command.to_string_lossy()
        ));
    }
    if command == "collect" {
        let (output, windows_updates) = parse_collection_options(&arguments[1..], true)?;
        return Ok(Command::Collect {
            output,
            windows_updates,
        });
    }
    let Some(option) = arguments.get(1) else {
        return Err(format!(
            "{}には--output <出力先ディレクトリ>が必要です",
            command.to_string_lossy()
        ));
    };
    if option != "--output" {
        return Err(format!(
            "{}の未対応オプションです: {}",
            command.to_string_lossy(),
            option.to_string_lossy()
        ));
    }
    let Some(output) = arguments.get(2) else {
        return Err("--outputの値が指定されていません".into());
    };
    if output.is_empty() {
        return Err("--outputに空のパスは指定できません".into());
    }
    if arguments.len() != 3 {
        return Err("余分な引数が指定されています".into());
    }
    let output = PathBuf::from(output);
    Ok(match command.to_string_lossy().as_ref() {
        "diagnose" => Command::Diagnose { output },
        "report" => Command::Report { output },
        _ => unreachable!(),
    })
}

fn parse_collection_options(
    arguments: &[OsString],
    output_required: bool,
) -> Result<(PathBuf, WindowsUpdateCollectionOptions), String> {
    let mut output = None;
    let mut options = WindowsUpdateCollectionOptions::default();
    let mut index = 0;
    while index < arguments.len() {
        let option = &arguments[index];
        if option == "--windows-update-all" {
            options.lookback_days = None;
            options.max_entries = None;
            index += 1;
            continue;
        }
        let Some(value) = arguments.get(index + 1) else {
            return Err(format!(
                "{}の値が指定されていません",
                option.to_string_lossy()
            ));
        };
        if value.is_empty() {
            return Err(format!(
                "{}に空の値は指定できません",
                option.to_string_lossy()
            ));
        }
        match option.to_string_lossy().as_ref() {
            "--output" => {
                if output.replace(PathBuf::from(value)).is_some() {
                    return Err("--outputは複数回指定できません".into());
                }
            }
            "--windows-update-days" => {
                options.lookback_days = parse_optional_limit(value, 3_650, option)?;
            }
            "--windows-update-max-entries" => {
                options.max_entries = parse_optional_limit(value, 100_000, option)?;
            }
            _ => {
                return Err(format!(
                    "未対応のオプションです: {}",
                    option.to_string_lossy()
                ));
            }
        }
        index += 2;
    }
    if output_required && output.is_none() {
        return Err("collectには--output <出力先ディレクトリ>が必要です".into());
    }
    Ok((output.unwrap_or_else(|| PathBuf::from(".")), options))
}

fn parse_optional_limit(
    value: &OsString,
    maximum: u32,
    option: &OsString,
) -> Result<Option<u32>, String> {
    if value == "all" {
        return Ok(None);
    }
    value
        .to_str()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| (1..=maximum).contains(value))
        .map(Some)
        .ok_or_else(|| {
            format!(
                "{}には1から{}の整数またはallを指定してください",
                option.to_string_lossy(),
                maximum
            )
        })
}

fn print_help() {
    println!(
        "pcdiag {}\n\n使用方法:\n  pcdiag [収集オプション]\n  pcdiag collect --output <出力先ディレクトリ> [収集オプション]\n  pcdiag diagnose --output <セッションディレクトリ>\n  pcdiag report --output <セッションディレクトリ>\n  pcdiag --help\n\n収集オプション:\n  --output <パス>                         出力先ディレクトリ\n  --windows-update-days <日数|all>       Windows Update履歴の取得期間（既定: 180日）\n  --windows-update-max-entries <件数|all> 最大取得件数（既定: 1000件）\n  --windows-update-all                   Windows Updateの全履歴を取得\n\nコマンド:\n  collect     診断対象PCの情報を収集し、収集バンドルを生成します\n  diagnose    収集バンドルを検証し、診断成果物を生成します\n  report      収集・診断成果物を検証し、HTMLレポートを生成します\n\n一括実行:\n  コマンドを省略すると、collect、diagnose、reportの順に実行します。\n  --outputを省略した場合は現在の作業ディレクトリを出力先にします。",
        env!("CARGO_PKG_VERSION")
    );
}

fn run_default_pipeline(
    output_root: &Path,
    windows_updates: WindowsUpdateCollectionOptions,
) -> Result<PathBuf, PipelineError> {
    run_pipeline(
        output_root,
        |output| {
            bundle::collect_to_bundle(output, windows_updates).map_err(PipelineError::from_bundle)
        },
        |session| {
            diagnose::diagnose_bundle(session)
                .map(|_| ())
                .map_err(PipelineError::from_diagnose)
        },
        |session| report::generate_report(session).map_err(PipelineError::from_report),
    )
}

fn run_pipeline<C, D, R>(
    output_root: &Path,
    collect: C,
    diagnose: D,
    report: R,
) -> Result<PathBuf, PipelineError>
where
    C: FnOnce(&Path) -> Result<PathBuf, PipelineError>,
    D: FnOnce(&Path) -> Result<(), PipelineError>,
    R: FnOnce(&Path) -> Result<PathBuf, PipelineError>,
{
    run_pipeline_with_interrupt_check(output_root, collect, diagnose, report, interrupt::check)
}

fn run_pipeline_with_interrupt_check<C, D, R, I>(
    output_root: &Path,
    collect: C,
    diagnose: D,
    report: R,
    check_interrupt: I,
) -> Result<PathBuf, PipelineError>
where
    C: FnOnce(&Path) -> Result<PathBuf, PipelineError>,
    D: FnOnce(&Path) -> Result<(), PipelineError>,
    R: FnOnce(&Path) -> Result<PathBuf, PipelineError>,
    I: Fn(&'static str) -> Result<(), interrupt::Interrupted>,
{
    check_interrupt("collect").map_err(PipelineError::Interrupted)?;
    eprintln!("pcdiag: 情報収集を開始します");
    let session = collect(output_root)?;
    check_interrupt("diagnose").map_err(PipelineError::Interrupted)?;
    eprintln!("pcdiag: 診断を開始します: {}", session.display());
    diagnose(&session)?;
    check_interrupt("report").map_err(PipelineError::Interrupted)?;
    eprintln!("pcdiag: レポートを生成します: {}", session.display());
    let report = report(&session)?;
    eprintln!("pcdiag: 完了しました");
    Ok(report)
}

#[derive(Debug)]
enum PipelineError {
    Failed(String),
    Interrupted(interrupt::Interrupted),
}

impl PipelineError {
    fn from_bundle(error: bundle::BundleError) -> Self {
        match error {
            bundle::BundleError::Interrupted(error) => Self::Interrupted(error),
            error => Self::Failed(format!("collectに失敗しました: {error}")),
        }
    }

    fn from_diagnose(error: diagnose::DiagnoseError) -> Self {
        match error {
            diagnose::DiagnoseError::Interrupted(error) => Self::Interrupted(error),
            error => Self::Failed(format!("diagnoseに失敗しました: {error}")),
        }
    }

    fn from_report(error: report::ReportError) -> Self {
        match error {
            report::ReportError::Interrupted(error) => Self::Interrupted(error),
            error => Self::Failed(format!("reportに失敗しました: {error}")),
        }
    }
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(message) => formatter.write_str(message),
            Self::Interrupted(error) => error.fmt(formatter),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn parses_collect_output() {
        assert_eq!(
            parse_args(["collect".into(), "--output".into(), "results".into()]).unwrap(),
            Command::Collect {
                output: PathBuf::from("results"),
                windows_updates: WindowsUpdateCollectionOptions::default(),
            }
        );
    }

    #[test]
    fn shows_sensitive_data_notice_only_for_artifact_commands() {
        for command in [
            Command::DefaultPipeline {
                output: PathBuf::from("."),
                windows_updates: WindowsUpdateCollectionOptions::default(),
            },
            Command::Collect {
                output: PathBuf::from("results"),
                windows_updates: WindowsUpdateCollectionOptions::default(),
            },
            Command::Diagnose {
                output: PathBuf::from("session"),
            },
            Command::Report {
                output: PathBuf::from("session"),
            },
        ] {
            assert!(command.handles_artifacts());
        }
        assert!(!Command::Help.handles_artifacts());
        assert!(
            SENSITIVE_DATA_NOTICE
                .lines()
                .all(|line| line.starts_with("pcdiag: 注意: "))
        );
        assert!(SENSITIVE_DATA_NOTICE.contains("端末や利用者を識別し得る情報"));
        assert!(SENSITIVE_DATA_NOTICE.contains("成果物は自動削除されません"));
    }

    #[test]
    fn rejects_missing_output_and_parses_default_pipeline() {
        assert!(parse_args(["collect".into()]).is_err());
        assert_eq!(
            parse_args(Vec::<OsString>::new()).unwrap(),
            Command::DefaultPipeline {
                output: PathBuf::from("."),
                windows_updates: WindowsUpdateCollectionOptions::default(),
            }
        );
    }

    #[test]
    fn parses_diagnose_session_directory() {
        assert_eq!(
            parse_args([
                "diagnose".into(),
                "--output".into(),
                "pcdiag-session".into(),
            ])
            .unwrap(),
            Command::Diagnose {
                output: PathBuf::from("pcdiag-session")
            }
        );
    }

    #[test]
    fn parses_default_pipeline_output() {
        assert_eq!(
            parse_args(["--output".into(), "D:\\pcdiag-results".into()]).unwrap(),
            Command::DefaultPipeline {
                output: PathBuf::from("D:\\pcdiag-results"),
                windows_updates: WindowsUpdateCollectionOptions::default(),
            }
        );
        assert!(parse_args(["--output".into()]).is_err());
    }

    #[test]
    fn parses_windows_update_collection_options() {
        assert_eq!(
            parse_args([
                "collect".into(),
                "--output".into(),
                "results".into(),
                "--windows-update-days".into(),
                "90".into(),
                "--windows-update-max-entries".into(),
                "500".into(),
            ])
            .unwrap(),
            Command::Collect {
                output: PathBuf::from("results"),
                windows_updates: WindowsUpdateCollectionOptions {
                    lookback_days: Some(90),
                    max_entries: Some(500),
                },
            }
        );
        assert_eq!(
            parse_args([
                "collect".into(),
                "--output".into(),
                "results".into(),
                "--windows-update-all".into(),
            ])
            .unwrap(),
            Command::Collect {
                output: PathBuf::from("results"),
                windows_updates: WindowsUpdateCollectionOptions {
                    lookback_days: None,
                    max_entries: None,
                },
            }
        );
        assert!(
            parse_args([
                "collect".into(),
                "--output".into(),
                "results".into(),
                "--windows-update-days".into(),
                "0".into(),
            ])
            .is_err()
        );
    }

    #[test]
    fn parses_report_session_directory() {
        assert_eq!(
            parse_args(["report".into(), "--output".into(), "pcdiag-session".into()]).unwrap(),
            Command::Report {
                output: PathBuf::from("pcdiag-session")
            }
        );
    }

    #[test]
    fn default_pipeline_uses_one_session_in_order() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let session = PathBuf::from("results/pcdiag-session");
        let report_directory = session.join("report");

        let actual = run_pipeline(
            Path::new("results"),
            {
                let calls = Rc::clone(&calls);
                let session = session.clone();
                move |output| {
                    calls
                        .borrow_mut()
                        .push(format!("collect:{}", output.display()));
                    Ok(session)
                }
            },
            {
                let calls = Rc::clone(&calls);
                move |input| {
                    calls
                        .borrow_mut()
                        .push(format!("diagnose:{}", input.display()));
                    Ok(())
                }
            },
            {
                let calls = Rc::clone(&calls);
                let report_directory = report_directory.clone();
                move |input| {
                    calls
                        .borrow_mut()
                        .push(format!("report:{}", input.display()));
                    Ok(report_directory)
                }
            },
        )
        .unwrap();

        assert_eq!(actual, session.join("report"));
        assert_eq!(
            *calls.borrow(),
            [
                "collect:results",
                "diagnose:results/pcdiag-session",
                "report:results/pcdiag-session"
            ]
        );
    }

    #[test]
    fn default_pipeline_stops_after_a_failed_stage() {
        let report_called = Rc::new(RefCell::new(false));
        let error = run_pipeline(
            Path::new("results"),
            |_| Ok(PathBuf::from("results/pcdiag-session")),
            |_| Err(PipelineError::Failed("diagnose error".into())),
            {
                let report_called = Rc::clone(&report_called);
                move |_| {
                    *report_called.borrow_mut() = true;
                    Ok(PathBuf::from("unreachable"))
                }
            },
        )
        .unwrap_err();

        assert!(matches!(error, PipelineError::Failed(message) if message == "diagnose error"));
        assert!(!*report_called.borrow());
    }

    #[test]
    fn default_pipeline_does_not_start_the_next_stage_after_interruption() {
        let diagnose_called = Rc::new(RefCell::new(false));
        let report_called = Rc::new(RefCell::new(false));
        let result = run_pipeline_with_interrupt_check(
            Path::new("results"),
            |_| Ok(PathBuf::from("results/pcdiag-session")),
            {
                let diagnose_called = Rc::clone(&diagnose_called);
                move |_| {
                    *diagnose_called.borrow_mut() = true;
                    Ok(())
                }
            },
            {
                let report_called = Rc::clone(&report_called);
                move |_| {
                    *report_called.borrow_mut() = true;
                    Ok(PathBuf::from("unreachable"))
                }
            },
            |stage| {
                if stage == "diagnose" {
                    Err(interrupt::Interrupted::for_stage(stage))
                } else {
                    Ok(())
                }
            },
        );

        assert!(matches!(result, Err(PipelineError::Interrupted(_))));
        assert!(!*diagnose_called.borrow());
        assert!(!*report_called.borrow());
    }
}
