use std::io::PipeWriter;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;

use crate::deploy::overlay;
use crate::deploy::overlay::Session;
use crate::error::AppError;
use crate::error::ErrorKind;
use crate::launch::model::Launch;

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
                    .map_err(|e| failed(e.to_string()))
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
                    .map_err(|e| failed(e.to_string()))
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
