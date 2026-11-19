//! The orchestrator: prepare a fixture, start the worker with or without the hook, and print the results.

mod ring;
mod worker;

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use gobbo_overlay_core::Layers;
use gobbo_overlay_launch::{Launch, spawn_hooked};

/// The number of copy-up operations. Each one needs a file in the lower layer.
const MAX_COPY_UP: u64 = 1_000;
/// The number of whiteout operations. Each one needs a file in the lower layer.
const MAX_WHITEOUT: u64 = 300;
/// The number of directory listings. A listing walks every entry, so it runs fewer times.
const MAX_LIST: u64 = 200;

/// The switches that must stay off in the benchmark: they add file I/O or allocations unrelated to the hook path.
const OFF_SWITCHES: [&str; 7] = [
    "GOBBO_OVERLAY_LOG",
    "GOBBO_OVERLAY_DIAG",
    "GOBBO_OVERLAY_CRASH",
    "GOBBO_OVERLAY_DUMP",
    "GOBBO_OVERLAY_RING",
    "GOBBO_OVERLAY_SESSION",
    "GOBBO_OVERLAY_CALLS",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Baseline,
    PassThrough,
    Active,
}

impl Mode {
    const ALL: [Self; 3] = [Self::Baseline, Self::PassThrough, Self::Active];

    const fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::PassThrough => "pass-through",
            Self::Active => "active",
        }
    }
}

/// The fixture of one mode: a game folder, a mod layer, an upper layer and a folder outside the game.
struct Fixture {
    game: PathBuf,
    mods: PathBuf,
    upper: PathBuf,
    outside: PathBuf,
}

impl Fixture {
    /// Creates the files the worker reads, writes, deletes and lists. `game` and `outside` have the same layout, so the
    /// baseline and the pass-through mode run the same operations.
    fn prepare(base: &Path, iterations: u64) -> Result<Self, Box<dyn Error>> {
        let fixture = Self {
            game: base.join("game"),
            mods: base.join("mods"),
            upper: base.join("upper"),
            outside: base.join("outside"),
        };
        for dir in [
            &fixture.game,
            &fixture.mods,
            &fixture.upper,
            &fixture.outside,
        ] {
            fs::create_dir_all(dir)?;
        }
        for root in [&fixture.game, &fixture.outside] {
            write(root.join("read").join("a.bin"), b"read-root")?;
            write(root.join("write").join("w.bin"), &[b'w'; 4096])?;
        }
        // The active read sees the mod layer above the game's own file.
        write(fixture.mods.join("read").join("a.bin"), b"read-mod")?;

        for number in 0..64 {
            write(
                fixture.game.join("list").join(format!("g{number}.txt")),
                b"x",
            )?;
            write(
                fixture.outside.join("list").join(format!("o{number}.txt")),
                b"x",
            )?;
        }
        for number in 0..1000 {
            write(
                fixture.game.join("big").join(format!("b{number:04}.bin")),
                b"x",
            )?;
            write(
                fixture
                    .outside
                    .join("big")
                    .join(format!("b{number:04}.bin")),
                b"x",
            )?;
            write(
                fixture
                    .game
                    .join("big_modded")
                    .join(format!("b{number:04}.bin")),
                b"x",
            )?;
            write(
                fixture
                    .outside
                    .join("big_modded")
                    .join(format!("b{number:04}.bin")),
                b"x",
            )?;
            // A large game folder with only a few overrides: the spelling strategy must not read all 1000 names just
            // to find ten of them.
            write(
                fixture
                    .game
                    .join("big_modded_few")
                    .join(format!("b{number:04}.bin")),
                b"x",
            )?;
            write(
                fixture
                    .outside
                    .join("big_modded_few")
                    .join(format!("b{number:04}.bin")),
                b"x",
            )?;
        }
        // A mod layer over the first 200 names: every one makes the listing ask the game folder for its spelling.
        for number in 0..200 {
            write(
                fixture
                    .mods
                    .join("big_modded")
                    .join(format!("b{number:04}.bin")),
                b"mod",
            )?;
        }
        for number in 0..10 {
            write(
                fixture
                    .mods
                    .join("big_modded_few")
                    .join(format!("b{number:04}.bin")),
                b"mod",
            )?;
        }
        for number in 0..16 {
            write(
                fixture.mods.join("list").join(format!("m{number}.txt")),
                b"x",
            )?;
        }
        // Names outside ASCII: the fold rule must not build a `String` per entry on the common path.
        for root in [&fixture.game, &fixture.outside] {
            write(root.join("zażółć").join("ąć.bin"), b"non-ascii")?;
            for number in 0..64 {
                write(root.join("zażółć").join(format!("ąć{number:02}.bin")), b"x")?;
            }
        }
        for number in 0..8 {
            write(
                fixture
                    .mods
                    .join("zażółć")
                    .join(format!("ąć{number:02}.bin")),
                b"mod",
            )?;
        }

        for number in 0..iterations.min(MAX_COPY_UP) {
            for root in [&fixture.game, &fixture.outside] {
                write(root.join("lower").join(format!("l{number}.bin")), b"lower")?;
            }
        }
        for number in 0..iterations.min(MAX_WHITEOUT) {
            for root in [&fixture.game, &fixture.outside] {
                write(
                    root.join("delete").join(format!("d{number}.bin")),
                    b"delete",
                )?;
            }
        }
        Ok(fixture)
    }

