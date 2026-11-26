//! Tests of the rules a definition must keep, and of the one loop that reads every folder in `games/`. Every rule test
//! changes one thing in a fixture definition, so the test shows which rule catches it. A new game needs no test here.

mod cache;
mod definition;
mod icons;
mod loaders;
mod rules;
mod tools;

use std::path::Path;

use super::*;
use crate::test_support::games::{ALIASED, EXTERNAL, MIRROR, PLUGIN, SLOT, lf};

fn problems(text: &str) -> Vec<String> {
    GameDef::parse(text)
        .expect("the text must parse; these tests check validation")
        .validate()
        .into_iter()
        .map(|problem| problem.message)
        .collect()
}

fn has_problem(text: &str, part: &str) {
    let found = problems(text);
    assert!(
        found.iter().any(|problem| problem.contains(part)),
        "no {part:?} in {found:?}"
    );
}
