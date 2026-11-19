//! Backend benchmarks. Run with:
//! `cargo bench --profile release-bench --features bench --bench backend -- --save-baseline before`.
//! The library fixtures are in `gobbo_lib::bench_api`, compiled only for this feature. The custom profile keeps the
//! release optimization level but unwinds, which the criterion harness and the `panic = "abort"` release profile cannot
//! share.

use std::collections::BTreeMap;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use gobbo_lib::bench_api::{
    DeployFixture, FomodFixture, ImportFixture, ImportPlanFixture, LibraryView, PackFixture,
    PlanMode, SourcePattern, StatusFixture, WatchFixture, archive_fixture, hex, history_states,
    history_summary, is_newer, library_json, normalize_path, parse_library, path_key,
    plan_with_mode, store_archive, synthetic_family, synthetic_library, tail_lines, witcher_deploy,
};
use sha2::{Digest, Sha256};

/// The F6 allocator experiment: the benchmark binary only, never the shipped app.
#[cfg(feature = "bench-mimalloc")]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn pure(c: &mut Criterion) {
    let digest = [0x3f_u8; 32];
    let paths = [
        r"Mods\modHoods\content\scripts\a.ws",
        "Content/scripts/gameplay/items/definitions/appearances.xml",
    ];
    let versions = [("1.10", "1.9"), ("2.0", "2.0b"), ("1.2.3-rev2", "1.2.3")];
    let pattern = SourcePattern::new("Textures\\2K\\");
    let entry = normalize_path("textures/2k/a.nif");

    let mut group = c.benchmark_group("pure");
    group.bench_function("hex/sha256", |b| {
        b.iter(|| hex(black_box(&digest)));
    });
    group.bench_function("path_key/2", |b| {
        b.iter(|| {
            for path in &paths {
                black_box(path_key(black_box(path)));
            }
        });
    });
    group.bench_function("version/3 pairs", |b| {
        b.iter(|| {
            versions
                .iter()
                .filter(|(left, right)| is_newer(black_box(left), black_box(right)))
                .count()
        });
    });
    group.bench_function("fomod/prepared pattern", |b| {
        b.iter(|| pattern.matches(black_box(&entry)));
    });
    group.finish();
}

fn hash(c: &mut Criterion) {
    let buffer = vec![0x5a_u8; 32 * 1024 * 1024];
    c.bench_function("hash/sha256 32 MiB", |b| {
        b.iter(|| Sha256::digest(black_box(&buffer)));
    });
}

fn tail(c: &mut Criterion) {
    let path = std::env::temp_dir().join(format!("gobbo-bench-tail-{}.log", std::process::id()));
    let body = (0..20_000)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    if std::fs::write(&path, body).is_err() {
        return;
    }
    c.bench_function("tail/500 of 20k lines", |b| {
        b.iter(|| tail_lines(black_box(&path), 500));
    });
    let _ = std::fs::remove_file(path);
}

fn history(c: &mut Criterion) {
    let (old, now) = history_states(5_000);
    c.bench_function("history/summarize 5k", |b| {
        b.iter(|| history_summary(black_box(&old), black_box(&now)));
    });
}

fn library(c: &mut Criterion) {
    let (library, profile) = synthetic_library(1_000, 100);
    let family = synthetic_family(200, 20);
    let family_view = LibraryView::new(&family);

    let mut group = c.benchmark_group("library");
    group.bench_function("inputs 1k x 100", |b| {
        b.iter(|| library.inputs(black_box(&profile)));
    });
    group.bench_function("view 1k families", |b| {
        b.iter(|| LibraryView::new(black_box(&library)));
    });
    group.bench_function("versions_of 200", |b| {
        b.iter(|| family_view.versions_of(black_box("nexus:1")));
    });
    group.finish();
}

fn plan(c: &mut Criterion) {
    let deploy = witcher_deploy();
    let winners = BTreeMap::new();

    let mut group = c.benchmark_group("plan");
    for (mods, files) in [(1_000, 100), (1_000, 20)] {
        let (library, profile) = synthetic_library(mods, files);
        let inputs = library.inputs(&profile);
        for (name, mode) in [("full", PlanMode::Full), ("summary", PlanMode::Summary)] {
            group.bench_with_input(
                BenchmarkId::new(name, format!("{mods} x {files}")),
                &mode,
                |b, mode| {
                    b.iter(|| {
                        plan_with_mode(
                            black_box(&deploy),
                            black_box(&inputs),
                            black_box(&winners),
                            *mode,
                        );
                    });
                },
            );
        }
    }
    group.finish();
}

fn archive(c: &mut Criterion) {
    let (source, target) = archive_fixture(64 * 1024 * 1024);
    c.bench_function("archive/store 64 MiB", |b| {
        b.iter(|| {
            black_box(store_archive(black_box(&source), black_box(&target)));
        });
    });
    let _ = std::fs::remove_file(source);
    let _ = std::fs::remove_file(target);
}

