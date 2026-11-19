//! Text labels derived from a mod's name when it has none of its own.

/// The name shown for a requirement: its own name, or `"provider #mod_id"` when it has none. The caller trims the
/// name when its source wants it; this only chooses between the two forms.
pub fn requirement_label(name: &str, provider: &str, mod_id: &str) -> String {
    if name.is_empty() {
        format!("{provider} #{mod_id}")
    } else {
        name.to_string()
    }
}
