//! Fixtures for `benches/backend.rs`. Compiled only with the `bench` feature, so no benchmark state or fixture code
//! reaches a release build.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::Sha256;

use crate::context::Data;
use crate::deploy::{Target, apply, plan_with_mode as deploy_plan, undo};
use crate::domain::profiles::ListState;
use crate::domain::profiles::Profiles;
use crate::domain::profiles::entry::ProfileMod;
use crate::domain::resolver::DeployPlan;
use crate::domain::source::{ProviderId, SourceMeta, SourceMod};
use crate::games::{Deploy, GameDef};
use crate::infra::archive::Archive;
use crate::infra::io::hash_and_copy;
use crate::infra::store::unique_suffix;
use crate::launch::watch;
use crate::library::{Family, Library, ModInfo, ModInfoEdit, ModVersion};
use crate::settings::config::{Settings, save_settings};
use crate::status::sync_profiles as status;

pub use crate::domain::pathkey::path_key;
pub use crate::domain::profiles::history::summarize as history_summary;
pub use crate::domain::resolver::{ModInput, PlanMode, plan_with_mode};
pub use crate::domain::version::is_newer;
pub use crate::import::fomod::{SourcePattern, normalize_path};
pub use crate::infra::text::tail_lines;
pub use crate::library::LibraryView;
pub use crate::packs::inspect::bench::PackFixture;
pub use faster_hex::hex_string as hex;

/// A list with room for `n` mods and nothing else in it yet.
fn empty_list_state(n: usize) -> ListState {
    ListState {
        mods: Vec::with_capacity(n),
        separators: Vec::new(),
        winners: BTreeMap::new(),
        waiting: Vec::new(),
        groups: Vec::new(),
    }
}

/// Two states of a list of `n` families, the second with every other mod switched off, for the history summary
/// benchmark.
pub fn history_states(n: usize) -> (ListState, ListState) {
    let mut old = empty_list_state(n);
    let mut now = empty_list_state(n);

    for index in 0..n {
        let entry = |enabled| crate::domain::profiles::Entry {
            id: format!("nexus:{index}"),
            folder: format!("mod{index}"),
            enabled,
            priority: None,
        };
        old.mods.push(entry(true));
        now.mods.push(entry(index % 2 == 0));
    }

    (old, now)
}

/// A library with `mods` independent families, each with `files` files. The paths are deliberately outside the shared
/// script kinds of the Witcher definition, so `PlanMode::Summary` can skip them.
pub fn synthetic_library(mods: usize, files: usize) -> (Library, Vec<ProfileMod>) {
    let mut library = Library::default();
    let mut profile = Vec::with_capacity(mods);

    for m in 0..mods {
        let folder = format!("mod{m}");
        let mut paths: Vec<String> = (0..files)
            .map(|f| format!("content/mesh/mod_{m}/file_{f}.bin"))
            .collect();
        // The library keeps file lists sorted; the numeric fixture names are not in lexical order.
        paths.sort_unstable();
        library.versions.insert(
            folder.clone(),
            ModVersion {
                folder: folder.clone(),
                family: format!("nexus:{m}"),
                slot: "Mods".into(),
                files: Arc::from(paths),
                size_bytes: 1,
                info: ModInfo {
                    name: folder.clone(),
                    ..ModInfo::default()
                },
                edit: ModInfoEdit::default(),
                archive: None,
                parsed_sources: BTreeMap::from([(
                    ProviderId::new("nexus"),
                    (m as u32).to_string(),
                )]),
                parsed_parts: BTreeMap::new(),
                installed_at: 0,
                fomod: Vec::new(),
                fomod_installer: false,
                integrity: None,
            },
        );
        library.families.insert(
            format!("nexus:{m}"),
            Family {
                id: format!("nexus:{m}"),
                name: folder.clone(),
                ..Family::default()
            },
        );
        profile.push(ProfileMod {
            id: format!("nexus:{m}"),
            folder,
            slot: "Mods".into(),
            enabled: true,
            priority: None,
        });
    }

    (library, profile)
}

