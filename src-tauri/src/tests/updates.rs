//! Installing and enabling the requirements of a mod.

use super::*;
use crate::domain::source::SourceRef;
use crate::test_support::import_from;

#[test]
fn installing_a_requirement_that_is_already_installed_enables_it() {
    let f = mirror_fixture("requirement-enable");
    let source = SourceRef::new("thunderstore", "owner_name-package_name");
    let zip = f.zip(
        "owner_name-package_name-1.0.0.zip",
        &[("r6/scripts/RequiredMod/a.reds", "x")],
    );
    import_from(
        &f.data,
        MIRROR_GAME,
        &zip,
        None,
        Some(source.clone()),
        &mut |_, _| {},
    )
    .unwrap();

    crate::sources::requirements::install_one_cancellable(
        &f.data,
        MIRROR_GAME,
        source,
        None,
        &mut |_| {},
    )
    .unwrap();
    let status = status(&f.data, MIRROR_GAME).unwrap();
    assert_eq!(status.mods.len(), 1);
    assert!(status.mods[0].enabled);
}
