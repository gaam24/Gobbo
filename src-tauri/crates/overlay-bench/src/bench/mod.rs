//! The orchestrator: prepare a fixture, start the worker with or without the hook, and print the results.

mod ring;
mod worker;

use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use gobbo_overlay_core::Layers;
use gobbo_overlay_decisions::{
    ENV_CALLS, ENV_CRASH, ENV_DIAG, ENV_IN_PROCESS_DUMP, ENV_LOG, ENV_RING, ENV_SESSION,
};
use gobbo_overlay_launch::{Launch, spawn_hooked};

/// The number of copy-up operations. Each one needs a file in the lower layer.
const MAX_COPY_UP: u64 = 1_000;
/// The number of whiteout operations. Each one needs a file in the lower layer.
const MAX_WHITEOUT: u64 = 300;
/// The number of directory listings. A listing walks every entry, so it runs fewer times.
const MAX_LIST: u64 = 200;

/// The command that builds a hook with every counter the benchmark reads.
const HOOK_BUILD_COMMAND: &str =
    "cargo build -p gobbo-overlay-hook --features \"bench-alloc bench-stats\"";

// The schema is declared once in `define_row!` below. Every helper that reads or writes a result (the CSV header and
// row, the JSON object, the table cells and the median) is generated from that one list, so a new metric cannot be left
// out of one format. A struct literal that does not supply a new field is a compile error.

/// The Rust type of one schema field.
macro_rules! field_type {
    (text) => {
        String
    };
    (time) => {
        u64
    };
    (rate) => {
        f64
    };
}

/// Parses one CSV field into its schema type.
macro_rules! parse_field {
    (text, $value:expr) => {
        $value.to_string()
    };
    (time, $value:expr) => {
        $value.parse()?
    };
    (rate, $value:expr) => {
        $value.parse()?
    };
}

/// Renders one field for a CSV row.
macro_rules! csv_field {
    (text, $value:expr) => {
        $value.clone()
    };
    (time, $value:expr) => {
        $value.to_string()
    };
    (rate, $value:expr) => {
        format!("{:.2}", $value)
    };
}

/// Renders one field for the JSON object. Durations are stored in nanoseconds and reported in microseconds.
macro_rules! json_field {
    (text, $value:expr) => {
        format!("\"{}\"", $value)
    };
    (time, $value:expr) => {
        format!("{:.2}", $value as f64 / 1000.0)
    };
    (rate, $value:expr) => {
        format!("{:.2}", $value)
    };
}

/// Renders one field for the markdown table. Durations use the same microsecond scale as the JSON object.
macro_rules! table_field {
    (text, $value:expr) => {
        $value.clone()
    };
    (time, $value:expr) => {
        format!("{:.2}", $value as f64 / 1000.0)
    };
    (rate, $value:expr) => {
        format!("{:.2}", $value)
    };
}

/// Takes the median of one field across rows. The operation name is the same in every round, so the first one is used.
macro_rules! median_field {
    (text, $rows:expr, $field:ident) => {
        $rows
            .first()
            .map_or_else(String::new, |row| row.$field.clone())
    };
    (time, $rows:expr, $field:ident) => {
        median_u64($rows.iter().map(|row| row.$field))
    };
    (rate, $rows:expr, $field:ident) => {
        median_f64($rows.iter().map(|row| row.$field))
    };
}

/// The JSON key of a field: its column name, unless the schema gives another one.
macro_rules! json_key {
    ($field:ident) => {
        stringify!($field)
    };
    ($field:ident => $key:literal) => {
        $key
    };
}

