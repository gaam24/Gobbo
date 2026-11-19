//! The errors of a FOMOD installer: broken XML, an unknown option, or a group that breaks its rule.

use crate::error::{AppError, ErrorKind};

/// An error of the FOMOD installer: the choices break the group rules, or the XML is broken.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FomodError {
    /// The XML cannot be read, for example a broken tag.
    #[error("FOMOD installer: {message}")]
    Parse { message: String },
    /// The chosen option does not exist in this group.
    #[error("FOMOD installer: option {option:?} does not exist in group {group:?}")]
    UnknownOption { group: String, option: String },
    /// The group breaks its rule, for example `SelectExactlyOne` has zero or two choices.
    /// `rule` is the message code, such as `fomod.group_pick_exactly`.
    #[error("FOMOD installer: group {group:?} breaks rule {rule}")]
    GroupRule { group: String, rule: &'static str },
    /// `<moduleDependencies>` are not met. They are checked before any step, so no choice can satisfy them.
    #[error("FOMOD installer: the module dependencies are not met")]
    ModuleDependenciesUnmet,
}

impl From<FomodError> for AppError {
    fn from(error: FomodError) -> Self {
        match error {
            FomodError::Parse { message } => Self::new("fomod.parse_failed")
                .with("detail", message)
                .kind(ErrorKind::Permanent),
            FomodError::UnknownOption { group, option } => Self::new("fomod.unknown_option")
                .with("group", group)
                .with("option", option)
                .kind(ErrorKind::UserAction),
            FomodError::GroupRule { group, rule } => Self::new(rule)
                .with("group", group)
                .kind(ErrorKind::UserAction),
            FomodError::ModuleDependenciesUnmet => {
                Self::new("fomod.module_dependencies_unmet").kind(ErrorKind::Permanent)
            }
        }
    }
}
