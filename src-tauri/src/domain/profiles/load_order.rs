//! The load order the user builds in its own window. Only the mods the user puts in the order get a priority; every other
//! mod keeps none, so the game orders it itself. The first mod of the order gets the lowest number the game reads, and
//! each next one the number after it. The list of mods does not decide the order: the user does.

use std::collections::BTreeSet;

use crate::domain::profiles::ListedMod;
use crate::error::AppError;

/// The priorities a game reads, `min..=max`, as its definition names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriorityRange {
    pub min: u32,
    pub max: u32,
}

impl PriorityRange {
    /// How many mods the range can number, `max - min + 1`. A definition with `min >= max` is refused when it loads.
    pub fn capacity(self) -> usize {
        usize::try_from(self.max.saturating_sub(self.min))
            .unwrap_or(usize::MAX)
            .saturating_add(1)
    }
}

/// Sets the priorities of the profile's mods from `order`: the first id gets `range.min`, the next `range.min + 1`, and
/// so on. A mod that is not in `order` loses its priority. Nothing changes when an id repeats, names a mod the profile
/// does not have, names a mod that `orderable` refuses (the game reads no number for it), or the range has too few
/// numbers for the order.
pub fn assign_priorities(
    mods: &mut [ListedMod],
    order: &[String],
    range: PriorityRange,
    orderable: impl Fn(&str) -> bool,
) -> Result<(), AppError> {
    if order.len() > range.capacity() {
        return Err(AppError::user("profile.load_order_full").with("max", range.capacity()));
    }

    let mut seen = BTreeSet::new();
    for id in order {
        if !seen.insert(id.as_str()) {
            return Err(AppError::user("profile.load_order_duplicate").with("id", id));
        }
        if !mods.iter().any(|entry| entry.id == *id) {
            return Err(AppError::user("profile.load_order_unknown").with("id", id));
        }
        if !orderable(id.as_str()) {
            return Err(AppError::user("profile.load_order_not_orderable").with("id", id));
        }
    }

    for entry in mods.iter_mut() {
        entry.priority = None;
    }

    for (index, id) in order.iter().enumerate() {
        let offset = u32::try_from(index).unwrap_or(u32::MAX);
        if let Some(entry) = mods.iter_mut().find(|entry| entry.id == *id) {
            entry.priority = Some(range.min.saturating_add(offset));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, priority: Option<u32>) -> ListedMod {
        ListedMod {
            id: id.to_string(),
            folder: format!("{id}-folder"),
            enabled: true,
            priority,
        }
    }

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn range(min: u32, max: u32) -> PriorityRange {
        PriorityRange { min, max }
    }

    fn priorities(mods: &[ListedMod]) -> Vec<(&str, Option<u32>)> {
        mods.iter()
            .map(|entry| (entry.id.as_str(), entry.priority))
            .collect()
    }

    #[test]
    fn the_listed_mods_get_the_range_in_order_and_the_others_lose_theirs() {
        let mut mods = vec![
            entry("mod-a", Some(40)),
            entry("mod-b", None),
            entry("mod-c", Some(3)),
        ];

        assign_priorities(&mut mods, &ids(&["mod-c", "mod-a"]), range(1, 9999), |_| {
            true
        })
        .unwrap();

        assert_eq!(
            priorities(&mods),
            [("mod-a", Some(2)), ("mod-b", None), ("mod-c", Some(1))]
        );
    }

    #[test]
    fn the_first_number_is_the_lowest_of_the_range_not_always_one() {
        let mut mods = vec![entry("mod-a", None), entry("mod-b", None)];

        assign_priorities(&mut mods, &ids(&["mod-b", "mod-a"]), range(10, 20), |_| {
            true
        })
        .unwrap();

        assert_eq!(
            priorities(&mods),
            [("mod-a", Some(11)), ("mod-b", Some(10))]
        );
    }

    #[test]
    fn an_empty_order_clears_every_priority() {
        let mut mods = vec![entry("mod-a", Some(1)), entry("mod-b", Some(2))];

        assign_priorities(&mut mods, &[], range(1, 9999), |_| true).unwrap();

        assert_eq!(priorities(&mods), [("mod-a", None), ("mod-b", None)]);
    }

    #[test]
    fn a_repeated_id_is_refused_and_nothing_changes() {
        let mut mods = vec![entry("mod-a", Some(7))];

        let error = assign_priorities(&mut mods, &ids(&["mod-a", "mod-a"]), range(1, 9999), |_| {
            true
        })
        .unwrap_err();

        assert_eq!(error.code, "profile.load_order_duplicate");
        assert_eq!(priorities(&mods), [("mod-a", Some(7))]);
    }

    #[test]
    fn an_id_the_profile_does_not_have_is_refused_and_nothing_changes() {
        let mut mods = vec![entry("mod-a", Some(7))];

        let error = assign_priorities(&mut mods, &ids(&["mod-a", "mod-x"]), range(1, 9999), |_| {
            true
        })
        .unwrap_err();

        assert_eq!(error.code, "profile.load_order_unknown");
        assert_eq!(priorities(&mods), [("mod-a", Some(7))]);
    }

    #[test]
    fn a_range_with_too_few_numbers_refuses_a_longer_order() {
        let mut mods = vec![
            entry("mod-a", None),
            entry("mod-b", None),
            entry("mod-c", None),
        ];

        let error = assign_priorities(
            &mut mods,
            &ids(&["mod-a", "mod-b", "mod-c"]),
            range(1, 2),
            |_| true,
        )
        .unwrap_err();

        assert_eq!(error.code, "profile.load_order_full");
        assert_eq!(error.params.get("max").map(String::as_str), Some("2"));
        assert_eq!(
            priorities(&mods),
            [("mod-a", None), ("mod-b", None), ("mod-c", None)]
        );
    }

    #[test]
    fn the_range_numbers_both_of_its_ends() {
        assert_eq!(range(1, 3).capacity(), 3);

        let mut mods = vec![entry("mod-a", None), entry("mod-b", None)];

        assign_priorities(&mut mods, &ids(&["mod-a", "mod-b"]), range(8, 9), |_| true).unwrap();

        assert_eq!(priorities(&mods), [("mod-a", Some(8)), ("mod-b", Some(9))]);
    }

    #[test]
    fn a_mod_the_game_cannot_order_is_refused_and_nothing_changes() {
        let mut mods = vec![entry("mod-a", Some(7)), entry("mod-b", None)];

        let error = assign_priorities(&mut mods, &ids(&["mod-b"]), range(1, 9999), |id| {
            id != "mod-b"
        })
        .unwrap_err();

        assert_eq!(error.code, "profile.load_order_not_orderable");
        assert_eq!(error.params.get("id").map(String::as_str), Some("mod-b"));
        assert_eq!(priorities(&mods), [("mod-a", Some(7)), ("mod-b", None)]);
    }

    #[test]
    fn an_id_the_profile_does_not_have_is_reported_before_its_orderability() {
        let mut mods = vec![entry("mod-a", None)];

        let error =
            assign_priorities(&mut mods, &ids(&["mod-x"]), range(1, 9999), |_| false).unwrap_err();

        assert_eq!(error.code, "profile.load_order_unknown");
    }
}
