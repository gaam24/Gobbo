//! Launch and running-game tests: Proton, native starts, overlay sessions and steps.

use super::*;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
#[cfg(unix)]
use std::sync::atomic::Ordering;
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use crate::deploy::ops::apply_deployment;
#[cfg(unix)]
use crate::deploy::ops::undo_deployment;
use crate::infra::tasks::TaskFlags;
use crate::launch::commands::tool_exit_payload;
use crate::launch::commands::tool_task_state;
use crate::launch::report::StepStatus;
use crate::launch::report::last_run;
#[cfg(target_os = "linux")]
use crate::launch::start::Launch;
use crate::launch::start::launch_command;
#[cfg(unix)]
use crate::launch::start::overlay_session_state;
use crate::launch::steps::run_steps;
#[cfg(unix)]
use crate::launch::tools::run_game;
use crate::settings::config::Settings;
#[cfg(unix)]
use crate::test_support::require_overlay_namespaces;
#[cfg(target_os = "linux")]
use crate::test_support::scratch;

#[test]
#[cfg(target_os = "linux")]
fn launch_without_a_deployment_starts_the_exe_through_proton() {
    let (f, out) = steam_fixture("launch-plain");
    let (launch, with_mods, _) =
        launch_command(&test_games::mirror(), &f.data, MIRROR_GAME, None, &f.base).unwrap();
    assert!(!with_mods);
    let crate::launch::start::Launch::Plain(mut command) = launch else {
        panic!("a game without deployment starts as a plain command");
    };
    assert!(command.status().unwrap().success());

    let written = fs::read_to_string(&out).unwrap();
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(
        lines[0],
        f.base
            .join("library/steamapps/compatdata/2002")
            .display()
            .to_string()
    );
    assert_eq!(lines[1], "2002");
    assert_eq!(lines[2], "waitforexitandrun");
    assert_eq!(
        lines[3],
        f.game_root.join("bin/x64/game.exe").display().to_string()
    );
    assert_eq!(lines[4], "hidden");
}

#[test]
#[cfg(target_os = "linux")]
fn launch_with_an_overlay_deployment_shows_the_mods() {
    require_overlay_namespaces!();
    let (f, out) = steam_fixture("launch-overlay");
    deploy_overlay(&f);

    let (launch, with_mods, _) =
        launch_command(&test_games::mirror(), &f.data, MIRROR_GAME, None, &f.base).unwrap();
    assert!(with_mods);
    run_launch(launch);
    let written = fs::read_to_string(&out).unwrap();
    assert_eq!(written.lines().last(), Some("visible"));
}

#[test]
#[cfg(target_os = "linux")]
fn launch_says_what_is_missing_without_proton() {
    let (f, _) = steam_fixture("launch-no-proton");
    fs::remove_file(f.base.join("library/steamapps/common/Proton 9.0/proton")).unwrap();
    let err =
        launch_command(&test_games::mirror(), &f.data, MIRROR_GAME, None, &f.base).unwrap_err();
    assert_eq!(err.code, "launch.proton_not_found", "{err}");
}

#[test]
#[cfg(target_os = "linux")]
fn launch_uses_the_proton_chosen_in_settings() {
    let (f, out) = steam_fixture("launch-chosen-proton");
    let chosen = f.base.join("Proton Custom");
    fs::create_dir_all(&chosen).unwrap();
    let marker = f.base.join("chosen.txt");
    write_script(
        &chosen.join("proton"),
        &format!("#!/bin/sh\necho chosen > '{}'\n", marker.display()),
    );
    f.save_settings(
        MIRROR_GAME,
        Settings {
            proton_dir: chosen.display().to_string(),
            ..f.settings()
        },
    )
    .unwrap();

    start_mirror(&f);
    assert_eq!(fs::read_to_string(&marker).unwrap().trim(), "chosen");
    assert!(!out.exists());
}

#[test]
#[cfg(target_os = "linux")]
fn launch_refuses_a_chosen_folder_without_proton() {
    let (f, _) = steam_fixture("launch-empty-proton");
    let empty = f.base.join("empty");
    fs::create_dir_all(&empty).unwrap();
    f.save_settings(
        MIRROR_GAME,
        Settings {
            proton_dir: empty.display().to_string(),
            ..f.settings()
        },
    )
    .unwrap();
    let err =
        launch_command(&test_games::mirror(), &f.data, MIRROR_GAME, None, &f.base).unwrap_err();
    assert_eq!(err.code, "launch.proton_file_missing", "{err}");
}