    fn layers(&self) -> Layers {
        Layers::new(
            self.game.clone(),
            Some(self.mods.clone()),
            self.upper.clone(),
        )
    }
}

/// Writes a file, creating its folder first.
fn write(path: PathBuf, contents: &[u8]) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

/// One row of the worker's CSV result.
struct Row {
    operation: String,
    p50: u64,
    p95: u64,
    p99: u64,
    mean: u64,
    worker_allocs: f64,
    hook_allocs: f64,
    hook_bytes: f64,
    spelling: f64,
    probes: f64,
    locks: f64,
    io: f64,
}

/// Parses the worker's result file.
fn read_rows(path: &Path) -> Result<Vec<Row>, Box<dyn Error>> {
    let text = fs::read_to_string(path)?;
    let mut rows = Vec::new();
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() != 12 {
            continue;
        }
        rows.push(Row {
            operation: fields[0].to_string(),
            p50: fields[1].parse()?,
            p95: fields[2].parse()?,
            p99: fields[3].parse()?,
            mean: fields[4].parse()?,
            worker_allocs: fields[5].parse()?,
            hook_allocs: fields[6].parse()?,
            hook_bytes: fields[7].parse()?,
            spelling: fields[8].parse()?,
            probes: fields[9].parse()?,
            locks: fields[10].parse()?,
            io: fields[11].parse()?,
        });
    }
    Ok(rows)
}

/// Fails when a result crosses a budget that should hold on every Windows machine. Timings are deliberately loose;
/// the allocation and path counts are the regression signal.
fn check(results: &[(Mode, Vec<Row>)]) -> Result<(), String> {
    let active = results
        .iter()
        .find(|(mode, _)| *mode == Mode::Active)
        .map(|(_, rows)| rows.as_slice())
        .ok_or_else(|| "the active mode is missing".to_string())?;
    let row = |operation: &str| {
        active
            .iter()
            .find(|row| row.operation == operation)
            .ok_or_else(|| format!("the active {operation} row is missing"))
    };
    let budgets: [(&str, f64, f64, u64); 8] = [
        ("get_final_path", 0.0, 0.0, 100),
        ("get_final_path_outside", 0.0, 0.0, 100),
        ("get_module_file_name", 0.0, 0.0, 10),
        ("list_modded", 64.0, 2.0, 10_000),
        ("list_nt_1000", 64.0, 2.0, 10_000),
        ("list_nt_modded", 64.0, 2.0, 10_000),
        ("list_nonascii", 48.0, 16.0, 10_000),
        ("metadata_nonascii", 8.0, 0.0, 500),
    ];
    for (operation, max_allocs, max_spelling, max_p50) in budgets {
        let operation = row(operation)?;
        if operation.hook_allocs > max_allocs
            || operation.spelling > max_spelling
            || operation.p50 > max_p50.saturating_mul(1000)
        {
            return Err(format!(
                "{} over budget: hook allocs {:.2} (max {max_allocs}), spelling {:.2} (max {max_spelling}), p50 {:.2} µs (max {max_p50})",
                operation.operation,
                operation.hook_allocs,
                operation.spelling,
                operation.p50 as f64 / 1000.0
            ));
        }
    }
    // A missing read asks the three layers once each; the parent's state is already in their answers.
    let missing = row("missing")?;
    if missing.probes > 3.0 {
        return Err(format!(
            "missing over probe budget: {:.2} (max 3)",
            missing.probes
        ));
    }
    Ok(())
}

