//! Tests of the Nexus update engine: the worker index, requirements, file matching and the generic conversion.

use super::requirements::{as_requirements, newest_match, with_mod_level};
use super::workers::take_index;
use super::*;
use crate::domain::source::SourceRef;
use crate::providers::SourceUpdateRequest;
use crate::providers::nexus::ModRequirement;
use crate::providers::nexus::files::ModFile;
use crate::providers::nexus::{NexusRequirement, NexusRequiresSync};
use std::collections::BTreeSet;
use std::sync::atomic::AtomicUsize;

/// The first check must be handed out, not skipped. This is the off-by-one that made a whole
/// library check return nothing when the counter was subtracted before the first index.
#[test]
fn every_check_index_is_handed_out_once() {
    let next = AtomicUsize::new(0);
    let mut seen: Vec<usize> = std::iter::from_fn(|| take_index(&next, 5)).collect();
    seen.sort_unstable();
    assert_eq!(seen, [0, 1, 2, 3, 4]);
    assert_eq!(take_index(&next, 5), None);
}

#[test]
fn workers_never_get_the_same_index() {
    let next = AtomicUsize::new(0);
    let seen = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|_| {
                scope
                    .spawn(|| std::iter::from_fn(|| take_index(&next, 100)).collect::<Vec<usize>>())
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect::<Vec<usize>>()
    });
    let unique: BTreeSet<usize> = seen.iter().copied().collect();
    assert_eq!(seen.len(), 100);
    assert_eq!(unique.len(), 100);
}

#[test]
fn alternatives_share_a_group_and_the_read_is_remembered() {
    let found = as_requirements(vec![
        vec![ModRequirement {
            mod_id: 5,
            name: "Five".into(),
        }],
        vec![
            ModRequirement {
                mod_id: 6,
                name: "Six".into(),
            },
            ModRequirement {
                mod_id: 7,
                name: "Seven".into(),
            },
        ],
    ]);
    let groups: Vec<(u32, Option<u32>)> = found
        .iter()
        .map(|requirement| (requirement.mod_id, requirement.group))
        .collect();
    assert_eq!(groups, [(5, None), (6, Some(1)), (7, Some(1))]);

    let sync = NexusRequiresSync {
        file_id: 171_126,
        version_id: "14315126168694".into(),
        checked_at: 1_000,
        mod_level: true,
        older_version: None,
    };
    let (requirements, dlcs, sync) = super::requirements(Some(Listed {
        requires: found,
        dlcs: vec![vec!["Phantom Liberty".into()]],
        sync: Some(sync),
    }));
    let requirements = requirements.unwrap();
    assert_eq!(
        requirements
            .iter()
            .map(|requirement| (
                requirement.source.mod_id.as_str(),
                requirement.group,
                requirement.from_source
            ))
            .collect::<Vec<_>>(),
        [
            ("5", None, true),
            ("6", Some(1), true),
            ("7", Some(1), true)
        ]
    );
    assert_eq!(dlcs, [vec!["Phantom Liberty".to_string()]]);
    assert_eq!(sync.map(|sync| sync.file_id), Some("171126".to_string()));
}

#[test]
fn an_archive_without_a_link_is_matched_to_the_newest_file_of_its_name() {
    let file = |file_id: u32, file_name: &str| ModFile {
        file_id,
        mod_file_id: "100".into(),
        mod_file_name: Some("CET".into()),
        file_name: file_name.into(),
        version: None,
        file_category: Some("main".into()),
        version_category: Some("main".into()),
        position: None,
        uploaded_at: None,
        is_active: true,
    };
    let files = [
        file(10, "Old.zip"),
        file(30, "New.zip"),
        file(40, "Old.zip"),
        file(20, "Other.zip"),
    ];
    let archives = [
        ("aa".to_string(), "Old.zip".to_string()),
        ("bb".to_string(), "New.zip".to_string()),
    ];
    assert_eq!(
        newest_match(&files, &archives).map(|(digest, file)| (digest, file.file_id)),
        Some(("aa".into(), 40))
    );
    assert_eq!(
        newest_match(&files, &archives[1..]).map(|(digest, file)| (digest, file.file_id)),
        Some(("bb".into(), 30))
    );
    assert_eq!(newest_match(&files[3..], &archives), None);
}

#[test]
fn the_mods_own_list_adds_only_what_the_file_does_not_list() {
    let listed = NexusRequirement {
        mod_id: 107,
        name: "Cyber Engine Tweaks".into(),
        from_nexus: true,
        group: None,
        ignored: false,
    };
    let mod_level = [
        ModRequirement {
            mod_id: 2380,
            name: "RED4ext".into(),
        },
        ModRequirement {
            mod_id: 107,
            name: "CET".into(),
        },
        ModRequirement {
            mod_id: 7780,
            name: "Itself".into(),
        },
    ];
    let merged = with_mod_level(vec![listed.clone()], &mod_level, 7780);
    assert_eq!(
        merged.iter().map(|item| item.mod_id).collect::<Vec<_>>(),
        [107, 2380]
    );
    assert_eq!(merged[0], listed);
    assert_eq!(merged[1].name, "RED4ext");
}

#[test]
fn a_generic_request_becomes_a_nexus_lookup() {
    let request = SourceUpdateRequest {
        family: "nexus:42".into(),
        source: SourceRef::new("nexus", "42"),
        game_key: "examplegame".into(),
        current: Some("1.0".into()),
        current_file_id: Some("9".into()),
        pictures_dir: std::path::PathBuf::from("pictures"),
        saved_meta: None,
        saved_sync: None,
        archives: vec![
            crate::providers::SourceArchive {
                sha256: "aa".into(),
                original_name: "Mod 42 1.0.zip".into(),
                file_id: Some("7".into()),
                part_id: Some("100".into()),
            },
            crate::providers::SourceArchive {
                sha256: "bb".into(),
                original_name: "Old.zip".into(),
                file_id: None,
                part_id: None,
            },
        ],
    };
    let lookup = lookup::lookup(&request).unwrap();
    assert_eq!(lookup.family, "nexus:42");
    assert_eq!(lookup.nexus_id, 42);
    assert_eq!(lookup.file_id, Some(7));
    assert_eq!(lookup.current_file_id, Some(9));
    assert_eq!(lookup.part_id, Some(100));
    assert_eq!(lookup.unmatched, [("bb".into(), "Old.zip".into())]);
}
