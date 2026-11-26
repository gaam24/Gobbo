//! The stores other than Steam that keep a game folder: Epic's launcher, GOG (Galaxy, its installers and minigalaxy),
//! Heroic with its Epic and GOG libraries, standalone Legendary and Lutris. Each adapter reads the files its store writes
//! and turns them into candidates; `detect` then checks the same marker files as for Steam. Where the files are on
//! this computer comes from `platform::store_folders`, so this module has no per-system code except Lutris, whose
//! database reader is Linux-only.
//!
//! A definition opts into a store with ids (`gog_ids`, `epic_ids`, `lutris_slugs`, `heroic_slugs`), so a folder with
//! the right files is never taken for a game that did not name it. A candidate says which of those lists its id
//! belongs to: a GOG game installed through Heroic or Lutris still carries its GOG product id and matches `gog_ids`.

use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;

use super::GameDef;
use crate::platform::StoreFolders;

mod epic;
pub mod gog;
mod heroic;
#[cfg(not(windows))]
mod lutris;
pub mod steam;

/// Generates the store enum and everything that reads its variants from the same list, so a new store cannot be added
/// to one of them without the others.
macro_rules! stores {
    ($(($variant:ident, $name:literal, $display:literal)),+ $(,)?) => {
        /// The stores whose install folders Gobbo can recognise, plus Steam and the folders no store claims.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
        #[serde(rename_all = "snake_case")]
        pub enum Store {
            $($variant),+,
        }

        impl Store {
            /// The stores in the order a saved source is compared with them.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The name the window and the saved settings use for this store.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name),+,
                }
            }

            /// The name shown for this store, for example in a game's subtitle.
            pub const fn display_name(self) -> &'static str {
                match self {
                    $(Self::$variant => $display),+,
                }
            }

            /// The store a saved source names, when it names one of the stores Gobbo knows.
            pub fn from_name(name: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|store| store.name() == name)
            }
        }

        impl From<&str> for Store {
            /// A saved name that names no store means a manual install, as it does for a missing source.
            fn from(name: &str) -> Self {
                Self::from_name(name).unwrap_or(Self::Manual)
            }
        }
    };
}

stores! {
    (Steam, "steam", "Steam"),
    (Gog, "gog", "GOG"),
    (Epic, "epic", "Epic"),
    (Heroic, "heroic", "Heroic"),
    (Legendary, "legendary", "Legendary"),
    (Lutris, "lutris", "Lutris"),
    (Manual, "manual", "Manual"),
}

/// The definition list a candidate's id is compared with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdKind {
    /// A GOG product id, matched against `gog_ids`.
    Gog,
    /// An Epic `AppName`, matched against `epic_ids`.
    Epic,
    /// A Lutris game slug, matched against `lutris_slugs`. Lutris is Linux-only, so Windows never has this kind.
    #[cfg(not(windows))]
    Lutris,
}

/// One folder a store says holds a game, before the markers are checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The store that manages the folder, and so owns the install.
    pub store: Store,
    pub root: PathBuf,
    /// The id the store gives the game: a GOG product id, an Epic `AppName` or a Lutris slug.
    pub id: String,
    /// Which definition list `id` belongs to.
    pub kind: IdKind,
    /// The Wine prefix the store runs this game in (the folder with `drive_c`), when the store names one.
    pub prefix: Option<PathBuf>,
}

/// Every installed game the other stores name on this computer.
pub fn candidates(home: &Path) -> Vec<Candidate> {
    candidates_in(&store_folders(home))
}

/// The folders the stores read on this computer, with minigalaxy's install folder added to the GOG libraries.
pub fn store_folders(home: &Path) -> StoreFolders {
    with_minigalaxy(crate::platform::store_folders(home))
}

/// Adds the folder minigalaxy installs GOG games into to the GOG libraries, unless it is listed already.
fn with_minigalaxy(mut folders: StoreFolders) -> StoreFolders {
    if let Some(library) = folders
        .minigalaxy
        .as_deref()
        .and_then(gog::minigalaxy_library)
        && !folders.gog_libraries.contains(&library)
    {
        folders.gog_libraries.push(library);
    }

    folders
}

/// One store whose installed folders can be turned into candidates. A new store is added by implementing this trait
/// and adding an entry to [`ADAPTERS`]; the scan itself does not change.
trait StoreAdapter: Send + Sync {
    /// The store this adapter reads.
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "the adapter registry test checks store identity; the scan only needs candidates"
        )
    )]
    fn store(&self) -> Store;

    /// The candidates this store names in `folders`, in the order detection prefers them.
    fn candidates(&self, folders: &StoreFolders) -> Vec<Candidate>;
}

/// Epic's launcher manifests and its older install list.
struct EpicAdapter;

impl StoreAdapter for EpicAdapter {
    fn store(&self) -> Store {
        Store::Epic
    }

