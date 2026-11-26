use std::error::Error as StdError;
use std::io;
use std::io::PipeWriter;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;

use crate::deploy::overlay;
use crate::deploy::overlay::Session;
use crate::error::AppError;
use crate::error::ErrorKind;
use crate::launch::model::Launch;

/// Windows `ERROR_ELEVATION_REQUIRED`. `CreateProcess` returns it when the program's manifest asks for administrator
/// rights, or when a compatibility setting on the program asks for them, and Gobbo is not elevated.
const ERROR_ELEVATION_REQUIRED: i32 = 740;

/// Whether any error in `error`'s source chain is the operating-system error `code`. The overlay path wraps the
/// `CreateProcess` error in a `FileError`, so the direct error alone is not enough.
fn has_os_error(error: &(dyn StdError + 'static), code: i32) -> bool {
    let mut current = Some(error);

    while let Some(node) = current {
        if node
            .downcast_ref::<io::Error>()
            .is_some_and(|error| error.raw_os_error() == Some(code))
        {
            return true;
        }

        current = node.source();
    }

    false
}

/// The error for a program that did not start. A program Windows refuses to create without administrator rights gets a
/// code the interface can explain and act on; every other failure keeps the caller's code and the system's text.
fn spawn_failure(spawn_error: &'static str, error: &(dyn StdError + 'static)) -> AppError {
    if spawn_error == "launch.native_failed" && has_os_error(error, ERROR_ELEVATION_REQUIRED) {
        crate::infra::log::warn(format!("launch: administrator rights required: {error}"));

        return AppError::user("launch.elevation_required")
            .with("os", ERROR_ELEVATION_REQUIRED)
            .with("detail", error);
    }

    AppError::new(spawn_error)
        .with("detail", error)
        .kind(ErrorKind::Permanent)
}

/// A started game. `wait` waits for it to exit.
pub enum Running {
    Plain(Child),
    /// Boxed: the overlay handle carries the hook and its launch timings, many times the size of a plain child.
    Layered(Box<overlay::Running>),
}

impl Launch {
    /// A plain command, or a command in the overlay layers when `session` is set.
    pub(in crate::launch) fn new(
        program: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        session: Option<Session>,
    ) -> Self {
        if let Some(session) = session {
            Self::Layered {
                session,
                program,
                args,
                env,
            }
        } else {
            let mut command = Command::new(&program);
            command.args(&args).envs(env);
            Self::Plain(command)
        }
    }

    /// Starts the game. A failure names `spawn_error`, which says whether Proton or the native file failed.
    pub(in crate::launch) fn start(
        self,
        spawn_error: &'static str,
        output: Option<PipeWriter>,
    ) -> Result<Running, AppError> {
        let failed = |detail: String| {
            AppError::new(spawn_error)
                .with("detail", detail)
                .kind(ErrorKind::Permanent)
        };

        match self {
            Self::Plain(mut command) => {
                let program = command.get_program().to_string_lossy().into_owned();
                let args: Vec<String> = command
                    .get_args()
                    .map(|arg| arg.to_string_lossy().into_owned())
                    .collect();
                crate::infra::log::info(format!("launch: program={program} args={args:?}"));

                if let Some(writer) = output {
                    let errors = writer.try_clone().map_err(|e| failed(e.to_string()))?;
                    command
                        .stdout(Stdio::from(writer))
                        .stderr(Stdio::from(errors));
                }

                command
                    .spawn()
                    .map(Running::Plain)
                    .map_err(|e| spawn_failure(spawn_error, &e))
            }

            Self::Layered {
                session,
                program,
                args,
                env,
            } => {
                crate::infra::log::info(format!("launch: program={program} args={args:?} overlay"));
                overlay::start(&session, &program, &args, &env, output)
                    .map(|running| Running::Layered(Box::new(running)))
                    .map_err(|e| spawn_failure(spawn_error, &e))
            }
        }
    }
}

impl Running {
    /// The launcher's steps when the game went through the overlay; `None` for a plain start.
    pub(in crate::launch) const fn timings(&self) -> Option<overlay::LaunchTimings> {
        match self {
            Self::Plain(_) => None,
            Self::Layered(running) => running.timings(),
        }
    }

    /// Waits for the game to exit, and returns its exit code when it has one.
    pub(in crate::launch) fn wait(&mut self) -> Option<i32> {
        match self {
            Self::Plain(child) => child.wait().ok().and_then(|status| status.code()),
            Self::Layered(running) => running.wait().ok(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::deploy::DeployError;
    use crate::error::FileError;

    #[test]
    fn a_program_that_needs_elevation_gets_its_own_code() {
        let error = io::Error::from_raw_os_error(ERROR_ELEVATION_REQUIRED);
        let failure = spawn_failure("launch.native_failed", &error);

        assert_eq!(failure.code, "launch.elevation_required");
        assert_eq!(failure.kind, ErrorKind::UserAction);
        assert_eq!(failure.params.get("os").map(String::as_str), Some("740"));
        assert!(failure.params.contains_key("detail"));
    }

    #[test]
    fn elevation_inside_the_overlay_error_is_found() {
        let error = DeployError::Io(FileError::new(
            Path::new("game.exe"),
            io::Error::from_raw_os_error(ERROR_ELEVATION_REQUIRED),
        ));
        let failure = spawn_failure("launch.native_failed", &error);

        assert_eq!(failure.code, "launch.elevation_required");
    }

    #[test]
    fn another_spawn_error_keeps_the_callers_code() {
        let error = io::Error::from_raw_os_error(5);
        let failure = spawn_failure("launch.native_failed", &error);

        assert_eq!(failure.code, "launch.native_failed");
        assert_eq!(failure.kind, ErrorKind::Permanent);
        assert!(!failure.params.contains_key("os"));
    }

    #[test]
    fn an_error_without_an_os_code_keeps_the_callers_code() {
        let error = io::Error::other("not enough memory");
        let failure = spawn_failure("launch.native_failed", &error);

        assert_eq!(failure.code, "launch.native_failed");
    }

    #[test]
    fn a_proton_failure_never_becomes_an_elevation_error() {
        let error = io::Error::from_raw_os_error(ERROR_ELEVATION_REQUIRED);
        let failure = spawn_failure("launch.proton_failed", &error);

        assert_eq!(failure.code, "launch.proton_failed");
    }
}
