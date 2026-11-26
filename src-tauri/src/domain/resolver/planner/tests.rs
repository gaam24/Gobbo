//! Tests for the folder names the plan gives to mods.

use super::plan::unique_name;
use crate::domain::naming;
use crate::domain::pathkey::path_key;

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
