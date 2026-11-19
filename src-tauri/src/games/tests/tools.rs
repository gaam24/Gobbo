//! Tools and their prerequisites in a definition: a source mod, and an https link with a checked digest.

use super::*;

#[test]
fn a_tool_names_a_source_mod_of_a_listed_provider() {
    has_problem(
        &SLOT.replace("mod_id = \"4242\"", "mod_id = \"\""),
        "no source mod id",
    );
    has_problem(
        &SLOT.replace("provider = \"nexus\"", "provider = \"modio\""),
        "which the game does not list",
    );
}

#[test]
fn a_provider_entry_needs_a_key() {
    has_problem(
        &SLOT.replace(
            "providers = { nexus = { key = \"slot\" } }",
            "providers = { nexus = {} }",
        ),
        "needs a key",
    );
    has_problem(
        &SLOT.replace(
            "providers = { nexus = { key = \"slot\" } }",
            "providers = { Nexus = { key = \"slot\" } }",
        ),
        "provider id",
    );
}

#[test]
fn a_prerequisite_comes_from_an_https_link_with_a_lowercase_sha512() {
    has_problem(
        &SLOT.replace("url = \"https://builds", "url = \"http://builds"),
        "https link",
    );
    has_problem(&SLOT.replace("sha512 = \"b6", "sha512 = \"B6"), "SHA-512");
}

#[test]
fn a_prerequisite_names_the_version_folder_it_checks() {
    has_problem(
        &SLOT.replace("check_starts_with = \"8.\"", "check_starts_with = \" \""),
        "check_starts_with",
    );
}
