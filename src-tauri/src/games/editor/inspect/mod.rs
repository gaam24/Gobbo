//! What a chosen game folder suggests: the programs, the relative folders, the store ids, an installed loader and the
//! engine. The folder is only read; the editor decides what to fill in. The loader and engine signals are data-driven: a
//! preset's `requires`, places, root files and `[engine]` markers are checked in the folder, never a loader id in this
//! code.

mod executables;
mod folder;
mod loaders;
mod stores;

pub use folder::inspect_folder;

pub(super) use folder::sorted_entries;

/// A lower-case ASCII slug from a title: the form of a game id. Non-ASCII letters that have a common Latin spelling
/// are folded; other punctuation becomes one dash.
pub(super) fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut dash = false;

    for character in text.chars() {
        let lower = character.to_lowercase().next().unwrap_or(character);
        let mapped = match lower {
            'a'..='z' | '0'..='9' => Some(lower),
            'ą' | 'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' => Some('a'),
            'ć' | 'ç' | 'č' => Some('c'),
            'ę' | 'é' | 'è' | 'ê' | 'ë' | 'ē' => Some('e'),
            'í' | 'ì' | 'î' | 'ï' | 'ī' => Some('i'),
            'ł' => Some('l'),
            'ń' | 'ñ' | 'ň' => Some('n'),
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ō' => Some('o'),
            'ś' | 'š' => Some('s'),
            'ú' | 'ù' | 'û' | 'ü' | 'ū' => Some('u'),
            'ý' | 'ÿ' => Some('y'),
            'ż' | 'ź' | 'ž' => Some('z'),
            _ => None,
        };

        if let Some(character) = mapped {
            out.push(character);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }

    while out.ends_with('-') {
        out.pop();
    }

    if out.is_empty() {
        "game".to_string()
    } else {
        out
    }
}