#[test]
#[cfg(target_os = "linux")]
fn launch_does_not_need_the_settings_folder() {
    let base = scratch!("launch-slot");
    let library = base.join("library");
    let game_root = library.join("steamapps/common/Slot Game");
    fs::create_dir_all(&game_root).unwrap();
    fs::create_dir_all(library.join("steamapps/compatdata/1001/pfx")).unwrap();
    let proton = library.join("steamapps/common/Proton 9.0");
    fs::create_dir_all(&proton).unwrap();
    fs::write(proton.join("proton"), "").unwrap();

    let data = Data::new(base.join("data"));
    save_folders(&data, SLOT_GAME, &game_root, "");
    let (_, with_mods, spawn_error) =
        launch_command(&test_games::slot(), &data, SLOT_GAME, None, &base).unwrap();
    assert!(!with_mods);
    assert_eq!(spawn_error, "launch.proton_failed");
}

#[test]
#[cfg(target_os = "linux")]
fn launch_passes_the_settings_environment_and_arguments() {
    let (f, _) = steam_fixture("launch-env-args");
    let chosen = f.base.join("Proton Env");
    fs::create_dir_all(&chosen).unwrap();
    let marker = f.base.join("env.txt");
    let proton = chosen.join("proton");
    let script = format!(
        "#!/bin/sh\n{{ echo \"$WINEDLLOVERRIDES\"; echo \"$@\"; }} > '{}'\n",
        marker.display()
    );
    write_script(&proton, &script);
    f.save_settings(
        MIRROR_GAME,
        Settings {
            proton_dir: chosen.display().to_string(),
            launch_env: "WINEDLLOVERRIDES=\"version=n,b\"".to_string(),
            launch_args: "-skipStartScreen".to_string(),
            ..f.settings()
        },
    )
    .unwrap();

    start_mirror(&f);
    let written = fs::read_to_string(&marker).unwrap();
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(lines[0], "version=n,b");
    assert!(lines[1].starts_with("waitforexitandrun "), "{}", lines[1]);
    assert!(lines[1].ends_with(" -skipStartScreen"), "{}", lines[1]);
}

#[test]
fn a_game_without_steam_app_ids_starts_its_own_file() {
    let f = native_fixture("native-plain");
    f.save_settings(
        MIRROR_GAME,
        Settings {
            launch_env: "GOBBO_TEST_VAR=1\n".into(),
            launch_args: "--test".into(),
            ..f.settings()
        },
    )
    .unwrap();

    let (launch, with_mods, spawn_error) =
        launch_command(&native_game(), &f.data, MIRROR_GAME, None, &f.base).unwrap();
    assert!(!with_mods);
    assert_eq!(spawn_error, "launch.native_failed");
    let crate::launch::start::Launch::Plain(command) = launch else {
        panic!("with nothing deployed the native game starts as a plain command");
    };
    let exe = f.game_root.join("bin/x64/game.exe");
    assert_eq!(command.get_program(), exe.as_os_str());
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args, vec!["--test"]);
    let envs: Vec<(String, String)> = command
        .get_envs()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            )
        })
        .collect();
    assert!(envs.contains(&("GOBBO_TEST_VAR".to_string(), "1".to_string())));
    assert!(
        envs.iter()
            .all(|(name, _)| name != "STEAM_COMPAT_DATA_PATH"),
        "{envs:?}"
    );
}

#[test]
#[cfg(target_os = "linux")]
fn a_native_game_in_an_overlay_session_runs_in_the_layer() {
    let f = native_fixture("native-overlay");
    deploy_overlay(&f);

    let (launch, with_mods, _) =
        launch_command(&native_game(), &f.data, MIRROR_GAME, None, &f.base).unwrap();
    assert!(with_mods);
    let Launch::Layered {
        session,
        program,
        args,
        ..
    } = launch
    else {
        panic!("an overlay deployment must start in the layer");
    };
    let command = crate::deploy::overlay::command(&session, &program, &args);
    assert_eq!(command.get_program(), "unshare");
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(
        args.iter()
            .any(|arg| { arg == &f.game_root.join("bin/x64/game.exe").display().to_string() })
    );
}

