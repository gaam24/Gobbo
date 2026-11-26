use serde::{Deserialize, Serialize};

/// Mod data from the `info.json` in the archive. Without the file, the name is the folder name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub struct ModInfo {
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
}

/// The name, version and author the user typed for one version. A field that is `None` keeps the value from
/// `info.json`. The edit is stored apart from the scanned info, so a rescan that reads the file again keeps it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ModInfoEdit {
    pub name: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
}

impl ModInfo {
    /// The name, version and author without the description. The list row keeps the short fields; the description is
    /// read by `mod_extras` when a column or the inspector needs it.
    pub fn without_description(&self) -> Self {
        Self {
            name: self.name.clone(),
            version: self.version.clone(),
            author: self.author.clone(),
            description: None,
        }
    }
}

impl ModInfoEdit {
    /// Whether none of the name, version or author was edited.
    pub const fn is_empty(&self) -> bool {
        self.name.is_none() && self.version.is_none() && self.author.is_none()
    }

    /// Trims the fields. An empty field becomes `None`, which means the value from the file.
    pub fn cleaned(self) -> Self {
        let clean = |field: Option<String>| {
            field
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
        };
        Self {
            name: clean(self.name),
            version: clean(self.version),
            author: clean(self.author),
        }
    }

    /// Puts the typed values over `info`.
    pub fn apply(&self, info: &mut ModInfo) {
        if let Some(name) = &self.name {
            info.name.clone_from(name);
        }

        if let Some(version) = &self.version {
            info.version = Some(version.clone());
        }

        if let Some(author) = &self.author {
            info.author = Some(author.clone());
        }
    }
}