    fn candidates(&self, folders: &StoreFolders) -> Vec<Candidate> {
        let mut out = Vec::new();
        epic::read(&mut out, folders);
        out
    }
}

/// Heroic's Epic and GOG install lists. A GOG game installed through Heroic still carries its GOG product id.
struct HeroicAdapter;

impl StoreAdapter for HeroicAdapter {
    fn store(&self) -> Store {
        Store::Heroic
    }

    fn candidates(&self, folders: &StoreFolders) -> Vec<Candidate> {
        let mut out = Vec::new();
        for heroic in &folders.heroic {
            heroic::read(&mut out, heroic);
        }
        out
    }
}

/// Standalone Legendary's Epic install list, read by Heroic's reader but owned by `Store::Legendary`.
struct LegendaryAdapter;

impl StoreAdapter for LegendaryAdapter {
    fn store(&self) -> Store {
        Store::Legendary
    }

    fn candidates(&self, folders: &StoreFolders) -> Vec<Candidate> {
        let mut out = Vec::new();
        if let Some(legendary) = &folders.legendary {
            heroic::read_legendary(&mut out, legendary);
        }
        out
    }
}

/// Lutris's database, on the platforms where Lutris runs.
#[cfg(not(windows))]
struct LutrisAdapter;

#[cfg(not(windows))]
impl StoreAdapter for LutrisAdapter {
    fn store(&self) -> Store {
        Store::Lutris
    }

    fn candidates(&self, folders: &StoreFolders) -> Vec<Candidate> {
        let mut out = Vec::new();
        for lutris in &folders.lutris {
            lutris::read(&mut out, lutris);
        }
        out
    }
}

/// GOG's registry pairs and its library folders, in that order.
struct GogAdapter;

impl StoreAdapter for GogAdapter {
    fn store(&self) -> Store {
        Store::Gog
    }

    fn candidates(&self, folders: &StoreFolders) -> Vec<Candidate> {
        let mut out = gog::registry(&folders.gog_registry);
        gog::read(&mut out, &folders.gog_libraries);
        out
    }
}

/// The stores detection reads, in the order a scan prefers when two stores claim one folder: the launchers that
/// manage an install come first, the plain `goggame-*.info` folders last. The GOG adapter keeps the registry pairs
/// before the library folders.
static ADAPTERS: &[&dyn StoreAdapter] = &[
    &EpicAdapter,
    &HeroicAdapter,
    &LegendaryAdapter,
    #[cfg(not(windows))]
    &LutrisAdapter,
    &GogAdapter,
];

/// Every installed game the stores in `folders` name. A missing or unreadable store file simply adds nothing; the order
/// is the order a scan prefers when two stores claim one folder: the launchers that manage an install come first, the
/// plain `goggame-*.info` folders last.
pub fn candidates_in(folders: &StoreFolders) -> Vec<Candidate> {
    ADAPTERS
        .iter()
        .flat_map(|adapter| adapter.candidates(folders))
        .collect()
}

