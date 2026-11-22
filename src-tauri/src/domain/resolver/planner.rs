//! Builds the deployment plan: where each file lands, which copy of a contested file is deployed, and the problems that
//! stop a deployment.

use std::collections::BTreeMap;

use rustc_hash::FxHashSet;

use super::model::{
    Conflict, DeployPlan, LayoutProfile, Materialized, ModInput, PlanMode, PlannedFile, Problem,
    SettingsRow,
};
use super::shared::SharedRules;
use crate::domain::deploy::{Deploy, Slot};
use crate::domain::naming;
use crate::domain::pathkey::{path_key, path_key_into};

/// The files a summary of this list has to materialize, under the game's mirror, root-file and shared rules. The
/// library computes this once when it loads; a status then skips the per-file rules for most mods.
pub fn layout_profile(deploy: &Deploy, files: &[String]) -> LayoutProfile {
    LayoutRules::new(deploy).profile(files)
}

/// The game rules of a layout, prepared once for many file lists. Building `SharedRules` per version was an allocation
/// per version on a large library load.
pub struct LayoutRules<'a> {
    deploy: &'a Deploy,
    shared: SharedRules,
}

impl<'a> LayoutRules<'a> {
    /// Prepares the layout rules of `deploy` once, for many file lists.
    pub fn new(deploy: &'a Deploy) -> Self {
        Self {
            deploy,
            shared: SharedRules::new(&deploy.shared),
        }
    }

    /// The files of `files` a summary must materialize under the prepared rules.
    pub fn profile(&self, files: &[String]) -> LayoutProfile {
        let mirrored_only = self.deploy.mirrored_only(files);

        if mirrored_only {
            return LayoutProfile {
                materialized: Materialized::All,
                mirrored_only: true,
            };
        }

        let mut indices = Vec::new();

        for (index, rel) in files.iter().enumerate() {
            if self.deploy.keeps_game_path(rel) || self.shared.matches(rel) {
                indices.push(u32::try_from(index).unwrap_or(u32::MAX));
            }
        }

        let materialized = if files.is_empty() || indices.is_empty() {
            Materialized::None
        } else if indices.len() == files.len() {
            Materialized::All
        } else {
            Materialized::Some(indices)
        };
        LayoutProfile {
            materialized,
            mirrored_only: false,
        }
    }
}

/// One path that several mods may write: the path as first seen, and the copies in list order.
#[derive(Default)]
struct Contest {
    path: String,
    entries: Vec<PlannedFile>,
}