/// Runs the benchmark and prints a table per mode, or JSON when `json` is set.
fn run(
    iterations: u64,
    warmup: u64,
    rounds: u64,
    json: bool,
    check_budgets: bool,
) -> Result<(), Box<dyn Error>> {
    let exe = std::env::current_exe()?;
    let hook = hook_dll(&exe)?;
    let base = std::env::temp_dir().join(format!("gobbo-overlay-bench-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base)?;
    if !json {
        println!("fixture: {}", base.display());
        println!("hook:    {}", hook.display());
        println!("iterations: {iterations}");
        println!("warmup: {warmup}");
        println!("rounds: {rounds}");
    }

    let mut measured = Vec::new();
    for round in 0..rounds.max(1) {
        let mut results = Vec::new();
        for mode in Mode::ALL {
            let dir = base.join(format!("{round}-{}", mode.name()));
            let fixture = Fixture::prepare(&dir, iterations)?;
            let input = if mode == Mode::PassThrough {
                fixture.outside.clone()
            } else {
                fixture.game.clone()
            };
            let result = dir.join("result.csv");
            let run = WorkerRun {
                input: &input,
                iterations,
                warmup,
                result: &result,
            };
            spawn_worker(mode, &exe, &hook, &fixture, &run)?;
            let rows = read_rows(&result)?;
            if mode == Mode::PassThrough {
                for row in &rows {
                    if row.hook_allocs > 0.0 || row.probes > 0.0 || row.locks > 0.0 || row.io > 0.0
                    {
                        return Err(format!(
                            "pass-through budget broken by {}: allocs={:.2}, probes={:.2}, locks={:.2}, io={:.2}",
                            row.operation, row.hook_allocs, row.probes, row.locks, row.io
                        )
                        .into());
                    }
                }
            }
            results.push((mode, rows));
        }
        measured.push(results);
    }
    let results = aggregate(measured);
    if check_budgets {
        check(&results)?;
    }
    if json {
        print_json(&results);
    } else {
        print_table(&results);
    }
    Ok(())
}

/// The median of the per-round rows. Every round has the same modes and operations in the same order.
fn aggregate(rounds: Vec<Vec<(Mode, Vec<Row>)>>) -> Vec<(Mode, Vec<Row>)> {
    let mut merged = Vec::new();
    for (mode_index, (mode, rows)) in rounds[0].iter().enumerate() {
        let mut merged_rows = Vec::new();
        for (operation_index, row) in rows.iter().enumerate() {
            let values: Vec<&Row> = rounds
                .iter()
                .map(|round| &round[mode_index].1[operation_index])
                .collect();
            merged_rows.push(Row {
                operation: row.operation.clone(),
                p50: median_u64(values.iter().map(|row| row.p50)),
                p95: median_u64(values.iter().map(|row| row.p95)),
                p99: median_u64(values.iter().map(|row| row.p99)),
                mean: median_u64(values.iter().map(|row| row.mean)),
                worker_allocs: median_f64(values.iter().map(|row| row.worker_allocs)),
                hook_allocs: median_f64(values.iter().map(|row| row.hook_allocs)),
                hook_bytes: median_f64(values.iter().map(|row| row.hook_bytes)),
                spelling: median_f64(values.iter().map(|row| row.spelling)),
                probes: median_f64(values.iter().map(|row| row.probes)),
                locks: median_f64(values.iter().map(|row| row.locks)),
                io: median_f64(values.iter().map(|row| row.io)),
            });
        }
        merged.push((*mode, merged_rows));
    }
    merged
}

fn median_u64(values: impl Iterator<Item = u64>) -> u64 {
    let mut values: Vec<u64> = values.collect();
    values.sort_unstable();
    values.get(values.len() / 2).copied().unwrap_or(0)
}

fn median_f64(values: impl Iterator<Item = f64>) -> f64 {
    let mut values: Vec<f64> = values.collect();
    values.sort_by(f64::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or(0.0)
}

/// The arguments one worker run shares: where to work, how many times, and where to write the result.
struct WorkerRun<'a> {
    input: &'a Path,
    iterations: u64,
    warmup: u64,
    result: &'a Path,
}

/// Starts one worker: directly for the baseline, or with the hook and the layers for the other modes.
fn spawn_worker(
    mode: Mode,
    exe: &Path,
    hook: &Path,
    fixture: &Fixture,
    run: &WorkerRun,
) -> Result<(), Box<dyn Error>> {
    let args = vec![
        "--worker".to_string(),
        run.input.display().to_string(),
        run.iterations.to_string(),
        run.warmup.to_string(),
        run.result.display().to_string(),
        // The baseline has no hook, so a missing counter is expected there and not a build mistake.
        u8::from(mode != Mode::Baseline).to_string(),
    ];
    match mode {
        Mode::Baseline => {
            let status = Command::new(exe).args(&args).status()?;
            if !status.success() {
                return Err(format!("baseline worker failed: {status}").into());
            }
        }
        Mode::PassThrough | Mode::Active => {
            let layers = fixture.layers();
            let mut env: Vec<(String, String)> = layers
                .to_env()
                .into_iter()
                .map(|(name, value)| (name.to_string(), value))
                .collect();
            for name in OFF_SWITCHES {
                env.push((name.to_string(), "off".to_string()));
            }
            let launch = Launch {
                program: exe,
                args: &args,
                hook,
                hook32: None,
                imports: &[],
                env: &env,
                cwd: None,
                output: None,
                early: None,
            };
            let hooked = spawn_hooked(&launch)?;
            let code = hooked.wait()?;
            if code != 0 {
                return Err(format!("{} worker exited with 0x{code:08x}", mode.name()).into());
            }
        }
    }
    Ok(())
}

/// Finds the hook DLL next to the benchmark binary, or one folder above it (`target/debug` for `target/debug/deps`).
fn hook_dll(exe: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let next_to_exe = exe.with_file_name("gobbo_overlay_hook.dll");
    let one_up = exe
        .parent()
        .and_then(Path::parent)
        .map(|dir| dir.join("gobbo_overlay_hook.dll"));
    [Some(next_to_exe), one_up]
        .into_iter()
        .flatten()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            "gobbo_overlay_hook.dll not found; run `cargo build -p gobbo-overlay-hook --features bench-alloc` first"
                .into()
        })
}