/// Whether the candidate belongs to this game: the store id must be one the definition opted into, and the folder must
/// have the game's marker files.
pub fn matches(game: &GameDef, candidate: &Candidate) -> bool {
    // The cheap id check first: a candidate the game does not opt into never needs a folder stat.
    let has = |ids: &[String]| ids.iter().any(|id| id.eq_ignore_ascii_case(&candidate.id));
    let ids = match candidate.kind {
        IdKind::Gog => &game.detect.gog_ids,
        IdKind::Epic => &game.detect.epic_ids,
        #[cfg(not(windows))]
        IdKind::Lutris => &game.detect.lutris_slugs,
    };
    let opted_in = has(ids) || (candidate.store == Store::Heroic && has(&game.detect.heroic_slugs));
    opted_in && super::detect::has_markers(&candidate.root, game)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::games as test_games;
    use crate::test_support::{scratch, write};

    #[test]
    fn every_store_round_trips_through_its_name() {
        for &store in Store::ALL {
            assert_eq!(Store::from_name(store.name()), Some(store));
            assert!(!store.display_name().is_empty(), "{store:?}");
        }
    }

    #[test]
    fn the_adapter_registry_names_each_candidate_store_once_in_scan_order() {
        let stores: Vec<_> = ADAPTERS.iter().map(|adapter| adapter.store()).collect();

        // An adapter names a store Gobbo knows, and no two adapters name the same one.
        for &store in &stores {
            assert!(Store::ALL.contains(&store), "{store:?}");
            assert_eq!(
                stores.iter().filter(|found| **found == store).count(),
                1,
                "{store:?} has more than one adapter"
            );
        }

        // Steam has its own detection and Manual is the fallback when no store is named, so neither is an adapter.
        assert!(!stores.contains(&Store::Steam));
        assert!(!stores.contains(&Store::Manual));

        // Every store that can produce candidates has one adapter, in the order a scan prefers them.
        let mut expected = vec![Store::Epic, Store::Heroic, Store::Legendary];
        #[cfg(not(windows))]
        expected.push(Store::Lutris);
        expected.push(Store::Gog);
        assert_eq!(stores, expected);
    }

    /// An install with the fixture's markers, so a candidate matches when its id is opted into.
    fn install(root: &Path) {
        write(&root.join("bin/game.exe"), "");
        write(&root.join("content/base/x.txt"), "");
    }

    /// A candidate of `store` whose id belongs to `kind`.
    fn candidate(store: Store, kind: IdKind, root: &Path, id: &str) -> Candidate {
        Candidate {
            store,
            root: root.to_path_buf(),
            id: id.into(),
            kind,
            prefix: None,
        }
    }

    #[test]
    fn a_candidate_needs_both_the_opted_in_id_and_the_game_files() {
        let base = scratch!("stores-matches");
        let root = base.join("Games/Slot");
        install(&root);
        let found = candidate(Store::Gog, IdKind::Gog, &root, "SlotId");
        let mut game = test_games::slot();
        game.detect.gog_ids = vec!["SlotId".into()];
        assert!(matches(&game, &found));

        game.detect.gog_ids.clear();
        assert!(!matches(&game, &found));

        game.detect.gog_ids = vec!["SlotId".into()];
        let empty = base.join("Games/Empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(!matches(
            &game,
            &candidate(Store::Gog, IdKind::Gog, &empty, "SlotId")
        ));
    }

    #[test]
    fn a_launcher_install_matches_the_list_of_the_store_that_sold_the_game() {
        let base = scratch!("stores-kinds");
        let root = base.join("Games/Slot");
        install(&root);
        let mut game = test_games::slot();
        game.detect.gog_ids = vec!["123".into()];
        game.detect.epic_ids = vec!["SlotApp".into()];

        // A GOG game through Heroic or Lutris is matched by its GOG product id, an Epic game by its AppName.
        assert!(matches(
            &game,
            &candidate(Store::Heroic, IdKind::Gog, &root, "123")
        ));
        assert!(matches(
            &game,
            &candidate(Store::Lutris, IdKind::Gog, &root, "123")
        ));
        assert!(matches(
            &game,
            &candidate(Store::Heroic, IdKind::Epic, &root, "SlotApp")
        ));
        // An id never matches the list of another kind.
        assert!(!matches(
            &game,
            &candidate(Store::Heroic, IdKind::Epic, &root, "123")
        ));
        assert!(!matches(
            &game,
            &candidate(Store::Heroic, IdKind::Gog, &root, "SlotApp")
        ));

        // `heroic_slugs` names a Heroic app whose id is in no store list.
        game.detect.heroic_slugs = vec!["sideloaded".into()];
        assert!(matches(
            &game,
            &candidate(Store::Heroic, IdKind::Epic, &root, "sideloaded")
        ));
        assert!(!matches(
            &game,
            &candidate(Store::Epic, IdKind::Epic, &root, "sideloaded")
        ));
    }

    #[test]
    fn a_scan_takes_a_store_candidate_only_when_the_game_opts_into_its_id() {
        let base = scratch!("stores-scan");
        let root = base.join("Epic Games/Slot");
        install(&root);
        let folders = StoreFolders {
            epic_list: Some(base.join("UnrealEngineLauncher")),
            ..StoreFolders::default()
        };
        write(
            &base.join("UnrealEngineLauncher/LauncherInstalled.dat"),
            &serde_json::json!({
                "InstallationList": [{
                    "AppName": "SlotApp",
                    "InstallLocation": root.display().to_string(),
                }],
            })
            .to_string(),
        );
        let mut game = test_games::slot();
        game.detect.epic_ids = vec!["SlotApp".into()];

        let scans = super::super::detect::scan_in(&[], &[game.clone()], &folders);
        let found = scans[0].found.as_ref().expect("the Epic install is found");
        assert_eq!(found.store, "epic");
        assert_eq!(found.game_root, root.display().to_string());

        game.detect.epic_ids.clear();
        let scans = super::super::detect::scan_in(&[], &[game], &folders);
        assert!(scans[0].found.is_none());
    }

    #[test]
    fn the_minigalaxy_install_folder_joins_the_gog_libraries_once() {
        let base = scratch!("stores-minigalaxy");
        let config = base.join("minigalaxy/config.json");
        let library = base.join("Mini Games");
        write(
            &config,
            &serde_json::json!({ "install_dir": library.display().to_string() }).to_string(),
        );
        let folders = with_minigalaxy(StoreFolders {
            minigalaxy: Some(config),
            ..StoreFolders::default()
        });
        assert_eq!(folders.gog_libraries, std::slice::from_ref(&library));
        assert_eq!(with_minigalaxy(folders).gog_libraries, [library]);
    }
}