/// A library like `synthetic_library`, but with `versions` versions of every family and an optional description on
/// each version and Nexus record.
///
/// Real libraries have several versions and long page texts, and the list payload grows with both.
pub fn synthetic_library_realistic(
    mods: usize,
    files: usize,
    versions: usize,
    description_bytes: usize,
) -> (Library, Vec<ProfileMod>) {
    let versions = versions.max(1);
    let mut library = Library::default();
    let mut profile = Vec::with_capacity(mods);

    for m in 0..mods {
        let family = format!("nexus:{m}");
        let mut selected = None;

        for v in 0..versions {
            let folder = if versions == 1 {
                format!("mod{m}")
            } else {
                format!("mod{m}_v{v}")
            };
            let mut paths: Vec<String> = (0..files)
                .map(|f| format!("content/mesh/mod_{m}/v{v}/file_{f}.bin"))
                .collect();
            paths.sort_unstable();
            let description = (description_bytes > 0).then(|| "x".repeat(description_bytes));
            library.versions.insert(
                folder.clone(),
                ModVersion {
                    folder: folder.clone(),
                    family: family.clone(),
                    slot: "Mods".into(),
                    files: Arc::from(paths),
                    size_bytes: 1,
                    info: ModInfo {
                        name: format!("Mod {m}"),
                        version: Some(format!("1.{v}")),
                        author: Some(format!("Author {m}")),
                        description: description.clone(),
                    },
                    edit: ModInfoEdit::default(),
                    archive: None,
                    parsed_sources: BTreeMap::from([(
                        ProviderId::new("nexus"),
                        (m as u32).to_string(),
                    )]),
                    parsed_parts: BTreeMap::new(),
                    installed_at: v as u32,
                    fomod: Vec::new(),
                    fomod_installer: false,
                    integrity: None,
                },
            );

            if v == 0 {
                selected = Some(folder);
            }
        }

        library.families.insert(
            family.clone(),
            Family {
                id: family.clone(),
                name: format!("Mod {m}"),
                tags: (0..3).map(|tag| format!("tag-{tag}")).collect(),
                notes: "A note".into(),
                instructions: "Install it.".into(),
                requires: Vec::new(),
                incompatible: Vec::new(),
                overrides: Vec::new(),
                sources: BTreeMap::from([(
                    ProviderId::new("nexus"),
                    SourceMod {
                        mod_id: m.to_string(),
                        part_id: None,
                        part_name: None,
                        part_category: None,
                        meta: Some(SourceMeta::from(crate::providers::nexus::NexusMeta {
                            name: Some(format!("Mod {m}")),
                            author: Some(format!("Author {m}")),
                            summary: Some("A mod summary".into()),
                            description: (description_bytes > 0)
                                .then(|| "x".repeat(description_bytes)),
                            tags: vec!["tag".into()],
                            ..crate::providers::nexus::NexusMeta::default()
                        })),
                    },
                )]),
                ..Family::default()
            },
        );
        profile.push(ProfileMod {
            id: family,
            folder: selected.expect("a version is selected"),
            slot: "Mods".into(),
            enabled: true,
            priority: None,
        });
    }

    (library, profile)
}

/// One family with `versions` installed versions, each with `files` files.
pub fn synthetic_family(versions: usize, files: usize) -> Library {
    let mut library = Library::default();

    for v in 0..versions {
        let folder = format!("mod{v}");
        let mut paths: Vec<String> = (0..files)
            .map(|f| format!("content/scripts/file_{v}_{f}.ws"))
            .collect();
        paths.sort_unstable();
        library.versions.insert(
            folder.clone(),
            ModVersion {
                folder: folder.clone(),
                family: "nexus:1".into(),
                slot: "Mods".into(),
                files: Arc::from(paths),
                size_bytes: 1,
                info: ModInfo {
                    name: folder,
                    version: Some(format!("1.{v}")),
                    ..ModInfo::default()
                },
                edit: ModInfoEdit::default(),
                archive: None,
                parsed_sources: BTreeMap::from([(ProviderId::new("nexus"), "1".to_string())]),
                parsed_parts: BTreeMap::new(),
                installed_at: v as u32,
                fomod: Vec::new(),
                fomod_installer: false,
                integrity: None,
            },
        );
    }

    library
}

/// A library serialized as compact JSON, for the JSON benchmarks.
pub fn library_json(library: &Library) -> String {
    serde_json::to_string(library).expect("the benchmark JSON")
}

/// Parses a serialized library and returns how many versions it holds.
pub fn parse_library(text: &str) -> usize {
    serde_json::from_str::<Library>(text)
        .expect("the benchmark JSON")
        .versions
        .len()
}

/// Prepared FOMOD patterns and entry names, for the matcher benchmark.
pub struct FomodFixture {
    patterns: Vec<SourcePattern>,
    entries: Vec<String>,
}

