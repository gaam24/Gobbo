//! The per-entry checks of the layout rules: a mirror's game path, the allowed extensions and the path prefixes.

use super::plan::MirrorRule;

/// The game path of a file under `rule`, in the definition's spelling.
pub(super) fn mirror_path(rule: &MirrorRule<'_>, parts: &[&str]) -> String {
    let mut rel = vec![rule.mirror.dir.clone()];
    match &parts[rule.dir.len()..] {
        [folder, inner @ ..] if !inner.is_empty() => {
            let prefixed = starts_with_ignore_case(folder, &rule.mirror.prefix);
            rel.push(if prefixed {
                (*folder).to_string()
            } else {
                crate::domain::naming::deploy_name(&rule.mirror.prefix, folder, folder)
            });
            rel.extend(inner.iter().map(|part| (*part).to_string()));
        }
        rest => rel.extend(rest.iter().map(|part| (*part).to_string())),
    }
    rel.join("/")
}

/// Whether the file has one of the allowed extensions. An empty list allows every file. The allowed extensions are
/// stored with a leading dot and in lower case, so the check compares bytes instead of formatting a new string.
pub(super) fn extension_allowed(parts: &[&str], extensions: &[String]) -> bool {
    if extensions.is_empty() {
        return true;
    }
    let Some(name) = parts.last() else {
        return false;
    };
    let Some((_, extension)) = name.rsplit_once('.') else {
        return false;
    };
    extensions.iter().any(|allowed| {
        allowed.len() == extension.len() + 1
            && allowed
                .get(1..)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(extension))
    })
}

/// Whether `text` starts with `prefix`, ignoring ASCII case. Game definitions use ASCII prefixes; a non-ASCII prefix
/// falls back to the full Unicode lowercasing.
fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    if prefix.is_ascii() {
        crate::domain::pathkey::starts_with_ignore_ascii_case(text, prefix)
    } else {
        text.to_lowercase().starts_with(&prefix.to_lowercase())
    }
}

/// Whether the path starts with these parts (regardless of letter case). An equal path counts too.
pub(super) fn starts_with_path(parts: &[&str], prefix: &[&str]) -> bool {
    parts.len() >= prefix.len()
        && parts
            .iter()
            .zip(prefix)
            .all(|(part, want)| part.eq_ignore_ascii_case(want))
}