/// Prints the per-mode table and the overhead against the baseline.
fn print_table(results: &[(Mode, Vec<Row>)]) {
    println!();
    println!(
        "| mode | operation | p50 µs | p95 µs | p99 µs | mean µs | worker allocs/op | hook allocs/op | hook bytes/op | spelling/op | probes/op | locks/op | io/op |"
    );
    println!("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    for (mode, rows) in results {
        for row in rows {
            println!(
                "| {} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |",
                mode.name(),
                row.operation,
                row.p50 as f64 / 1000.0,
                row.p95 as f64 / 1000.0,
                row.p99 as f64 / 1000.0,
                row.mean as f64 / 1000.0,
                row.worker_allocs,
                row.hook_allocs,
                row.hook_bytes,
                row.spelling,
                row.probes,
                row.locks,
                row.io,
            );
        }
    }

    let Some((_, baseline)) = results.iter().find(|(mode, _)| *mode == Mode::Baseline) else {
        return;
    };
    println!();
    println!(
        "| operation | pass-through p50 | active p50 | active hook allocs/op | active probes/op | active locks/op | active io/op |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|");
    for row in baseline {
        let Some(active) = results
            .iter()
            .find(|(mode, _)| *mode == Mode::Active)
            .and_then(|(_, rows)| {
                rows.iter()
                    .find(|candidate| candidate.operation == row.operation)
            })
        else {
            continue;
        };
        let pass = results
            .iter()
            .find(|(mode, _)| *mode == Mode::PassThrough)
            .and_then(|(_, rows)| {
                rows.iter()
                    .find(|candidate| candidate.operation == row.operation)
            });
        let percent = |mode: &Row| {
            if row.p50 == 0 {
                0.0
            } else {
                (mode.p50 as f64 / row.p50 as f64 - 1.0) * 100.0
            }
        };
        println!(
            "| {} | {:+.1}% | {:+.1}% | {:.2} | {:.2} | {:.2} | {:.2} |",
            row.operation,
            pass.map_or(0.0, percent),
            percent(active),
            active.hook_allocs,
            active.probes,
            active.locks,
            active.io,
        );
    }
}

