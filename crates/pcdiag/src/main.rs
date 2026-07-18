mod bundle;
mod diagnose;
mod report;

use std::{ffi::OsString, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    match parse_args(std::env::args_os().skip(1)) {
        Ok(Command::Collect { output }) => match bundle::collect_to_bundle(&output) {
            Ok(path) => {
                println!("{}", path.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("pcdiag: {error}");
                ExitCode::from(1)
            }
        },
        Ok(Command::Diagnose { output }) => match diagnose::diagnose_bundle(&output) {
            Ok(path) => {
                println!("{}", path.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("pcdiag: {error}");
                ExitCode::from(1)
            }
        },
        Ok(Command::Report { output }) => match report::generate_report(&output) {
            Ok(path) => {
                println!("{}", path.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("pcdiag: {error}");
                ExitCode::from(1)
            }
        },
        Ok(Command::Help) => {
            print_help();
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("pcdiag: {message}");
            eprintln!("使用方法は pcdiag --help で確認できます。");
            ExitCode::from(2)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Collect { output: PathBuf },
    Diagnose { output: PathBuf },
    Report { output: PathBuf },
    Help,
}

fn parse_args(arguments: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut arguments = arguments.into_iter();
    let Some(command) = arguments.next() else {
        return Err(
            "引数なし実行はreportの実装後に有効になります。現在はcollectまたはdiagnoseを指定してください"
                .into(),
        );
    };
    if command == "--help" || command == "-h" {
        if arguments.next().is_some() {
            return Err("--helpに追加の引数は指定できません".into());
        }
        return Ok(Command::Help);
    }
    if command != "collect" && command != "diagnose" && command != "report" {
        return Err(format!(
            "未対応のコマンドです: {}",
            command.to_string_lossy()
        ));
    }
    let Some(option) = arguments.next() else {
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
    let Some(output) = arguments.next() else {
        return Err("--outputの値が指定されていません".into());
    };
    if output.is_empty() {
        return Err("--outputに空のパスは指定できません".into());
    }
    if arguments.next().is_some() {
        return Err("余分な引数が指定されています".into());
    }
    let output = PathBuf::from(output);
    Ok(match command.to_string_lossy().as_ref() {
        "collect" => Command::Collect { output },
        "diagnose" => Command::Diagnose { output },
        "report" => Command::Report { output },
        _ => unreachable!(),
    })
}

fn print_help() {
    println!(
        "pcdiag {}\n\n使用方法:\n  pcdiag collect --output <出力先ディレクトリ>\n  pcdiag diagnose --output <セッションディレクトリ>\n  pcdiag report --output <セッションディレクトリ>\n  pcdiag --help\n\nコマンド:\n  collect     診断対象PCの情報を収集し、収集バンドルを生成します\n  diagnose    収集バンドルを検証し、診断成果物を生成します\n  report      収集・診断成果物を検証し、HTMLレポートを生成します\n\n注記:\n  引数なし実行は今後の実装で有効になります。",
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_collect_output() {
        assert_eq!(
            parse_args(["collect".into(), "--output".into(), "results".into()]).unwrap(),
            Command::Collect {
                output: PathBuf::from("results")
            }
        );
    }

    #[test]
    fn rejects_missing_output_and_unimplemented_default_pipeline() {
        assert!(parse_args(["collect".into()]).is_err());
        assert!(parse_args(Vec::<OsString>::new()).is_err());
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
    fn parses_report_session_directory() {
        assert_eq!(
            parse_args(["report".into(), "--output".into(), "pcdiag-session".into()]).unwrap(),
            Command::Report {
                output: PathBuf::from("pcdiag-session")
            }
        );
    }
}