impl FomodFixture {
    pub fn new(patterns: usize, entries: usize) -> Self {
        let prepared = (0..patterns)
            .map(|number| SourcePattern::new(&format!("textures/2k_{number}/*.dds")))
            .collect();
        let entries = (0..entries)
            .map(|number| format!("textures/2k_{}/file_{number}.dds", number % patterns))
            .collect();
        Self {
            patterns: prepared,
            entries,
        }
    }

    /// Counts the entries that at least one pattern takes.
    pub fn matches(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| self.patterns.iter().any(|pattern| pattern.matches(entry)))
            .count()
    }
}

/// A `size`-byte archive to read and an empty target to copy it to, both under the temporary folder.
pub fn archive_fixture(size: usize) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("gobbo-bench-archive-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the benchmark folder");
    let source = dir.join("source.zip");

    if std::fs::metadata(&source).map_or(0, |meta| meta.len()) != size as u64 {
        std::fs::write(&source, vec![0x5a_u8; size]).expect("the benchmark archive");
    }

    (source, dir.join("target.zip"))
}

/// Stores an archive the way an import does: the digest of the source, and a copy for `archives/`.
pub fn store_archive(source: &Path, target: &Path) -> String {
    hash_and_copy::<Sha256>(source, target, &mut |_| {}).expect("the benchmark copy")
}

/// A game data folder with a `library.json`, a `profiles.json` and a `config.json`, and the folders of the installed
/// mods, so `crate::status` runs its real path instead of a copy of it.
pub struct StatusFixture {
    dir: std::path::PathBuf,
    root: Data,
}

impl StatusFixture {
    /// `mods` families with `files` files each, all enabled in the one profile.
    pub fn new(mods: usize, files: usize) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gobbo-bench-status-{}-{mods}-{files}",
            std::process::id()
        ));
        Self::from_library(dir, synthetic_library(mods, files))
    }

    /// A status fixture with several versions per family and long descriptions, like a real Nexus library. The
    /// default fixture has one short version per family, which hides the version and payload costs.
    pub fn realistic(mods: usize, files: usize, versions: usize, description_bytes: usize) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gobbo-bench-status-real-{}-{mods}-{files}-{versions}-{description_bytes}",
            std::process::id()
        ));
        Self::from_library(
            dir,
            synthetic_library_realistic(mods, files, versions, description_bytes),
        )
    }

    fn from_library(
        dir: std::path::PathBuf,
        (library, profile): (Library, Vec<ProfileMod>),
    ) -> Self {
        let _ = std::fs::remove_dir_all(&dir);
        let root = Data::new(dir.clone());
        let game = root.game("the-witcher-3-wild-hunt");
        let library_dir = game.dir.join("library");
        std::fs::create_dir_all(&library_dir).expect("the benchmark library");

        for folder in library.versions.keys() {
            std::fs::create_dir_all(library_dir.join(folder)).expect("a benchmark mod folder");
        }

        library.save(&game.index()).expect("the benchmark index");
        let entries = profile
            .into_iter()
            .map(|mod_entry| crate::domain::profiles::Entry {
                id: mod_entry.id,
                folder: mod_entry.folder,
                enabled: mod_entry.enabled,
                priority: mod_entry.priority,
            })
            .collect();
        let mut profiles = Profiles::with_default(entries);
        let mods = profiles.active().mods.clone();
        profiles.active_mut().deployed_mods = mods;
        profiles.active_mut().deployed_at = Some(1);
        profiles
            .save(&game.profiles_file())
            .expect("the benchmark profiles");
        let game_root = dir.join("game");
        let settings_dir = dir.join("settings");
        std::fs::create_dir_all(&game_root).expect("the benchmark game folder");
        std::fs::create_dir_all(&settings_dir).expect("the benchmark settings folder");
        save_settings(
            &root,
            "the-witcher-3-wild-hunt",
            Settings {
                game_root: game_root.display().to_string(),
                settings_dir: settings_dir.display().to_string(),
                ..Settings::default()
            },
        )
        .expect("the benchmark settings");
        Self { dir, root }
    }

    /// Writes a journal for every installed version, and its folder in the game's `Mods` slot. This is the state a
    /// deployed game has: the status has to read the journal and list the slot on every call.
    pub fn deployed(self) -> Self {
        let game = self.root.game("the-witcher-3-wild-hunt");
        let library = self.loaded_library();
        let definition =
            crate::games::load("the-witcher-3-wild-hunt").expect("the benchmark definition");
        let slot = definition
            .deploy
            .slot("Mods")
            .expect("the Witcher definition has a Mods slot");
        let state = game.dir.join("state").join("p1");
        std::fs::create_dir_all(&state).expect("the benchmark state");
        let journal = crate::deploy::journal::Journal::new(state.join("journal.jsonl"));
        let slot_root = self.dir.join("game").join(&slot.dir);

        for (folder, version) in &library.versions {
            let own = crate::domain::naming::deploy_name(&slot.prefix, &version.info.name, folder);
            let dir = slot_root.join(&own);
            std::fs::create_dir_all(&dir).expect("a deployed mod folder");
            journal
                .append(&crate::deploy::journal::Entry::DirCreated { path: dir.clone() })
                .expect("the benchmark journal folder");
            for rel in version.files.iter() {
                journal
                    .append(&crate::deploy::journal::Entry::FileDeployed {
                        path: dir.join(rel),
                    })
                    .expect("the benchmark journal file");
            }
        }

        drop(journal);
        self
    }

    /// Runs the status command and returns how many mods it listed.
    pub fn status(&self) -> usize {
        self.status_value().mods.len()
    }

    /// The status value, for the serialization benchmark.
    pub fn status_value(&self) -> crate::status::views::Status {
        status(&self.root, "the-witcher-3-wild-hunt").expect("the benchmark status")
    }

    /// Reads and parses the library index, as every status does.
    pub fn load_library(&self) -> usize {
        self.loaded_library().versions.len()
    }

    /// The parsed library index, for the cache and clone benchmarks.
    pub fn loaded_library(&self) -> Library {
        let game = crate::games::load("the-witcher-3-wild-hunt").expect("the benchmark definition");
        crate::context::load_library(&self.root.game("the-witcher-3-wild-hunt"), &game)
            .expect("the index")
    }

    /// Lists the folders under `library/`, as every status does to spot folders added by hand.
    pub fn mod_folders(&self) -> usize {
        crate::context::library::mod_folders(
            &self
                .root
                .game("the-witcher-3-wild-hunt")
                .dir
                .join("library"),
        )
        .expect("the library folders")
        .len()
    }
}