/// Declares the one result row type and every format derived from it. Each entry is `name: kind`, with an optional
/// `=> "json_key"` when the JSON object uses a different name (the timings report microseconds, not nanoseconds).
macro_rules! define_row {
    ($( $field:ident : $kind:ident $( => $json:literal )? ),+ $(,)?) => {
        /// The ordered CSV column names, and the order of every value in a row.
        const COLUMNS: &[&str] = &[ $( stringify!($field) ),+ ];
        /// The width of one result row.
        const COLUMN_COUNT: usize = COLUMNS.len();
        // Keep the public format fixed: adding a column without updating this fails the build, not the output.
        const _: () = assert!(COLUMN_COUNT == 12);

        /// One result row: an operation name and the measurements taken for it.
        #[derive(Clone, Debug, PartialEq)]
        struct Row {
            $( $field: field_type!($kind) ),+
        }

        impl Row {
            /// The ordered column names as owned strings.
            fn columns() -> [String; COLUMN_COUNT] {
                [ $( String::from(stringify!($field)) ),+ ]
            }

            /// The JSON object keys, in the same order as the columns.
            fn json_keys() -> [String; COLUMN_COUNT] {
                [ $( String::from(json_key!($field $(=> $json)?)) ),+ ]
            }

            /// Parses a CSV data line, refusing one whose field count does not match the schema.
            fn from_csv(line: &str) -> Result<Self, Box<dyn Error>> {
                let fields: Vec<&str> = line.split(',').collect();

                if fields.len() != COLUMN_COUNT {
                    return Err(format!(
                        "expected {} fields ({}) but got {}: {line}",
                        COLUMN_COUNT,
                        COLUMNS.join(","),
                        fields.len()
                    )
                    .into());
                }

                // The width was checked above, so every `next()` yields a field.
                let mut fields = fields.into_iter();
                Ok(Self {
                    $( $field: {
                        let value = fields.next().unwrap_or("");
                        parse_field!($kind, value)
                    } ),+
                })
            }

            /// Renders the row as one CSV line, without a trailing newline.
            fn to_csv(&self) -> String {
                [ $( csv_field!($kind, self.$field) ),+ ].join(",")
            }

            /// Writes the row as one CSV line.
            fn write_csv(&self, out: &mut impl Write) -> std::io::Result<()> {
                writeln!(out, "{}", self.to_csv())
            }

            /// The markdown table cells, in schema order.
            fn table_cells(&self) -> [String; COLUMN_COUNT] {
                [ $( table_field!($kind, self.$field) ),+ ]
            }

            /// The JSON object body for this row, without the surrounding braces.
            fn to_json(&self) -> String {
                let values = [ $( json_field!($kind, self.$field) ),+ ];
                Self::json_keys()
                    .iter()
                    .zip(values.iter())
                    .map(|(key, value)| format!("\"{key}\":{value}"))
                    .collect::<Vec<_>>()
                    .join(",")
            }

            /// The median of each field across rounds, keyed by the first row's operation name.
            fn median(rows: &[&Row]) -> Row {
                Row {
                    $( $field: median_field!($kind, rows, $field) ),+
                }
            }
        }
    };
}

define_row! {
    operation: text,
    p50: time => "p50_us",
    p95: time => "p95_us",
    p99: time => "p99_us",
    mean: time => "mean_us",
    worker_allocs: rate,
    hook_allocs: rate,
    hook_bytes: rate,
    spelling: rate,
    probes: rate,
    locks: rate,
    io: rate,
}

/// The folder names of one mode's fixture. The fixture builder and the worker share them so a path is built from the
/// layout instead of being inferred from another folder's name.
mod layout {
    /// The game folder, which is the worker's input in the baseline and active modes.
    pub const GAME: &str = "game";
    /// The mod layer, read above the game folder in the active mode.
    pub const MODS: &str = "mods";
    /// The writable upper layer.
    pub const UPPER: &str = "upper";
    /// The folder outside every mount, which is the worker's input in the pass-through mode.
    pub const OUTSIDE: &str = "outside";
    /// The folder with the small read fixture.
    pub const READ: &str = "read";
    /// The folder with the write fixture.
    pub const WRITE: &str = "write";
    /// The folder with the small listing fixture.
    pub const LIST: &str = "list";
    /// The folder with a thousand unmodified names.
    pub const BIG: &str = "big";
    /// The folder with a thousand names and two hundred overrides.
    pub const BIG_MODDED: &str = "big_modded";
    /// The folder with a thousand names and ten overrides.
    pub const BIG_MODDED_FEW: &str = "big_modded_few";
    /// The folder with non-ASCII names.
    pub const NONASCII: &str = "zażółć";
    /// The folder with the copy-up fixture.
    pub const LOWER: &str = "lower";
    /// The folder with the whiteout fixture.
    pub const DELETE: &str = "delete";
}

