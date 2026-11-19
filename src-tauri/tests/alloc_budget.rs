//! The allocation budget of the hottest paths. Run with the benchmark fixtures:
//! `cargo test --features bench --test alloc_budget`.
//!
//! Every case measures one operation in its own `dhat` profiler. The fixtures are built before the profiler, so their
//! allocations are not counted. A budget is a ceiling with room for compiler and dependency changes: it fails when a
//! change makes a path allocate several times more than it does now, not when it moves by a few bytes.

use std::collections::BTreeMap;

use dhat::{HeapStats, Profiler};
use gobbo_lib::bench_api::{
    FomodFixture, PlanMode, StatusFixture, plan_with_mode, synthetic_library, witcher_deploy,
};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// The allocations of one operation.
struct Budget {
    blocks: u64,
    bytes: u64,
}

fn measure(operation: impl FnOnce()) -> Budget {
    let profiler = Profiler::builder().testing().build();
    operation();
    let stats = HeapStats::get();
    drop(profiler);
    Budget {
        blocks: stats.total_blocks,
        bytes: stats.total_bytes,
    }
}

fn assert_under(name: &str, budget: &Budget, blocks: u64, bytes: u64) {
    assert!(
        budget.blocks <= blocks,
        "{name} allocated {} blocks, budget is {blocks}",
        budget.blocks
    );
    assert!(
        budget.bytes <= bytes,
        "{name} allocated {} bytes, budget is {bytes}",
        budget.bytes
    );
}

#[test]
fn the_status_list_carries_description_previews_not_the_full_text() {
    let status = StatusFixture::realistic(8, 1, 2, 4_096).status_value();
    // The source preview carries no page text at all; the inspector reads it through `mod_extras`.
    for description in status
        .mods
        .iter()
        .filter_map(|listed| listed.info.description.as_deref())
    {
        assert!(
            description.chars().count() <= 513,
            "the status carried a full description of {} chars",
            description.chars().count()
        );
    }
}

#[test]
fn the_hot_paths_stay_in_their_allocation_budget() {
    let status = StatusFixture::new(1_000, 20);
    let load = measure(|| {
        std::hint::black_box(status.load_library());
    });
    assert_under(
        "status/load library 1k x 20",
        &load,
        60_000,
        10 * 1024 * 1024,
    );

    let status_full = StatusFixture::new(1_000, 20);
    let full = measure(|| {
        std::hint::black_box(status_full.status());
    });
    assert_under("status 1k x 20", &full, 100_000, 14 * 1024 * 1024);

    let realistic = StatusFixture::realistic(1_000, 20, 3, 1_024);
    let realistic_status = measure(|| {
        std::hint::black_box(realistic.status());
    });
    assert_under(
        "status/realistic 1k x 20 x 3",
        &realistic_status,
        230_000,
        40 * 1024 * 1024,
    );

    let deployed = StatusFixture::new(1_000, 20).deployed().with_cache();
    let deployed_status = measure(|| {
        std::hint::black_box(deployed.status());
    });
    assert_under(
        "status/deployed cached 1k x 20",
        &deployed_status,
        60_000,
        8 * 1024 * 1024,
    );

    let (library, profile) = synthetic_library(1_000, 100);
    let inputs = measure(|| {
        std::hint::black_box(library.inputs(&profile).len());
    });
    assert_under("library/inputs 1k x 100", &inputs, 6_000, 256 * 1024);

    let deploy = witcher_deploy();
    let mod_inputs = library.inputs(&profile);
    let winners = BTreeMap::new();
    let summary = measure(|| {
        std::hint::black_box(
            plan_with_mode(&deploy, &mod_inputs, &winners, PlanMode::Summary).file_count,
        );
    });
    assert_under("plan/summary 1k x 100", &summary, 9_000, 3 * 1024 * 1024);

    let full_plan = measure(|| {
        std::hint::black_box(
            plan_with_mode(&deploy, &mod_inputs, &winners, PlanMode::Full)
                .files
                .len(),
        );
    });
    assert_under("plan/full 1k x 100", &full_plan, 180_000, 24 * 1024 * 1024);

    let fomod = FomodFixture::new(500, 10_000);
    let matches = measure(|| {
        std::hint::black_box(fomod.matches());
    });
    assert_under("fomod/500 x 10k", &matches, 0, 0);
}