impl StatusFixture {
    /// The same files with the app's library index cache enabled.
    pub fn with_cache(&self) -> Self {
        Self {
            dir: self.dir.clone(),
            root: Data::with_cache(self.dir.clone()),
        }
    }

    /// Saves the index, then reads the status, as a command that changed a mod does.
    pub fn save_and_status(&self, library: Library) -> usize {
        self.root
            .game("the-witcher-3-wild-hunt")
            .save_library(library)
            .expect("the benchmark save");
        self.status()
    }
}

impl Drop for StatusFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A pseudo-random payload of `bytes` bytes for one benchmark entry, so the data does not compress away.
fn payload(index: usize, bytes: usize) -> Vec<u8> {
    let mut payload = vec![0_u8; bytes];
    let mut state = u32::try_from(index)
        .unwrap_or(u32::MAX)
        .wrapping_mul(2_654_435_761);
    for byte in &mut payload {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *byte = (state >> 24) as u8;
    }

    payload
}

/// A zip of `files` entries with `bytes` bytes each, and a fresh game data folder for every import.
pub struct ImportFixture {
    dir: std::path::PathBuf,
    source: std::path::PathBuf,
}

impl ImportFixture {
    /// Writes one mod with `files` files of `bytes` bytes into a deflated zip.
    pub fn new(files: usize, bytes: usize) -> Self {
        use std::io::Write as _;

        let dir = std::env::temp_dir().join(format!(
            "gobbo-bench-import-{}-{files}-{bytes}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the benchmark folder");
        let source = dir.join("bench.zip");
        let file = std::fs::File::create(&source).expect("the benchmark archive");
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for index in 0..files {
            writer
                .start_file(format!("Mods/bench/content/file_{index}.bin"), options)
                .expect("a benchmark archive entry");
            // A pseudo-random payload does not compress away, so the extraction reads a realistic amount of data.
            writer
                .write_all(&payload(index, bytes))
                .expect("a benchmark file");
        }

        writer.finish().expect("the benchmark archive");
        Self { dir, source }
    }

    /// Writes the same mod into a non-solid 7z archive (COPY, no compression) so the extraction can be measured.
    pub fn new_7z(files: usize, bytes: usize) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gobbo-bench-import-7z-{}-{files}-{bytes}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the benchmark folder");
        let source = dir.join("bench.7z");
        let mut writer =
            sevenz_rust2::ArchiveWriter::create(&source).expect("the benchmark archive");
        writer.set_content_methods(vec![sevenz_rust2::EncoderConfiguration::new(
            sevenz_rust2::EncoderMethod::COPY,
        )]);

        for index in 0..files {
            let entry = sevenz_rust2::ArchiveEntry::new_file(&format!(
                "Mods/bench/content/file_{index}.bin"
            ));
            writer
                .push_archive_entry(entry, Some(std::io::Cursor::new(payload(index, bytes))))
                .expect("a benchmark archive entry");
        }

        writer.finish().expect("the benchmark archive");
        Self { dir, source }
    }