fn status(c: &mut Criterion) {
    let fixture = StatusFixture::new(1_000, 20);
    c.bench_function("status/1k x 20", |b| b.iter(|| fixture.status()));
    c.bench_function("status/load library 1k x 20", |b| {
        b.iter(|| fixture.load_library());
    });
    c.bench_function("status/mod_folders 1k", |b| {
        b.iter(|| fixture.mod_folders());
    });

    let no_files = StatusFixture::new(1_000, 0);
    c.bench_function("status/1k x 0", |b| b.iter(|| no_files.status()));
    let small = StatusFixture::new(100, 20);
    c.bench_function("status/100 x 20", |b| b.iter(|| small.status()));
    let large = StatusFixture::new(5_000, 20);
    c.bench_function("status/5k x 20", |b| b.iter(|| large.status()));
    c.bench_function("status/load library 5k x 20", |b| {
        b.iter(|| large.load_library());
    });
    let loaded = large.loaded_library();
    c.bench_function("status/library clone 5k x 20", |b| {
        b.iter(|| black_box(loaded.clone()));
    });
    let cached = large.with_cache();
    c.bench_function("status/cached 5k x 20", |b| b.iter(|| cached.status()));
    let cached_small = fixture.with_cache();
    c.bench_function("status/cached 1k x 20", |b| {
        b.iter(|| cached_small.status());
    });
    let cached_library = cached.loaded_library();
    c.bench_function("status/save+status 5k x 20", |b| {
        // The clone is setup: production save moves the library into the cache instead of cloning it.
        b.iter_batched(
            || cached_library.clone(),
            |library| cached.save_and_status(library),
            criterion::BatchSize::PerIteration,
        );
    });
    c.bench_function("status/mod_folders 5k", |b| {
        b.iter(|| large.mod_folders());
    });

    // The default fixtures have one short version per family and no deployment. Real libraries have several
    // versions, long Nexus descriptions and a journal that the status reads on every call.
    let realistic = StatusFixture::realistic(1_000, 20, 3, 1_024);
    c.bench_function("status/realistic 1k x 20 x 3", |b| {
        b.iter(|| realistic.status());
    });
    let realistic_status = realistic.status_value();
    c.bench_function("status/serialize realistic 1k x 3", |b| {
        b.iter(|| {
            serde_json::to_string(&realistic_status)
                .expect("the benchmark status JSON")
                .len()
        });
    });
    let deployed = StatusFixture::new(1_000, 20).deployed();
    c.bench_function("status/deployed 1k x 20", |b| b.iter(|| deployed.status()));
    let deployed_cached = deployed.with_cache();
    c.bench_function("status/deployed cached 1k x 20", |b| {
        b.iter(|| deployed_cached.status());
    });

    // The XL scale builds a 10 000 × 500 index (5 million paths) and is too heavy for every run. It is opt-in:
    // `$env:GOBBO_BENCH_XL=1; cargo bench ...`.
    if std::env::var_os("GOBBO_BENCH_XL").is_some() {
        let xl = StatusFixture::new(10_000, 500);
        c.bench_function("status/10k x 500", |b| b.iter(|| xl.status()));
    }
}

fn import(c: &mut Criterion) {
    let plan_fixture = ImportPlanFixture::new(100, 200);
    c.bench_function("import/plan 20k entries", |b| {
        b.iter(|| plan_fixture.plan());
    });
    let fixture = ImportFixture::new(2_000, 4 * 1024);
    c.bench_function("import/zip 2k x 4 KiB", |b| {
        b.iter_batched(
            || fixture.folder(),
            |folder| folder.import(fixture.source()),
            criterion::BatchSize::PerIteration,
        );
    });

    if std::env::var_os("GOBBO_BENCH_FORMATS").is_some() {
        let sevenz = ImportFixture::new_7z(2_000, 4 * 1024);
        c.bench_function("import/7z 2k x 4 KiB", |b| {
            b.iter_batched(
                || sevenz.folder(),
                |folder| folder.import(sevenz.source()),
                criterion::BatchSize::PerIteration,
            );
        });
    }
}

fn deploy(c: &mut Criterion) {
    let fixture = DeployFixture::new(100, 100);
    c.bench_function("deploy/refresh 10k", |b| b.iter(|| fixture.refresh()));
    c.bench_function("deploy/plan 10k", |b| b.iter(|| fixture.plan()));
    c.bench_function("deploy/apply 10k", |b| {
        b.iter_batched(
            || {
                fixture.undo();
            },
            |()| fixture.apply(),
            criterion::BatchSize::PerIteration,
        );
    });
    c.bench_function("deploy/undo 10k", |b| {
        b.iter_batched(
            || {
                fixture.undo();
                fixture.apply();
            },
            |()| fixture.undo(),
            criterion::BatchSize::PerIteration,
        );
    });
    c.bench_function("deploy/apply+undo 10k", |b| b.iter(|| fixture.cycle()));
}

fn watch(c: &mut Criterion) {
    if std::env::var_os("GOBBO_BENCH_WATCH").is_none() {
        return;
    }
    let fixture = WatchFixture::new(50_000);
    c.bench_function("watch/take 50k", |b| {
        b.iter(|| black_box(fixture.take()));
    });
    c.bench_function("watch/changes 50k", |b| b.iter(|| fixture.changes()));
}

fn json(c: &mut Criterion) {
    let fixture = StatusFixture::new(5_000, 20);
    let library = fixture.loaded_library();
    let text = library_json(&library);
    let save_path =
        std::env::temp_dir().join(format!("gobbo-bench-save-{}.json", std::process::id()));
    c.bench_function("json/serialize 5k x 20", |b| {
        b.iter(|| library_json(black_box(&library)));
    });
    c.bench_function("json/parse 5k x 20", |b| {
        b.iter(|| parse_library(black_box(&text)));
    });
    c.bench_function("json/save 5k x 20", |b| {
        b.iter(|| {
            library
                .save(black_box(&save_path))
                .expect("the benchmark save");
        });
    });
    let _ = std::fs::remove_file(save_path);

    let fomod = FomodFixture::new(500, 10_000);
    c.bench_function("fomod/500 patterns x 10k", |b| {
        b.iter(|| fomod.matches());
    });
}

fn pack(c: &mut Criterion) {
    let fixture = PackFixture::new(300, 2, 50);
    c.bench_function("pack/find 50 in 300 families", |b| {
        b.iter(|| fixture.resolve());
    });
}

criterion_group!(
    benches, pure, hash, tail, history, library, plan, status, import, deploy, watch, json, pack,
    archive
);
criterion_main!(benches);