#[test]
fn launch_reports_a_missing_native_file() {
    let f = native_fixture("native-missing-exe");
    fs::remove_file(f.game_root.join("bin/x64/game.exe")).unwrap();
    let err = launch_command(&native_game(), &f.data, MIRROR_GAME, None, &f.base).unwrap_err();
    assert_eq!(err.code, "launch.exe_missing", "{err}");
}

#[test]
#[cfg(target_os = "linux")]
fn launch_reports_the_missing_prefix_as_before() {
    let (f, _) = steam_fixture("launch-no-prefix");
    fs::remove_dir_all(f.base.join("library/steamapps/compatdata/2002/pfx")).unwrap();
    let err =
        launch_command(&test_games::mirror(), &f.data, MIRROR_GAME, None, &f.base).unwrap_err();
    assert_eq!(err.code, "settings.prefix_missing", "{err}");
}

#[test]
#[cfg(unix)]
fn run_prepares_the_overlay_session_from_the_active_profile() {
    require_overlay_namespaces!();
    let f = fixture("run-prepares");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    apply_deployment(&f.data, SLOT_GAME).unwrap();

    // Steam calls `gobbo run`; it switches the active profile to the deployed layer first.
    let code = run_game(&f.data, SLOT_GAME, "true", &[]).unwrap();
    assert_eq!(code, 0);
    assert!(
        overlay_session_state(&f.data.game(SLOT_GAME))
            .unwrap()
            .is_some()
    );
    undo_deployment(&f.data, SLOT_GAME).unwrap();
}

#[test]
fn a_step_is_skipped_without_a_deployment_and_the_launch_goes_on() {
    use crate::games::Phase;

    let f = fixture("steps-skipped");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    // No apply: a step runs only inside the overlay layer.
    let data = f.data.game(SLOT_GAME);
    fs::write(
        data.dir.join("mod_steps.toml"),
        "[mods.modA]\nbefore_start = [\"compile\"]\n",
    )
    .unwrap();

    run_steps(
        &data,
        &test_games::slot(),
        Phase::BeforeStart,
        &f.base,
        &crate::context::load_settings(&data),
    )
    .unwrap();
    let report = last_run(&f.data, SLOT_GAME).unwrap();
    assert_eq!(report.steps[0].id, "compile");
    assert_eq!(report.steps[0].status, StepStatus::Skipped);
    assert_eq!(report.steps[0].code, "steps.needs_overlay");
}

