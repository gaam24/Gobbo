//! Finding the mod that breaks the game, by halves. The enabled mods are the suspects. Each test keeps one half of them
//! on and the other off; the user starts the game and says whether the problem is still there, and the half that holds
//! it becomes the new suspects. A list of 200 mods takes about 8 tests. The mods a tested mod requires stay on with it,
//! so a test never fails only because a requirement is off. At the end the list goes back to how it was.

use crate::context::Data;
use crate::context::Loaded;
use crate::context::load_game;
use crate::context::load_profiles;
use crate::context::profile::bisect::{Search, file, read};
use crate::context::profile::history::save_profiles;
use crate::context::profile::lock_profiles;
use crate::domain::profiles::Profile;
use crate::error::AppError;
use crate::error::io_at;
use crate::games;
use crate::infra::json;
use crate::library::Library;
use std::collections::BTreeSet;
use std::fs;

/// Starts a search over the enabled mods of the active profile, and sets up the first test.
pub fn bisect_start(root: &Data, game_id: &str) -> Result<(), AppError> {
    let data = root.game(game_id);
    let Loaded { library, .. } = load_game(root, game_id)?;
    let _profiles = lock_profiles();
    if read(&data).is_some() {
        return Err(AppError::user("bisect.running"));
    }
    let mut profiles = load_profiles(&data)?;
    let profile = profiles.active();
    let suspects: Vec<String> = profile
        .enabled_mods()
        .into_iter()
        .map(|entry| entry.id)
        .collect();
    if suspects.len() < 2 {
        return Err(AppError::user("bisect.not_enough"));
    }
    let mut search = Search {
        profile: profile.id.clone(),
        original: profile
            .mods
            .iter()
            .map(|entry| (entry.id.clone(), entry.enabled))
            .collect(),
        testing: Vec::new(),
        suspects,
        step: 0,
    };
    next_test(&mut search, &library, profiles.active_mut());
    json::write_json(&file(&data), &search)?;
    // The list before the search can be restored from the history.
    save_profiles(&data, &profiles)
}

/// The answer to a test: whether the problem was there. The half that holds it becomes the suspects.
pub fn bisect_answer(root: &Data, game_id: &str, broken: bool) -> Result<(), AppError> {
    let data = root.game(game_id);
    let Loaded { library, .. } = load_game(root, game_id)?;
    let _profiles = lock_profiles();
    let mut search = read(&data).ok_or_else(not_running)?;
    if search.suspects.len() < 2 {
        return Err(AppError::user("bisect.done"));
    }
    let testing: BTreeSet<&String> = search.testing.iter().collect();
    let (on, off): (Vec<String>, Vec<String>) = search
        .suspects
        .iter()
        .cloned()
        .partition(|id| testing.contains(id));
    search.suspects = if broken { on } else { off };
    let mut profiles = load_profiles(&data)?;
    let profile = profiles.get_mut(&search.profile)?;
    next_test(&mut search, &library, profile);
    json::write_json(&file(&data), &search)?;
    data.save_profiles(&profiles)
}

/// Ends the search and puts the list back as it was. With `disable_found`, the mod that holds the problem stays off.
pub fn bisect_stop(root: &Data, game_id: &str, disable_found: bool) -> Result<(), AppError> {
    games::load(game_id)?;
    let data = root.game(game_id);
    let _profiles = lock_profiles();
    let search = read(&data).ok_or_else(not_running)?;
    let mut profiles = load_profiles(&data)?;
    if let Ok(profile) = profiles.get_mut(&search.profile) {
        let found =
            (disable_found && search.suspects.len() == 1).then(|| search.suspects[0].clone());
        for entry in &mut profile.mods {
            if let Some((_, enabled)) = search.original.iter().find(|(id, _)| *id == entry.id) {
                entry.enabled = *enabled && found.as_ref() != Some(&entry.id);
            }
        }
        data.save_profiles(&profiles)?;
    }
    fs::remove_file(file(&data)).map_err(io_at(&file(&data)))
}

/// Sets up the next test: the first half of the suspects on, the other half off, the mods already cleared on, and the
/// requirements of every mod that is on switched on with it. With one suspect left the list keeps the last test.
fn next_test(search: &mut Search, library: &Library, profile: &mut Profile) {
    if search.suspects.len() < 2 {
        return;
    }
    search.step += 1;
    let half = search.suspects.len().div_ceil(2);
    search.testing = search.suspects[..half].to_vec();
    let suspects: BTreeSet<&String> = search.suspects.iter().collect();
    let originally_on: BTreeSet<&String> = search
        .original
        .iter()
        .filter(|(_, enabled)| *enabled)
        .map(|(id, _)| id)
        .collect();
    let mut on: BTreeSet<String> = originally_on
        .iter()
        .filter(|id| !suspects.contains(*id))
        .map(|id| (*id).clone())
        .chain(search.testing.iter().cloned())
        .collect();
    // Requirements follow the mods that need them, as far as they were on before the search.
    let mut queue: Vec<String> = on.iter().cloned().collect();
    while let Some(id) = queue.pop() {
        let requires = library
            .families
            .get(&id)
            .map(|family| family.requires.as_slice())
            .unwrap_or_default();
        for required in requires {
            if originally_on.contains(required) && on.insert(required.clone()) {
                queue.push(required.clone());
            }
        }
    }
    for entry in &mut profile.mods {
        entry.enabled = on.contains(&entry.id);
    }
}

fn not_running() -> AppError {
    AppError::user("bisect.not_running")
}