    /// The archive every import of this fixture reads.
    pub fn source(&self) -> &Path {
        &self.source
    }

    /// A data folder for one import. Its drop removes the imported folders, so the next import starts empty.
    pub fn folder(&self) -> ImportFolder {
        let dir = self.dir.join(format!("data-{}", unique_suffix()));
        ImportFolder {
            root: Data::new(dir),
        }
    }
}

impl Drop for ImportFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// One fresh game data folder for the import benchmark. Dropping it removes everything the import wrote.
pub struct ImportFolder {
    root: Data,
}

impl ImportFolder {
    /// Imports the fixture archive into this folder and returns how many files landed.
    pub fn import(&self, source: &Path) -> usize {
        crate::import::import_archive(
            &self.root,
            "the-witcher-3-wild-hunt",
            crate::import::ImportRequest::new(source),
            None,
            &mut |_, _| {},
        )
        .expect("the benchmark import")
        .files as usize
    }
}

impl Drop for ImportFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root.dir);
    }
}

/// A zip with `mods` mod folders of `files` files, opened once, for the layout pass of an import without its I/O.
pub struct ImportPlanFixture {
    dir: std::path::PathBuf,
    archive: Archive,
    game: GameDef,
}

impl ImportPlanFixture {
    pub fn new(mods: usize, files: usize) -> Self {
        use std::io::Write as _;

        let dir = std::env::temp_dir().join(format!(
            "gobbo-bench-plan-{}-{mods}-{files}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the benchmark folder");
        let path = dir.join("plan.zip");
        let file = std::fs::File::create(&path).expect("the benchmark archive");
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for index in 0..mods {
            for number in 0..files {
                writer
                    .start_file(
                        format!("Mods/mod{index}/content/file_{number}.bin"),
                        options,
                    )
                    .expect("a benchmark archive entry");
                writer.write_all(b"x").expect("a benchmark file");
            }

            writer
                .start_file(format!("DLC/dlc{index}/content/file.bin"), options)
                .expect("a benchmark archive entry");
            writer.write_all(b"x").expect("a benchmark file");
        }

        writer.finish().expect("the benchmark archive");
        Self {
            dir,
            archive: Archive::open(&path).expect("the benchmark archive"),
            game: GameDef::parse(include_str!(
                "../../games/the-witcher-3-wild-hunt/game.toml"
            ))
            .expect("the benchmark definition"),
        }
    }

    /// Runs the layout pass over every entry.
    pub fn plan(&self) -> usize {
        crate::import::install::plan(&self.archive, &self.game, "bench", None).map_or(0, |_| 1)
    }
}

impl Drop for ImportPlanFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A library and a game folder with a full deployment plan, for `apply` and `undo` on real files.
pub struct DeployFixture {
    dir: std::path::PathBuf,
    library: std::path::PathBuf,
    target: Target,
    game: GameDef,
    /// Owned names: the fixture keeps the library and the profile next to it, and a plan input may not borrow from a
    /// field of the same struct.
    inputs: Vec<crate::domain::resolver::ModInput<'static>>,
    plan: DeployPlan,
    records: Library,
    profile: Vec<ProfileMod>,
}

impl DeployFixture {
    /// `mods` mod folders with `files` files each, and an overlay plan for them.
    pub fn new(mods: usize, files: usize) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gobbo-bench-deploy-{}-{mods}-{files}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let (records, profile) = synthetic_library(mods, files);
        let library_dir = dir.join("library");

        for (folder, version) in &records.versions {
            for relative in version.files.iter() {
                let path = library_dir.join(folder).join(relative);
                std::fs::create_dir_all(path.parent().expect("a parent folder"))
                    .expect("a benchmark mod folder");
                std::fs::write(&path, b"x").expect("a benchmark mod file");
            }
        }