#[test]
#[cfg(target_os = "linux")]
fn a_step_of_a_steam_game_runs_through_proton_in_the_overlay() {
    use crate::games::{Phase, Step};

    // The placeholder that `game.toml` uses for the game folder.
    const GAME_FOLDER: &str = "{game}";

    require_overlay_namespaces!();
    let (f, out) = steam_fixture("step-proton");
    deploy_overlay(&f);
    // The step's program lies in the game folder, as a Windows tool of the game does.
    fs::write(f.game_root.join("bin/x64/tool.exe"), "").unwrap();
    // The mod's library folder is the only one the fixture imported, so the step is asked for by its name.
    let data = f.data.game(MIRROR_GAME);
    let folder = fs::read_dir(data.dir.join("library"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name()
        .to_string_lossy()
        .into_owned();
    fs::write(
        data.dir.join("mod_steps.toml"),
        format!("[mods.\"{folder}\"]\nbefore_start = [\"compile\"]\n"),
    )
    .unwrap();
    let mut game = test_games::mirror();
    game.steps.push(Step {
        id: "compile".into(),
        phase: Phase::BeforeStart,
        program: "bin/x64/tool.exe".into(),
        args: vec!["-out".into(), format!("{}/r6/cache/final.bin", GAME_FOLDER)],
        when_dir: Some("r6/scripts".into()),
        clean: Vec::new(),
        outputs: Vec::new(),
        log: None,
    });

    run_steps(
        &data,
        &game,
        Phase::BeforeStart,
        &f.base,
        &crate::context::load_settings(&data),
    )
    .unwrap();

    // The fake Proton writes the Steam variables, then its arguments, then whether the mod file is visible.
    let written = fs::read_to_string(&out).unwrap();
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(lines[1], "2002");
    assert_eq!(lines[2], "waitforexitandrun");
    assert_eq!(
        lines[3],
        f.game_root.join("bin/x64/tool.exe").display().to_string()
    );
    assert_eq!(lines[4], "-out");
    assert_eq!(
        lines[5],
        format!("{}/r6/cache/final.bin", f.game_root.display())
    );
    assert_eq!(lines[6], "visible");
    undo_deployment(&f.data, MIRROR_GAME).unwrap();
}

#[cfg(windows)]
#[test]
fn on_windows_a_game_starts_its_own_executable_in_its_own_folder() {
    let f = fixture("launch-windows");
    let exe = f.game_root.join("bin/game.exe");
    fs::create_dir_all(exe.parent().unwrap()).unwrap();
    fs::write(&exe, b"").unwrap();

    let game = test_games::slot();
    let (launch, _, _) = launch_command(&game, &f.data, SLOT_GAME, None, &f.base).unwrap();
    let crate::launch::start::Launch::Plain(command) = launch else {
        panic!("on Windows a game starts as a plain command when nothing is deployed");
    };

    assert_eq!(command.get_program(), exe.as_os_str());
    assert_eq!(command.get_current_dir(), exe.parent());
}

#[cfg(target_os = "linux")]
#[test]
fn a_native_linux_game_starts_in_the_folder_of_its_executable() {
    let f = native_fixture("launch-native-cwd");
    let marker = f.base.join("cwd.txt");
    let exe = f.game_root.join("bin/x64/game.exe");
    write_script(&exe, &format!("#!/bin/sh\npwd > '{}'\n", marker.display()));

    let (launch, _, _) =
        launch_command(&native_game(), &f.data, MIRROR_GAME, None, &f.base).unwrap();
    run_launch(launch);
    assert_eq!(
        fs::read_to_string(&marker).unwrap().trim(),
        exe.parent().unwrap().display().to_string()
    );
}

#[test]
#[cfg(target_os = "linux")]
fn a_tool_run_returns_before_the_tool_exits() {
    require_overlay_namespaces!();
    // A Steam fixture with the tool in the game folder, its prerequisite in the prefix, and a fake Proton that just
    // runs the program. The overlay deployment is what a tool run needs to start at all.
    test_games::install();
    let base = scratch!("tool-background");
    let library = base.join("library");
    let game_root = library.join("steamapps/common/Slot Game");
    let prefix = library.join("steamapps/compatdata/1001/pfx");
    fs::create_dir_all(prefix.join("drive_c/Runtime/8.0")).unwrap();
    let proton_dir = library.join("steamapps/common/Proton 9.0");
    fs::create_dir_all(&proton_dir).unwrap();
    write_script(
        &proton_dir.join("proton"),
        "#!/bin/sh\nshift\nexec \"$@\"\n",
    );
    let program = game_root.join("tools/tool.exe");
    fs::create_dir_all(program.parent().unwrap()).unwrap();
    write_script(&program, "#!/bin/sh\nsleep 2\n");

    let settings_dir = base.join("settings");
    fs::create_dir_all(&settings_dir).unwrap();
    let f = Fixture {
        data: Data::new(base.join("data")),
        game_root,
        settings_dir,
        base,
    };
    f.save_settings(SLOT_GAME, f.settings()).unwrap();
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    apply_deployment(&f.data, SLOT_GAME).unwrap();

    let (sender, receiver) = std::sync::mpsc::channel();
    let tasks = TaskFlags::default();
    let started = Instant::now();
    crate::launch::tools::running::run_tool(
        &f.data,
        SLOT_GAME,
        "merger",
        &f.base,
        Arc::new(AtomicBool::new(false)),
        tasks.handle("tool:launch:test"),
        Some(Box::new(move |code| {
            let _ = sender.send(code);
        })),
    )
    .unwrap();

    assert!(
        started.elapsed() < Duration::from_secs(1),
        "starting a tool does not wait for it to exit"
    );
    assert!(
        receiver.recv_timeout(Duration::from_millis(200)).is_err(),
        "the tool is still running when the command has returned"
    );
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(15)).unwrap(),
        Some(0)
    );
    undo_deployment(&f.data, SLOT_GAME).unwrap();
}

#[test]
fn a_cancelled_tool_run_is_rejected_before_it_starts() {
    let f = fixture("tool-cancelled");
    let tasks = TaskFlags::default();
    let error = crate::launch::tools::running::run_tool(
        &f.data,
        SLOT_GAME,
        "merger",
        &f.base,
        Arc::new(AtomicBool::new(true)),
        tasks.handle("tool:launch:test"),
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, "tools.cancelled");
    assert!(error.is_cancelled());
}

