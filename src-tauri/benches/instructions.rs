//! Instruction counts of the pure hot functions, for the Linux CI (valgrind). Run with:
//! `cargo bench --features bench --bench instructions`.
//!
//! Instruction counts do not depend on the speed of the CI machine, so they catch a change that makes a parser or a
//! path key slower even when the wall-clock benchmark would drown in noise.
#![cfg_attr(
    not(target_os = "linux"),
    allow(
        dead_code,
        reason = "the benchmarks need valgrind, which runs on Linux"
    )
)]

#[cfg(target_os = "linux")]
use std::hint::black_box;

#[cfg(target_os = "linux")]
use gobbo_lib::bench_api::{SourcePattern, hex, is_newer, normalize_path, path_key};
#[cfg(target_os = "linux")]
use iai_callgrind::{
    Callgrind, EventKind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main,
};

#[cfg(target_os = "linux")]
#[library_benchmark]
fn hex_32() -> String {
    let digest = [0x3f_u8; 32];
    black_box(hex(black_box(&digest)))
}

#[cfg(target_os = "linux")]
#[library_benchmark]
fn path_key_2() -> String {
    let paths = [
        r"Mods\modHoods\content\scripts\a.ws",
        "Content/scripts/gameplay/items/definitions/appearances.xml",
    ];
    let mut key = String::new();
    for path in paths {
        key = path_key(black_box(path));
    }
    black_box(key)
}

#[cfg(target_os = "linux")]
#[library_benchmark]
fn version_3() -> usize {
    let pairs = [("1.10", "1.9"), ("2.0", "2.0b"), ("1.2.3-rev2", "1.2.3")];
    pairs
        .iter()
        .filter(|(left, right)| is_newer(black_box(left), black_box(right)))
        .count()
}

#[cfg(target_os = "linux")]
#[library_benchmark]
fn fomod_pattern() -> bool {
    let pattern = SourcePattern::new(black_box("Textures\\2K\\"));
    let entry = normalize_path(black_box("textures/2k/a.nif"));
    black_box(pattern.matches(black_box(&entry)))
}

#[cfg(target_os = "linux")]
library_benchmark_group!(
    name = pure;
    benchmarks = hex_32, path_key_2, version_3, fomod_pattern
);

#[cfg(target_os = "linux")]
main!(
    config = LibraryBenchmarkConfig::default()
        .tool(Callgrind::default().soft_limits([(EventKind::Ir, 2.0)]));
    library_benchmark_groups = pure
);

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("the instruction benchmarks need valgrind; run them on Linux");
}