        let game_root = dir.join("game");
        std::fs::create_dir_all(&game_root).expect("the benchmark game folder");
        let target = Target {
            root: game_root.clone(),
            game_root,
            settings: None,
            state: dir.join("state"),
            user_data: Vec::new(),
        };
        let game = GameDef::parse(include_str!(
            "../../games/the-witcher-3-wild-hunt/game.toml"
        ))
        .expect("the definition");
        let deploy = game.deploy.clone();
        let inputs: Vec<crate::domain::resolver::ModInput<'static>> = records
            .inputs(&profile)
            .into_iter()
            .map(|input| crate::domain::resolver::ModInput {
                id: std::borrow::Cow::Owned(input.id.into_owned()),
                folder: std::borrow::Cow::Owned(input.folder.into_owned()),
                slot: std::borrow::Cow::Owned(input.slot.into_owned()),
                display_name: std::borrow::Cow::Owned(input.display_name.into_owned()),
                enabled: input.enabled,
                priority: input.priority,
                files: input.files,
                // The benchmark fixture keeps the plan path without a profile, as a library built in memory does.
                layout: None,
            })
            .collect();
        let plan = deploy_plan(
            &deploy,
            &inputs,
            &std::collections::BTreeMap::new(),
            PlanMode::Full,
        );
        Self {
            dir,
            library: library_dir,
            target,
            game,
            inputs,
            plan,
            records,
            profile,
        }
    }

    /// Rescans the enabled mod folders, as every deployment does before it writes.
    pub fn refresh(&self) -> usize {
        let mut library = self.records.clone();
        library
            .refresh(&self.library, &self.game, &self.profile)
            .expect("the benchmark refresh");
        library
            .versions
            .values()
            .map(|version| version.files.len())
            .sum()
    }

    /// Builds the deployment plan again, as a deploy does before it writes.
    pub fn plan(&self) -> usize {
        deploy_plan(
            &self.game.deploy,
            &self.inputs,
            &std::collections::BTreeMap::new(),
            PlanMode::Full,
        )
        .files
        .len()
    }

    /// Applies the plan and undoes it, so the next iteration starts from an empty game folder.
    pub fn cycle(&self) -> usize {
        self.undo();
        let report =
            apply(&self.plan, &self.library, &self.target).expect("the benchmark deployment");
        undo(&self.target.journal_roots()).expect("the benchmark undo");
        report.files
    }

    /// Applies the plan alone.
    pub fn apply(&self) -> usize {
        apply(&self.plan, &self.library, &self.target)
            .expect("the benchmark deployment")
            .files
    }

    /// Undoes the last deployment alone. A missing journal is a no-op, so the setup of a benchmark can call it.
    pub fn undo(&self) -> usize {
        match undo(&self.target.journal_roots()) {
            Ok(warnings) => warnings.len(),
            Err(crate::deploy::DeployError::NothingToUndo) => 0,
            Err(error) => panic!("the benchmark undo: {error}"),
        }
    }
}

impl Drop for DeployFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A game folder with `files` files, a snapshot before a change and one after it. Building 50 000 files is slow, so
/// the harness runs this only when `GOBBO_BENCH_WATCH` is set.
pub struct WatchFixture {
    dir: PathBuf,
    before: watch::Snapshot,
    after: watch::Snapshot,
    expected: Vec<String>,
}

impl WatchFixture {
    pub fn new(files: usize) -> Self {
        let dir =
            std::env::temp_dir().join(format!("gobbo-bench-watch-{}-{files}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        for number in 0..files {
            let folder = dir.join(format!("d{}", number / 1_000));
            std::fs::create_dir_all(&folder).expect("a benchmark watch folder");
            std::fs::write(folder.join(format!("f{number}.bin")), b"x")
                .expect("a benchmark watch file");
        }

        let before = watch::take(&dir);
        std::fs::write(dir.join("d0/f0.bin"), b"changed").expect("a changed benchmark file");
        std::fs::write(dir.join("new.bin"), b"new").expect("a new benchmark file");
        let after = watch::take(&dir);
        Self {
            dir,
            before,
            after,
            expected: Vec::new(),
        }
    }

    /// Records the folder again.
    pub fn take(&self) -> watch::Snapshot {
        watch::take(&self.dir)
    }

    /// Compares the two snapshots.
    pub fn changes(&self) -> usize {
        watch::changes(&self.before, &self.after, &self.expected).len()
    }
}

impl Drop for WatchFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The deployment rules of the shipped Witcher 3 definition.
pub fn witcher_deploy() -> Deploy {
    GameDef::parse(include_str!(
        "../../games/the-witcher-3-wild-hunt/game.toml"
    ))
    .expect("the shipped Witcher 3 definition must parse")
    .deploy
}