/// The switches that must stay off in the benchmark: they add file I/O or allocations unrelated to the hook path.
const OFF_SWITCHES: [&str; 7] = [
    ENV_LOG,
    ENV_DIAG,
    ENV_CRASH,
    ENV_IN_PROCESS_DUMP,
    ENV_RING,
    ENV_SESSION,
    ENV_CALLS,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

    /// Parses the mode name a worker receives, so the worker never infers its behavior from a folder name.
    fn parse(name: &str) -> Result<Self, Box<dyn Error>> {
        match name {
            "baseline" => Ok(Self::Baseline),
            "pass-through" => Ok(Self::PassThrough),
            "active" => Ok(Self::Active),
            other => Err(format!("unknown worker mode: {other}").into()),
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
            game: base.join(layout::GAME),
            mods: base.join(layout::MODS),
            upper: base.join(layout::UPPER),
            outside: base.join(layout::OUTSIDE),
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
            write(root.join(layout::READ).join("a.bin"), b"read-root")?;
            write(root.join(layout::WRITE).join("w.bin"), &[b'w'; 4096])?;
        }
        // The active read sees the mod layer above the game's own file.
        write(fixture.mods.join(layout::READ).join("a.bin"), b"read-mod")?;

        for number in 0..64 {
            write(
                fixture
                    .game
                    .join(layout::LIST)
                    .join(format!("g{number}.txt")),
                b"x",
            )?;
            write(
                fixture
                    .outside
                    .join(layout::LIST)
                    .join(format!("o{number}.txt")),
                b"x",
            )?;
        }

        for number in 0..1000 {
            write(
                fixture
                    .game
                    .join(layout::BIG)
                    .join(format!("b{number:04}.bin")),
                b"x",
            )?;
            write(
                fixture
                    .outside
                    .join(layout::BIG)
                    .join(format!("b{number:04}.bin")),
                b"x",
            )?;
            write(
                fixture
                    .game
                    .join(layout::BIG_MODDED)
                    .join(format!("b{number:04}.bin")),
                b"x",
            )?;
            write(
                fixture
                    .outside
                    .join(layout::BIG_MODDED)
                    .join(format!("b{number:04}.bin")),
                b"x",
            )?;
            // A large game folder with only a few overrides: the spelling strategy must not read all 1000 names just
            // to find ten of them.
            write(
                fixture
                    .game
                    .join(layout::BIG_MODDED_FEW)
                    .join(format!("b{number:04}.bin")),
                b"x",
            )?;
            write(
                fixture
                    .outside
                    .join(layout::BIG_MODDED_FEW)
                    .join(format!("b{number:04}.bin")),
                b"x",
            )?;
        }
        // A mod layer over the first 200 names: every one makes the listing ask the game folder for its spelling.
        for number in 0..200 {
            write(
                fixture
                    .mods
                    .join(layout::BIG_MODDED)
                    .join(format!("b{number:04}.bin")),
                b"mod",
            )?;
        }

        for number in 0..10 {
            write(
                fixture
                    .mods
                    .join(layout::BIG_MODDED_FEW)
                    .join(format!("b{number:04}.bin")),
                b"mod",
            )?;
        }

        for number in 0..16 {
            write(
                fixture
                    .mods
                    .join(layout::LIST)
                    .join(format!("m{number}.txt")),
                b"x",
            )?;
        }
        // Names outside ASCII: the fold rule must not build a `String` per entry on the common path.
        for root in [&fixture.game, &fixture.outside] {
            write(root.join(layout::NONASCII).join("ąć.bin"), b"non-ascii")?;

            for number in 0..64 {
                write(
                    root.join(layout::NONASCII)
                        .join(format!("ąć{number:02}.bin")),
                    b"x",
                )?;
            }
        }

        for number in 0..8 {
            write(
                fixture
                    .mods
                    .join(layout::NONASCII)
                    .join(format!("ąć{number:02}.bin")),
                b"mod",
            )?;
        }

        for number in 0..iterations.min(MAX_COPY_UP) {
            for root in [&fixture.game, &fixture.outside] {
                write(
                    root.join(layout::LOWER).join(format!("l{number}.bin")),
                    b"lower",
                )?;
            }
        }

        for number in 0..iterations.min(MAX_WHITEOUT) {
            for root in [&fixture.game, &fixture.outside] {
                write(
                    root.join(layout::DELETE).join(format!("d{number}.bin")),
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

/// Parses the worker's result file and reports the position of any bad line.
fn read_rows(path: &Path) -> Result<Vec<Row>, Box<dyn Error>> {
    let text = fs::read_to_string(path)?;
    let mut rows = Vec::new();

    for (line_number, line) in text.lines().enumerate().skip(1) {
        let row = Row::from_csv(line)
            .map_err(|error| format!("result line {} ({error})", line_number + 1))?;
        rows.push(row);
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
            let result = dir.join("result.csv");
            let run = WorkerRun {
                root: &dir,
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

    let results = aggregate(measured)?;

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

/// The rows of every mode in every measurement round.
type Measured = Vec<Vec<(Mode, Vec<Row>)>>;
/// The rows of every mode after taking the median across rounds.
type Aggregated = Vec<(Mode, Vec<Row>)>;

/// The median of the per-round rows. A missing mode or operation is a diagnosis, not an out-of-bounds panic.
fn aggregate(rounds: Measured) -> Result<Aggregated, Box<dyn Error>> {
    let first = rounds.first().ok_or("no rounds were measured")?;
    let mut merged = Vec::new();

    for (mode_index, (mode, rows)) in first.iter().enumerate() {
        let mut merged_rows = Vec::new();

        for (operation_index, row) in rows.iter().enumerate() {
            let mut values = Vec::with_capacity(rounds.len());

            for (round_index, round) in rounds.iter().enumerate() {
                let mode_rows = round.get(mode_index).ok_or_else(|| {
                    format!("round {} has no {} mode", round_index + 1, mode.name())
                })?;
                let value = mode_rows.1.get(operation_index).ok_or_else(|| {
                    format!(
                        "round {} has no `{}` row for mode {} (row {})",
                        round_index + 1,
                        row.operation,
                        mode.name(),
                        operation_index + 1
                    )
                })?;
                values.push(value);
            }

            merged_rows.push(Row::median(&values));
        }

        merged.push((*mode, merged_rows));
    }

    Ok(merged)
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

/// The arguments one worker run shares: the fixture root, how many times to run, and where to write the result.
struct WorkerRun<'a> {
    root: &'a Path,
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
        run.root.display().to_string(),
        run.iterations.to_string(),
        run.warmup.to_string(),
        run.result.display().to_string(),
        mode.name().to_string(),
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
            // The bench uses the legacy `Layers::to_env()` variables on purpose: it isolates the hook's own call
            // behavior. The file-based session the app writes is covered by the integration suite, and nothing measured
            // here happens before the hooks are installed.
            let mut env: Vec<(String, String)> = layers
                .to_env()
                .into_iter()
                .map(|(name, value)| (name.to_string(), value))
                .collect();
            for name in OFF_SWITCHES {
                env.push((name.to_string(), "off".to_string()));
            }

            // No early redirect, static imports, cwd or output pipe: the bench measures hooked file calls, not the
            // app's launch orchestration (the integration suite owns that path).
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
            format!("gobbo_overlay_hook.dll not found; run `{HOOK_BUILD_COMMAND}` first").into()
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
            println!("| {} | {} |", mode.name(), row.table_cells().join(" | "));
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
            .map(|row| format!("{{{}}}", row.to_json()))
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
        let root = args.next().ok_or("--worker needs a fixture root")?;
        let iterations = args.next().ok_or("--worker needs iterations")?.parse()?;
        let warmup = args.next().ok_or("--worker needs warmup")?.parse()?;
        let result = args.next().ok_or("--worker needs a result file")?;
        let mode = Mode::parse(&args.next().ok_or("--worker needs a mode")?)?;
        return worker::run(
            Path::new(&root),
            iterations,
            warmup,
            Path::new(&result),
            mode,
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
                    "overlay-bench [--iterations N] [--warmup N] [--rounds N] [--json] [--check]\noverlay-bench --ring\n\nBuild the hook with `{HOOK_BUILD_COMMAND}` first."
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row() -> Row {
        Row {
            operation: "open_read".to_string(),
            p50: 1_250,
            p95: 2_500,
            p99: 3_750,
            mean: 2_000,
            worker_allocs: 1.25,
            hook_allocs: 2.5,
            hook_bytes: 4.75,
            spelling: 1.0,
            probes: 3.25,
            locks: 2.0,
            io: 0.5,
        }
    }

    #[test]
    fn every_column_is_in_the_csv_header_and_the_json_object() {
        let row = sample_row();
        let header = Row::columns().join(",");
        let json = row.to_json();

        for (column, key) in COLUMNS.iter().zip(Row::json_keys().iter()) {
            assert!(
                header.split(',').any(|name| name == *column),
                "{column} is missing from the CSV header"
            );
            assert!(
                json.contains(&format!("\"{key}\":")),
                "{key} is missing from the JSON object"
            );
        }
    }

    #[test]
    fn the_csv_and_json_formats_stay_byte_identical() {
        let row = sample_row();

        assert_eq!(
            Row::columns().join(","),
            "operation,p50,p95,p99,mean,worker_allocs,hook_allocs,hook_bytes,spelling,probes,locks,io"
        );
        assert_eq!(
            row.to_csv(),
            "open_read,1250,2500,3750,2000,1.25,2.50,4.75,1.00,3.25,2.00,0.50"
        );
        assert_eq!(
            format!("{{{}}}", row.to_json()),
            r#"{"operation":"open_read","p50_us":1.25,"p95_us":2.50,"p99_us":3.75,"mean_us":2.00,"worker_allocs":1.25,"hook_allocs":2.50,"hook_bytes":4.75,"spelling":1.00,"probes":3.25,"locks":2.00,"io":0.50}"#
        );
    }

    #[test]
    fn one_schema_width_guards_every_format() {
        let row = sample_row();

        assert_eq!(COLUMNS.len(), 12);
        assert_eq!(Row::columns().len(), COLUMNS.len());
        assert_eq!(Row::json_keys().len(), COLUMNS.len());
        assert_eq!(row.to_csv().split(',').count(), COLUMNS.len());
        assert_eq!(row.table_cells().len(), COLUMNS.len());
        assert_eq!(row.to_json().split(",\"").count(), COLUMNS.len());
    }

    #[test]
    fn median_covers_every_generated_field() {
        let low = sample_row();
        let high = Row {
            operation: "open_read".to_string(),
            p50: 9_000,
            p95: 9_100,
            p99: 9_200,
            mean: 9_300,
            worker_allocs: 9.1,
            hook_allocs: 9.2,
            hook_bytes: 9.3,
            spelling: 9.4,
            probes: 9.5,
            locks: 9.6,
            io: 9.7,
        };

        assert_eq!(Row::median(&[&low, &high]), high);
    }
}
