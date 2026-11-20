//! Numbered names and the shared "first name nobody has" loop.

/// A numbered variant of a name: `base`, `base (2)`, `base (3)` and so on. The caller decides whether the name is
/// free; this only builds it.
pub fn numbered_name(base: &str, number: u32) -> String {
    if number <= 1 {
        base.to_string()
    } else {
        format!("{base} ({number})")
    }
}

/// The first free name of a series: `variant(1)`, `variant(2)` and so on until `free` accepts one. Every "pick a name
/// nobody has yet" rule (library folders, archive copies, downloads, profiles, game folders) is this loop with its own
/// variants and its own test of what is taken.
pub fn first_free(
    mut variant: impl FnMut(u32) -> String,
    mut free: impl FnMut(&str) -> bool,
) -> String {
    let mut number = 1;

    loop {
        let name = variant(number);

        if free(&name) {
            return name;
        }

        number = number.saturating_add(1);
    }
}

/// A numbered variant of a file name that keeps its extension: `name.zip`, `name (2).zip`, `name (3).zip`.
pub fn numbered_file_name(name: &str, number: u32) -> String {
    if number <= 1 {
        return name.to_string();
    }

    let (stem, extension) = crate::domain::archive::split_name(name);
    format!("{stem} ({number}){extension}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_free_takes_the_first_variant_that_is_not_taken() {
        let taken = ["Name", "Name (2)"];
        let name = first_free(|n| numbered_name("Name", n), |name| !taken.contains(&name));
        assert_eq!(name, "Name (3)");
        assert_eq!(numbered_file_name("mod.zip", 1), "mod.zip");
        assert_eq!(numbered_file_name("mod.tar.gz", 2), "mod.tar (2).gz");
        assert_eq!(numbered_file_name("readme", 3), "readme (3)");
    }
}