/// Prints the same rows as JSON, for a CI threshold or a stored measurement.
fn print_json(results: &[(Mode, Vec<Row>)]) {
    let mut modes = Vec::new();
    for (mode, rows) in results {
        let rows: Vec<String> = rows
            .iter()
            .map(|row| {
                format!(
                    r#"{{"operation":"{}","p50_us":{:.2},"p95_us":{:.2},"p99_us":{:.2},"mean_us":{:.2},"worker_allocs":{:.2},"hook_allocs":{:.2},"hook_bytes":{:.2},"spelling":{:.2},"probes":{:.2},"locks":{:.2},"io":{:.2}}}"#,
                    row.operation,
                    row.p50 as f64 / 1000.0,
                    row.p95 as f64 / 1000.0,
                    row.p99 as f64 / 1000.0,
                    row.mean as f64 / 1000.0,
                    row.worker_allocs,
                    row.hook_allocs,
                    row.hook_bytes,
                    row.spelling,
                    row.probes,
                    row.locks,
                    row.io,
                )
            })
            .collect();
        modes.push(format!(
            r#"{{"mode":"{}","rows":[{}]}}"#,
            mode.name(),
            rows.join(",")
        ));
    }
    println!(r#"{{"modes":[{}]}}"#, modes.join(","));
}

/// Parses the arguments and runs either the orchestrator or one worker.
pub fn main() {
    if let Err(error) = run_main() {
        eprintln!("overlay-bench: {error}");
        std::process::exit(1);
    }
}

fn run_main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    if first.as_deref() == Some("--ring") {
        return ring::run();
    }
    if first.as_deref() == Some("--worker") {
        let dir = args.next().ok_or("--worker needs a directory")?;
        let iterations = args.next().ok_or("--worker needs iterations")?.parse()?;
        let warmup = args.next().ok_or("--worker needs warmup")?.parse()?;
        let result = args.next().ok_or("--worker needs a result file")?;
        let hooked = match args.next().as_deref() {
            Some("1") => true,
            Some("0") | None => false,
            Some(other) => {
                return Err(format!("--worker hooked flag must be 0 or 1, not {other}").into());
            }
        };
        return worker::run(
            Path::new(&dir),
            iterations,
            warmup,
            Path::new(&result),
            hooked,
        );
    }
    let mut iterations = 5_000;
    let mut warmup = 500;
    let mut rounds = 1;
    let mut json = false;
    let mut check_budgets = false;
    let mut next = first;
    while let Some(arg) = next.take().or_else(|| args.next()) {
        match arg.as_str() {
            "--iterations" => {
                iterations = args.next().ok_or("--iterations needs a number")?.parse()?;
            }
            "--warmup" => {
                warmup = args.next().ok_or("--warmup needs a number")?.parse()?;
            }
            "--rounds" => {
                rounds = args.next().ok_or("--rounds needs a number")?.parse()?;
            }
            "--json" => {
                json = true;
            }
            "--check" => {
                check_budgets = true;
            }
            "--help" | "-h" => {
                println!(
                    "overlay-bench [--iterations N] [--warmup N] [--rounds N] [--json] [--check]\noverlay-bench --ring\n\nBuild the hook with `cargo build -p gobbo-overlay-hook --features \"bench-alloc bench-stats\"` first."
                );
                return Ok(());
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }
    run(
        iterations.max(1),
        warmup,
        rounds.max(1),
        json,
        check_budgets,
    )
}
