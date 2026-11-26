//! The layout profile a summary uses to skip files that cannot conflict or land outside a mod's own folder. Shared by
//! the library, which builds the profiles, and the resolver, which reads them.

/// What a summary has to materialize for one file list. The profile is computed once with the game's rules, so a
/// status does not run the mirror, root-file and shared-kind checks for every file of every mod.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LayoutProfile {
    /// The files that can land outside the mod's own folder, in file order. `None` means none of them can; `All` means
    /// every file can (a mirrored-only list or a shared rule over the whole list).
    pub materialized: Materialized,
    /// Every file keeps its game path, so the per-file mirror check is already answered.
    pub mirrored_only: bool,
}

/// How much of a file list a summary materializes: none, all, or the files named by index.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum Materialized {
    /// No profile was computed for this list; check every file.
    #[default]
    Unknown,
    /// No file of the list can materialize.
    None,
    /// Every file of the list materializes.
    All,
    /// These file indexes materialize, in file order.
    Some(Vec<u32>),
}
