//! Allocation budget for the hot paths. Run with:
//! `cargo bench --profile release-bench --features bench --bench alloc`.
//!
//! `dhat` counts every heap allocation while one operation runs. The fixtures and the warm-up run outside the
//! profiler, so the numbers describe the operation, not the test data. The output is a Markdown table for
//! `plans/backend-benchmark.md`.

use std::collections::BTreeMap;
use std::hint::black_box;

use dhat::{HeapStats, Profiler};
use gobbo_lib::bench_api::{
    DeployFixture, FomodFixture, ImportPlanFixture, LibraryView, PackFixture, PlanMode,
    StatusFixture, library_json, plan_with_mode, synthetic_library, witcher_deploy,
};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// Runs `operation` once for warm-up, then once under the profiler and prints its allocations.
fn measure(name: &str, mut operation: impl FnMut() -> usize) {
    black_box(operation());
    let profiler = Profiler::builder().testing().build();
    black_box(operation());
    let stats = HeapStats::get();
    drop(profiler);
    println!(
        "| {name} | {} | {} |",
        stats.total_blocks, stats.total_bytes
    );
}

fn main() {
    println!("| case | allocs | bytes |");
    println!("|---|---:|---:|");

    let status = StatusFixture::new(1_000, 20);
    measure("status 1k x 20", || status.status());
    measure("status/load library 1k x 20", || status.load_library());
    measure("status/mod_folders 1k", || status.mod_folders());
    let realistic = StatusFixture::realistic(1_000, 20, 3, 1_024);
    measure("status/realistic 1k x 20 x 3", || realistic.status());
    let deployed = StatusFixture::new(1_000, 20).deployed();
    measure("status/deployed 1k x 20", || deployed.status());
    let deployed_cached = deployed.with_cache();
    measure("status/deployed cached 1k x 20", || {
        deployed_cached.status()
    });

    let (library, profile) = synthetic_library(1_000, 100);
    measure("library/inputs 1k x 100", || library.inputs(&profile).len());
    measure("library/view 1k families", || {
        LibraryView::new(&library).versions_of("nexus:1").len()
    });

    let deploy = witcher_deploy();
    let inputs = library.inputs(&profile);
    let winners = BTreeMap::new();
    measure("plan/summary 1k x 100", || {
        plan_with_mode(&deploy, &inputs, &winners, PlanMode::Summary).file_count as usize
    });
    measure("plan/full 1k x 100", || {
        plan_with_mode(&deploy, &inputs, &winners, PlanMode::Full)
            .files
            .len()
    });

    let import = ImportPlanFixture::new(100, 200);
    measure("import/plan 20k entries", || import.plan());

    let fomod = FomodFixture::new(500, 10_000);
    measure("fomod/500 x 10k", || fomod.matches());

    let deployable = DeployFixture::new(100, 100);
    measure("deploy/plan 10k", || deployable.plan());

    let pack = PackFixture::new(300, 2, 50);
    measure("pack/find 50 in 300 families", || pack.resolve());

    let json = StatusFixture::new(5_000, 20);
    let large = json.loaded_library();
    measure("json/serialize 5k x 20", || library_json(&large).len());
}