/// Builds the plan from the mods and the chosen winners.
///
/// `winners` maps the key of a contested path to the family the user chose for it. A plan with problems must not be
/// executed.
///
/// `Summary` skips the files that cannot conflict and do not land outside the mod's own folder; the status and the
/// deployment preview only need the count, the conflicts and the problems.
pub fn plan_with_mode(
    deploy: &Deploy,
    mods: &[ModInput<'_>],
    winners: &BTreeMap<String, String>,
    plan_mode: PlanMode,
) -> DeployPlan {
    let full = plan_mode == PlanMode::Full;
    let shared = SharedRules::new(&deploy.shared);
    let mut plan = DeployPlan::default();
    let mut contests: BTreeMap<String, Contest> = BTreeMap::new();
    let mut seen: rustc_hash::FxHashSet<&str> = rustc_hash::FxHashSet::default();
    let mut taken_names: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
    let mut not_materialized = 0_u32;

    for m in mods {
        if !seen.insert(m.id.as_ref()) {
            plan.problems.push(Problem::DuplicateMod {
                id: m.id.to_string(),
            });
            continue;
        }

        let slot = deploy.slot(&m.slot);
        // A mod whose files all mirror game paths has no folder of its own. The library's profile already answered
        // this; a library built in memory still scans its list.
        let mirrored_only = match m.layout {
            Some(profile) if profile.materialized != Materialized::Unknown => profile.mirrored_only,
            _ => deploy.mirrored_only(&m.files),
        };
        let own_folder = match slot {
            Some(slot) if !slot.flat && !mirrored_only => {
                let name = naming::deploy_name(&slot.prefix, &m.display_name, &m.folder);
                Some(unique_name(name, &m.id, &mut taken_names))
            }
            _ => None,
        };

        if let (Some(priority), Some(folder)) = (m.priority, &own_folder) {
            plan.settings.push(SettingsRow {
                folder: folder.clone(),
                enabled: m.enabled,
                priority,
            });
        }

        if !m.enabled {
            continue;
        }

        if own_folder.is_some() && !m.files.is_empty() {
            plan.slots_used.insert(m.slot.to_string());
        }
        // The library keeps every file list sorted, so the normal path walks it directly. A caller that hands in an
        // unsorted list still gets the old deterministic order. The mod's family, folder and file list are added to
        // the plan once, when its first file is materialized; a full plan then holds indexes, not three strings per
        // file.
        let sorted = m.files.is_sorted();
        let flat = slot.is_some_and(|slot| slot.flat);
        let may_keep_path = !deploy.mirror.is_empty() || !deploy.root_files.is_empty();
        let may_share = !deploy.shared.is_empty();
        // A summary of a slot-only game cannot materialize anything: every file stays in the mod's own folder. Count
        // them without walking the list at all.
        if !full && !may_keep_path && !may_share && !flat && !mirrored_only && slot.is_some() {
            not_materialized =
                not_materialized.saturating_add(u32::try_from(m.files.len()).unwrap_or(u32::MAX));
            continue;
        }
        // A profile answers the whole list from the rules once. Only the files it names need the per-file path; the
        // rest still count as not materialized. An unsorted list keeps the old order, so it takes the per-file path.
        let selected = if !full && !flat && sorted && slot.is_some() {
            match m.layout.map(|profile| &profile.materialized) {
                Some(Materialized::None) => {
                    not_materialized = not_materialized
                        .saturating_add(u32::try_from(m.files.len()).unwrap_or(u32::MAX));
                    continue;
                }
                Some(Materialized::Some(indices)) => {
                    not_materialized = not_materialized.saturating_add(
                        u32::try_from(m.files.len().saturating_sub(indices.len()))
                            .unwrap_or(u32::MAX),
                    );
                    Some(indices.as_slice())
                }
                _ => None,
            }
        } else {
            None
        };
        let mut source_at: Option<u32> = None;
        let mut place = |index: usize, rel: &String| {
            // When every file of the mod mirrors a game path, the per-file check is already known to be true.
            let mirrored = mirrored_only || (may_keep_path && deploy.keeps_game_path(rel));
            let shared_rule = !mirrored && !flat && may_share && shared.matches(rel);

            // A mod without a slot (a loader release) keeps its game paths, so its files can meet another mod's.
            if !full && !mirrored && !flat && !shared_rule && slot.is_some() {
                // The file stays in the mod's own folder and cannot meet another mod. It still counts, but it does
                // not need a destination string or a `PlannedFile`.
                not_materialized = not_materialized.saturating_add(1);
                return;
            }

            let (dest, game_path) = destination(slot, own_folder.as_deref(), rel, mirrored);
            let source = *source_at.get_or_insert_with(|| {
                let at = u32::try_from(plan.sources.len()).unwrap_or(u32::MAX);
                plan.sources.push(super::model::PlanSource {
                    id: std::sync::Arc::from(m.id.as_ref()),
                    folder: std::sync::Arc::from(m.folder.as_ref()),
                    files: std::sync::Arc::clone(&m.files),
                });
                at
            });
            let planned = PlannedFile {
                source,
                file: u32::try_from(index).unwrap_or(u32::MAX),
                dest,
                game_path,
            };
            // A file in the mod's own folder meets no other mod's, unless the game reads its kind from one mod only.
            // A game whose loose files are keyed by resource name meets every other file with the same name, also
            // when the two files sit in different folders.
            let contested_as = if deploy.resource_names && game_path {
                Some(file_name(&planned.dest))
            } else if game_path {
                Some(planned.dest.as_str())
            } else if shared_rule {
                Some(rel.as_str())
            } else {
                None
            };
            let Some(path) = contested_as else {
                plan.files.push(planned);
                return;
            };
            let contest = contests.entry(path_key(path)).or_insert_with(|| Contest {
                path: path.to_string(),
                entries: Vec::new(),
            });
            // A flat slot can bring two files of one mod to the same name. Keep the first, but name the collision
            // instead of dropping the second file silently.
            if let Some(existing) = contest.entries.iter().find(|entry| entry.source == source) {
                if existing.file != planned.file {
                    let path = if deploy.resource_names && game_path {
                        file_name(&planned.dest).to_string()
                    } else {
                        planned.dest.clone()
                    };
                    plan.problems.push(Problem::DuplicateDestination {
                        path,
                        id: m.id.to_string(),
                    });
                }
            } else {
                contest.entries.push(planned);
            }
        };

        if let Some(indices) = selected {
            for &index in indices {
                if let Some(rel) = m.files.get(index as usize) {
                    place(index as usize, rel);
                }
            }
        } else if sorted {
            for (index, rel) in m.files.iter().enumerate() {
                place(index, rel);
            }
        } else {
            let mut files: Vec<(usize, &String)> = m.files.iter().enumerate().collect();
            files.sort_unstable_by_key(|(_, left)| *left);

            for (index, rel) in files {
                place(index, rel);
            }
        }
    }

    settle_contests(contests, winners, &mut plan);

    if full {
        check_file_folder_collisions(&mut plan);
    }

    plan.file_count = u32::try_from(plan.files.len())
        .unwrap_or(u32::MAX)
        .saturating_add(not_materialized);
    plan
}

/// A deployment cannot write a file `DLC/x` and a file `DLC/x/y` in one pass: one of them would have to be a folder.
/// The plan names the pair before the first write, rather than failing halfway through with `ENOTDIR`.
fn check_file_folder_collisions(plan: &mut DeployPlan) {
    // All keys live in one string and one range table. The old version allocated a fresh key for every ancestor of
    // every file, which was a large allocation cost on a plan of a thousand mods.
    let capacity: usize = plan.files.iter().map(|file| file.dest.len() + 1).sum();
    let mut arena = String::with_capacity(capacity);
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::with_capacity(plan.files.len());

    for file in &plan.files {
        ranges.push(path_key_into(&file.dest, &mut arena));
    }

    let destinations: FxHashSet<&str> = ranges.iter().map(|range| &arena[range.clone()]).collect();

    for (file, range) in plan.files.iter().zip(&ranges) {
        let key = &arena[range.clone()];
        let mut at = key.len();

        while let Some(slash) = key[..at].rfind('/') {
            at = slash;

            if destinations.contains(&key[..at]) {
                plan.problems.push(Problem::FileFolderCollision {
                    path: file.dest.clone(),
                });
                break;
            }
        }
    }
}

/// The last component of a path, with either separator. A file name is the resource key of a game whose loose files are
/// resolved by name.
fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Where a file of a mod lands, relative to the deployment root, and whether that is a game path. A file of a mirrored
/// folder keeps its game path. In a slot it goes into the mod's own folder, or straight into a flat slot.
fn destination(
    slot: Option<&Slot>,
    own_folder: Option<&str>,
    rel: &str,
    mirrored: bool,
) -> (String, bool) {
    match (mirrored, slot) {
        (false, Some(slot)) if slot.flat => {
            let name = rel.rsplit(['/', '\\']).next().unwrap_or(rel);
            let mut dest = String::with_capacity(slot.dir.len() + 1 + name.len());
            dest.push_str(&slot.dir);
            dest.push('/');
            dest.push_str(name);
            (dest, true)
        }
        (false, Some(slot)) => {
            let folder = own_folder.unwrap_or_default();
            let mut dest = String::with_capacity(slot.dir.len() + folder.len() + rel.len() + 2);
            dest.push_str(&slot.dir);
            dest.push('/');
            dest.push_str(folder);
            dest.push('/');
            dest.push_str(rel);
            (dest, false)
        }
        _ => (rel.to_string(), true),
    }
}

/// Picks one copy of each contested path: the user's choice, else the first mod on the list. The others become
/// losers of a conflict.
fn settle_contests(
    contests: BTreeMap<String, Contest>,
    winners: &BTreeMap<String, String>,
    plan: &mut DeployPlan,
) {
    for (key, contest) in contests {
        let mut entries = contest.entries;
        let chosen = winners.get(&key).and_then(|wanted| {
            entries.iter().position(|entry| {
                plan.source_of(entry)
                    .is_some_and(|source| source.id.as_ref() == wanted.as_str())
            })
        });
        let winner = entries.remove(chosen.unwrap_or(0));

        if !entries.is_empty() {
            let winner_id = plan.family(&winner).to_string();
            let losers: Vec<String> = entries
                .iter()
                .map(|entry| plan.family(entry).to_string())
                .collect();
            plan.conflicts.push(Conflict {
                path: contest.path,
                winner: winner_id,
                losers,
            });
        }

        plan.files.push(winner);
    }
}

/// The folder name in the game. When another family already has it, a short stable suffix keeps them apart, and a
/// number after that in the unlikely case that the suffix is taken too.
fn unique_name(base: String, family: &str, taken: &mut rustc_hash::FxHashSet<String>) -> String {
    let suffixed = format!("{base}-{}", naming::short_hash(family));
    naming::first_free(
        |number| match number {
            1 => base.clone(),
            2 => suffixed.clone(),
            _ => format!("{suffixed}-{}", number - 1),
        },
        |candidate| taken.insert(path_key(candidate)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_clash_on_the_suffixed_name_gets_the_next_number() {
        let suffix = naming::short_hash("local:second");
        let taken_names = [
            "Same".to_string(),
            format!("Same-{suffix}"),
            format!("Same-{suffix}-2"),
        ];
        let mut taken: rustc_hash::FxHashSet<String> =
            taken_names.iter().map(|name| path_key(name)).collect();
        assert_eq!(
            unique_name("Same".to_string(), "local:second", &mut taken),
            format!("Same-{suffix}-3")
        );
    }
}

#[cfg(test)]
mod problem_tests {
    use super::*;
    use crate::domain::deploy::{Deploy, Slot};
    use std::sync::Arc;

    fn deploy(flat: bool) -> Deploy {
        Deploy {
            slots: vec![Slot {
                dir: "Mods".to_string(),
                prefix: String::new(),
                marker: None,
                markers: Vec::new(),
                enable_file: None,
                extensions: Vec::new(),
                root_extensions: Vec::new(),
                root_folder_extensions: Vec::new(),
                flat,
            }],
            ..Deploy::default()
        }
    }

    fn input(id: &str, files: &[&str]) -> ModInput<'static> {
        ModInput {
            id: id.to_string().into(),
            folder: "folder".to_string().into(),
            slot: "Mods".to_string().into(),
            display_name: id.to_string().into(),
            enabled: true,
            priority: None,
            files: Arc::from(
                files
                    .iter()
                    .map(|file| (*file).to_string())
                    .collect::<Vec<_>>(),
            ),
            layout: None,
        }
    }

    #[test]
    fn two_files_of_one_mod_at_the_same_destination_are_a_problem() {
        let plan = plan_with_mode(
            &deploy(true),
            &[input("local:mod", &["a/x.pak", "b/x.pak"])],
            &BTreeMap::new(),
            PlanMode::Full,
        );
        assert!(plan.problems.iter().any(|problem| matches!(
            problem,
            Problem::DuplicateDestination { path, .. } if path == "Mods/x.pak"
        )));
        assert_eq!(plan.files.len(), 1);
    }

    #[test]
    fn a_file_and_a_folder_at_the_same_path_are_a_problem() {
        let plan = plan_with_mode(
            &deploy(false),
            &[input("local:mod", &["DLC/x", "DLC/x/y"])],
            &BTreeMap::new(),
            PlanMode::Full,
        );
        assert!(
            plan.problems
                .iter()
                .any(|problem| matches!(problem, Problem::FileFolderCollision { .. }))
        );
    }
}