#[test]
fn a_tool_exit_code_and_a_cancel_are_different_task_states() {
    use crate::infra::tasks::state;

    assert_eq!(tool_task_state(Some(0), false), state::DONE);
    assert_eq!(tool_task_state(Some(7), false), state::FAILED);
    assert_eq!(tool_task_state(None, true), state::CANCELLED);
}

#[test]
fn a_cancelled_tool_exit_keeps_a_null_code_in_its_payload() {
    let killed = tool_exit_payload("game".into(), "helper".into(), Some(9), true);
    assert!(killed.cancelled);
    assert_eq!(killed.code, None, "a killed tool has no real exit code");

    let finished = tool_exit_payload("game".into(), "helper".into(), Some(0), false);
    assert!(!finished.cancelled);
    assert_eq!(finished.code, Some(0));
}

#[test]
#[cfg(target_os = "linux")]
// On a hosted runner the helper process is not visible to `Process::by_program` even though the overlay starts and the
// other launch tests pass. Run it locally with `cargo test -- --ignored`; CI does not run ignored tests.
#[ignore = "the helper process is not discovered on GitHub-hosted runners"]
fn cancelling_a_running_tool_stops_its_process_and_reports_a_null_code() {
    use crate::infra::tasks::state;
    use crate::platform::Process;

    require_overlay_namespaces!();
    // The same tool fixture as the background test, with a helper that runs long enough to be stopped. The program is
    // run through the overlay, so its process is found by the path below.
    test_games::install();
    let base = scratch!("tool-cancel-running");
    let library = base.join("library");
    let game_root = library.join("steamapps/common/Slot Game");
    let prefix = library.join("steamapps/compatdata/1001/pfx");
    fs::create_dir_all(prefix.join("drive_c/Runtime/8.0")).unwrap();
    let proton_dir = library.join("steamapps/common/Proton 9.0");
    fs::create_dir_all(&proton_dir).unwrap();
    write_script(
        &proton_dir.join("proton"),
        "#!/bin/sh\nshift\nexec \"$@\"\n",
    );
    // A copy of a long-running program is the helper: it has no child that would keep the overlay namespace mounted
    // after the process is stopped, so the exit notice arrives promptly.
    let program = game_root.join("tools/tool.exe");
    fs::create_dir_all(program.parent().unwrap()).unwrap();
    fs::copy("/bin/sleep", &program).unwrap();

    let settings_dir = base.join("settings");
    fs::create_dir_all(&settings_dir).unwrap();
    let f = Fixture {
        data: Data::new(base.join("data")),
        game_root,
        settings_dir,
        base,
    };
    f.save_settings(
        SLOT_GAME,
        Settings {
            launch_args: "60".into(),
            ..f.settings()
        },
    )
    .unwrap();
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    apply_deployment(&f.data, SLOT_GAME).unwrap();

    let cancel = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = std::sync::mpsc::channel();
    let tasks = TaskFlags::default();
    let notice_cancel = Arc::clone(&cancel);
    crate::launch::tools::running::run_tool(
        &f.data,
        SLOT_GAME,
        "merger",
        &f.base,
        Arc::clone(&cancel),
        tasks.handle("tool:launch:test"),
        Some(Box::new(move |code| {
            // The app's notice turns a cancelled flag into a null code; the raw wait code of a killed process is not
            // shown to the window.
            let _ = sender.send(tool_exit_payload(
                "slot".into(),
                "merger".into(),
                code,
                notice_cancel.load(Ordering::Relaxed),
            ));
        })),
    )
    .unwrap();

    // The tool is running; the watcher must find exactly this process. A busy runner can take a while to schedule the
    // namespace and the spawned program, so allow more than the nominal five seconds of a quiet machine.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut process = None;

    while process.is_none() && Instant::now() < deadline {
        process = Process::by_program(&program);

        if process.is_none() {
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    let process = process.expect("the helper is running in the overlay");
    assert!(process.exists());

    // A Stop click sets the flag; the watcher turns it into a real stop while the tool is still running.
    cancel.store(true, Ordering::Relaxed);
    let notice = receiver
        .recv_timeout(Duration::from_secs(15))
        .expect("the notice reports the end of the stopped tool");
    assert!(notice.cancelled);
    assert_eq!(
        notice.code, None,
        "a killed tool is not given a fake exit code"
    );

    let gone_deadline = Instant::now() + Duration::from_secs(10);

    while process.exists() && Instant::now() < gone_deadline {
        std::thread::sleep(Duration::from_millis(25));
    }

    assert!(!process.exists(), "the helper process was terminated");
    assert_eq!(
        tool_task_state(notice.code, notice.cancelled),
        state::CANCELLED
    );
    undo_deployment(&f.data, SLOT_GAME).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn a_steam_game_starts_in_its_game_folder_and_passes_the_install_path() {
    let (f, _) = steam_fixture("launch-proton-cwd");
    let chosen = f.base.join("Proton Cwd");
    fs::create_dir_all(&chosen).unwrap();
    let marker = f.base.join("cwd.txt");
    write_script(
        &chosen.join("proton"),
        &format!(
            "#!/bin/sh\n{{ pwd; printf '%s\\n' \"$STEAM_COMPAT_INSTALL_PATH\"; }} > '{}'\n",
            marker.display()
        ),
    );
    f.save_settings(
        MIRROR_GAME,
        Settings {
            proton_dir: chosen.display().to_string(),
            ..f.settings()
        },
    )
    .unwrap();

    let (launch, _, _) =
        launch_command(&test_games::mirror(), &f.data, MIRROR_GAME, None, &f.base).unwrap();
    run_launch(launch);
    let written = fs::read_to_string(&marker).unwrap();
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(lines[0], f.game_root.display().to_string());
    assert_eq!(lines[1], f.game_root.display().to_string());
}

/// The prefix Gobbo owns lives with the game's data and is the one a non-Steam Windows program starts in.
#[cfg(target_os = "linux")]
#[test]
fn a_windows_program_from_a_plain_folder_starts_in_the_owned_prefix() {
    let (f, out) = owned_fixture("launch-owned");
    let mut game = test_games::mirror();
    game.detect.steam_app_ids.clear();
    let (launch, with_mods, spawn_error) =
        launch_command(&game, &f.data, MIRROR_GAME, None, &f.base).unwrap();
    assert!(!with_mods);
    assert_eq!(spawn_error, "launch.proton_failed");
    let Launch::Plain(mut command) = launch else {
        panic!("a game without deployment starts as a plain command");
    };
    assert_eq!(command.get_current_dir(), Some(f.game_root.as_path()));
    assert!(command.status().unwrap().success());

    let written = fs::read_to_string(&out).unwrap();
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(
        lines[0],
        f.data
            .game(MIRROR_GAME)
            .dir
            .join("compatdata")
            .display()
            .to_string()
    );
    assert_eq!(lines[1], f.game_root.display().to_string());
    assert_eq!(lines[2], "", "an owned prefix has no Steam app id");
    assert_eq!(lines[3], "waitforexitandrun");
    assert_eq!(
        lines[4],
        f.game_root.join("bin/x64/game.exe").display().to_string()
    );
    assert_eq!(lines[5], "hidden");
}

/// The definition may list Steam app ids for a game that is also sold elsewhere. The install's source, not the
/// definition, decides the runtime, so a GOG install no longer asks for a Steam prefix it does not have.
#[cfg(target_os = "linux")]
#[test]
fn a_cross_store_install_uses_the_owned_prefix_instead_of_the_steam_one() {
    let (f, out) = owned_fixture("launch-cross-store");
    let game = test_games::mirror();
    assert!(!game.detect.steam_app_ids.is_empty());
    let (launch, _, _) = launch_command(&game, &f.data, MIRROR_GAME, None, &f.base).unwrap();
    let Launch::Plain(mut command) = launch else {
        panic!("a game without deployment starts as a plain command");
    };
    assert!(command.status().unwrap().success());

    let written = fs::read_to_string(&out).unwrap();
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(
        lines[0],
        f.data
            .game(MIRROR_GAME)
            .dir
            .join("compatdata")
            .display()
            .to_string()
    );
    assert_eq!(lines[2], "", "a GOG install has no Steam app id");
}

#[cfg(target_os = "linux")]
#[test]
fn a_step_of_a_non_steam_game_runs_in_the_owned_prefix() {
    use crate::games::{Phase, Step};

    const GAME_FOLDER: &str = "{game}";

    require_overlay_namespaces!();
    let (f, out) = owned_fixture("step-owned");
    deploy_overlay(&f);
    fs::write(f.game_root.join("bin/x64/tool.exe"), "").unwrap();
    let data = f.data.game(MIRROR_GAME);
    let folder = fs::read_dir(data.dir.join("library"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name()
        .to_string_lossy()
        .into_owned();
    fs::write(
        data.dir.join("mod_steps.toml"),
        format!("[mods.\"{folder}\"]\nbefore_start = [\"compile\"]\n"),
    )
    .unwrap();
    let mut game = test_games::mirror();
    game.detect.steam_app_ids.clear();
    game.steps.push(Step {
        id: "compile".into(),
        phase: Phase::BeforeStart,
        program: "bin/x64/tool.exe".into(),
        args: vec!["-out".into(), format!("{}/r6/cache/final.bin", GAME_FOLDER)],
        when_dir: Some("r6/scripts".into()),
        clean: Vec::new(),
        outputs: Vec::new(),
        log: None,
    });

    run_steps(
        &data,
        &game,
        Phase::BeforeStart,
        &f.base,
        &crate::context::load_settings(&data),
    )
    .unwrap();

    let written = fs::read_to_string(&out).unwrap();
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(lines[0], data.dir.join("compatdata").display().to_string());
    assert_eq!(lines[2], "");
    assert_eq!(lines[3], "waitforexitandrun");
    assert_eq!(
        lines[4],
        f.game_root.join("bin/x64/tool.exe").display().to_string()
    );
    assert_eq!(lines[5], "-out");
    assert_eq!(lines[7], "visible");
    undo_deployment(&f.data, MIRROR_GAME).unwrap();
}

/// A GOG install travels through a scan, is added with its store, deploys and starts in the overlay as a native program.
#[cfg(target_os = "linux")]
#[test]
fn a_gog_install_scans_adds_deploys_and_starts_in_the_overlay() {
    use crate::context::load_settings;
    use crate::settings::managed::add_game;

    require_overlay_namespaces!();
    let base = scratch!("gog-e2e");
    let home = base.join("home");
    let game_root = home.join("GOG Games/Slot Install");
    fs::create_dir_all(game_root.join("bin")).unwrap();
    fs::create_dir_all(game_root.join("content/base")).unwrap();
    fs::write(game_root.join("bin/game.exe"), "").unwrap();
    fs::write(game_root.join("goggame-2002.info"), "{}").unwrap();

    let mut game = test_games::slot();
    game.detect.gog_ids = vec!["2002".into()];
    let program = game.executables[0].path.clone();
    game.executables[0].path_linux = Some(program);
    let folders = crate::platform::StoreFolders {
        gog_libraries: vec![home.join("GOG Games")],
        ..crate::platform::StoreFolders::default()
    };
    let scans = crate::games::detect::scan_in(&[], std::slice::from_ref(&game), &folders);
    assert_eq!(
        scans[0].found.as_ref().map(|found| found.store.as_str()),
        Some("gog")
    );

    let data = Data::new(base.join("data"));
    add_game(&data, SLOT_GAME, &game_root.display().to_string()).unwrap();
    assert_eq!(load_settings(&data.game(SLOT_GAME)).source, "gog");
    fs::create_dir_all(base.join("settings")).unwrap();
    save_settings(
        &data,
        SLOT_GAME,
        Settings {
            settings_dir: base.join("settings").display().to_string(),
            ..load_settings(&data.game(SLOT_GAME))
        },
    )
    .unwrap();

    let zip =
        crate::test_support::make_zip(&base, "gog-mod.zip", &[("Mods/modA/content/x.ws", "x")]);
    import(&data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
    let rows: Vec<Row> = status(&data, SLOT_GAME)
        .unwrap()
        .mods
        .iter()
        .map(|view| row(&view.folder, true))
        .collect();
    save_profile(&data, SLOT_GAME, &rows).unwrap();
    apply_deployment(&data, SLOT_GAME).unwrap();
    assert!(
        overlay_session_state(&data.game(SLOT_GAME))
            .unwrap()
            .is_some()
    );

    let (launch, with_mods, spawn_error) =
        launch_command(&game, &data, SLOT_GAME, None, &home).unwrap();
    assert!(with_mods);
    assert_eq!(spawn_error, "launch.native_failed");
    let Launch::Layered { program, .. } = launch else {
        panic!("a deployed game starts in the overlay layer");
    };
    assert_eq!(
        program,
        game_root.join("bin/game.exe").display().to_string()
    );
}
