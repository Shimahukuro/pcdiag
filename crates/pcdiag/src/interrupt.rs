use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU8, Ordering},
};

static INTERRUPT_COUNT: AtomicU8 = AtomicU8::new(0);

pub(crate) const EXIT_CODE: u8 = 130;

pub(crate) fn install_handler() -> io::Result<()> {
    platform::install_handler()
}

pub(crate) fn is_requested() -> bool {
    INTERRUPT_COUNT.load(Ordering::SeqCst) != 0
}

pub(crate) fn check(stage: &'static str) -> Result<(), Interrupted> {
    if is_requested() {
        Err(Interrupted::new(stage, None, None))
    } else {
        Ok(())
    }
}

pub(crate) fn check_with_log(
    stage: &'static str,
    incomplete_directory: &Path,
) -> Result<(), Interrupted> {
    if !is_requested() {
        return Ok(());
    }
    let log_path = incomplete_directory.join("interruption.log");
    let log_error = write_interruption_log(&log_path, stage)
        .err()
        .map(|error| error.to_string());
    Err(Interrupted::new(
        stage,
        Some(incomplete_directory.to_owned()),
        log_error,
    ))
}

fn write_interruption_log(path: &Path, stage: &str) -> io::Result<()> {
    fs::write(
        path,
        format!("pcdiag interruption\nstage: {stage}\nstatus: incomplete\n"),
    )
}

#[derive(Debug)]
pub(crate) struct Interrupted {
    stage: &'static str,
    incomplete_directory: Option<PathBuf>,
    log_error: Option<String>,
}

impl Interrupted {
    #[cfg(test)]
    pub(crate) fn for_stage(stage: &'static str) -> Self {
        Self::new(stage, None, None)
    }

    fn new(
        stage: &'static str,
        incomplete_directory: Option<PathBuf>,
        log_error: Option<String>,
    ) -> Self {
        Self {
            stage,
            incomplete_directory,
            log_error,
        }
    }
}

impl fmt::Display for Interrupted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{0}を中断しました", self.stage)?;
        if let Some(path) = &self.incomplete_directory {
            write!(
                formatter,
                "。未完了の成果物を保持しています: {}",
                path.display()
            )?;
        }
        if let Some(error) = &self.log_error {
            write!(formatter, "。中断ログを書き込めませんでした: {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Interrupted {}

#[cfg(windows)]
mod platform {
    use std::io;

    use windows_sys::Win32::System::{
        Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT, SetConsoleCtrlHandler},
        Threading::ExitProcess,
    };

    use super::{EXIT_CODE, INTERRUPT_COUNT};

    unsafe extern "system" fn console_handler(control_type: u32) -> i32 {
        if control_type != CTRL_C_EVENT && control_type != CTRL_BREAK_EVENT {
            return 0;
        }
        if INTERRUPT_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 1 {
            // SAFETY: A second console interrupt intentionally requests immediate process exit.
            unsafe { ExitProcess(EXIT_CODE as u32) };
        }
        1
    }

    pub(super) fn install_handler() -> io::Result<()> {
        // SAFETY: console_handler has the required lifetime and system ABI.
        if unsafe { SetConsoleCtrlHandler(Some(console_handler), 1) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::io;

    pub(super) fn install_handler() -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interruption_log_identifies_incomplete_stage() {
        let directory =
            std::env::temp_dir().join(format!("pcdiag-interrupt-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();

        let log_path = directory.join("interruption.log");
        write_interruption_log(&log_path, "report").unwrap();
        let error = Interrupted::new("report", Some(directory.clone()), None);

        assert!(error.to_string().contains("reportを中断しました"));
        assert_eq!(
            fs::read_to_string(log_path).unwrap(),
            "pcdiag interruption\nstage: report\nstatus: incomplete\n"
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
