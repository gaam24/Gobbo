//! Micro-benchmarks of the pure overlay rules. Run with:
//! `cargo bench -p gobbo-overlay-core --profile release-bench --bench core`.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use gobbo_overlay_core::nt_listing;
use gobbo_overlay_core::{Whiteouts, relative_to, wildcard_match};

fn whiteouts() -> Whiteouts {
    let mut whiteouts = Whiteouts::default();

    for number in 0..1_000 {
        whiteouts.hide(&format!(r"content\archive{number}\data\file{number}.bin"));
    }

    whiteouts
}

fn paths(c: &mut Criterion) {
    let whiteouts = whiteouts();
    let mut group = c.benchmark_group("whiteouts");
    group.bench_function("hides/miss/depth8", |b| {
        b.iter(|| {
            black_box(whiteouts.hides(black_box(r"content\archive500\data\sub\deep\file.bin")))
        });
    });
    group.bench_function("hides/miss/depth16", |b| {
        b.iter(|| {
            black_box(whiteouts.hides(black_box(
                r"content\archive500\data\a\b\c\d\e\f\g\h\file.bin",
            )))
        });
    });
    group.bench_function("hides/hit", |b| {
        b.iter(|| black_box(whiteouts.hides(black_box(r"content\archive500\data\file500.bin"))));
    });
    group.bench_function("hides_child/hit", |b| {
        b.iter(|| {
            black_box(whiteouts.hides_child_in_visible_folder(
                black_box(r"content\archive500\data"),
                black_box("file500.bin"),
            ))
        });
    });
    group.bench_function("hides_child/miss", |b| {
        b.iter(|| {
            black_box(whiteouts.hides_child_in_visible_folder(
                black_box(r"content\archive500\data"),
                black_box("missing.bin"),
            ))
        });
    });
    let units: Vec<u16> = "file500.bin".encode_utf16().collect();
    group.bench_function("hides_child_units/hit", |b| {
        b.iter(|| {
            black_box(
                whiteouts.hides_child_units_in_visible_folder(
                    black_box(r"content\archive500\data"),
                    &units,
                ),
            )
        });
    });
    group.finish();

    let mut group = c.benchmark_group("relative_to");
    group.bench_function("inside", |b| {
        b.iter(|| {
            black_box(relative_to(
                black_box(r"C:\Games\Example"),
                black_box(r"C:\Games\Example\content\scripts\gameplay\items.xml"),
            ))
        });
    });
    group.bench_function("outside", |b| {
        b.iter(|| {
            black_box(relative_to(
                black_box(r"C:\Games\Example"),
                black_box(r"D:\Other\content\scripts\gameplay\items.xml"),
            ))
        });
    });
    group.bench_function("dots", |b| {
        b.iter(|| {
            black_box(relative_to(
                black_box(r"C:\Games\Example"),
                black_box(r"C:\Games\Example\mods\..\content\.\item.xml"),
            ))
        });
    });
    group.finish();

    let mut group = c.benchmark_group("wildcard");
    group.bench_function("ascii/star-pak", |b| {
        b.iter(|| black_box(wildcard_match(black_box("*.pak"), black_box("mod0000.pak"))));
    });
    group.bench_function("ascii/long", |b| {
        b.iter(|| {
            black_box(wildcard_match(
                black_box("content*0_*"),
                black_box("content0000_MergedFiles.0"),
            ))
        });
    });
    group.finish();
}

fn listings(c: &mut Criterion) {
    let names: Vec<Vec<u16>> = (0..1_000)
        .map(|number| format!("file{number:04}.bin").encode_utf16().collect())
        .collect();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut group = c.benchmark_group("nt_listing");
    group.bench_function("fill/names/1000", |b| {
        b.iter(|| {
            let fill = nt_listing::fill(
                nt_listing::NAMES,
                names.len(),
                |index| nt_listing::Entry {
                    name: &names[index],
                    short_name: &[],
                    attributes: 0,
                    size: 0,
                    created: 0,
                    accessed: 0,
                    written: 0,
                },
                0,
                &mut buffer,
                false,
            );
            black_box(fill);
        });
    });
    group.finish();
}

criterion_group!(benches, paths, listings);
criterion_main!(benches);
