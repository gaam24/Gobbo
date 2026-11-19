//! The folding rule: a name spelled in another case outside ASCII is the same name, on every platform.

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn a_letter_outside_ascii_folds_to_its_lower_case_form() {
        assert_eq!(fold_char('Ś'), 'ś');
        assert_eq!(fold_char('ś'), 'ś');
        assert_eq!(fold_char('A'), 'a');
        assert_eq!(fold_unit('Ś' as u16), 'ś' as u16);
        // A surrogate is not a character, so it keeps its unit.
        assert_eq!(fold_unit(0xD835), 0xD835);
    }

    #[test]
    fn a_folder_spelled_in_another_case_outside_ascii_is_inside() {
        assert_eq!(
            relative_to(r"C:\Gra\ś", r"C:\Gra\Ś\save.dat").as_deref(),
            Some("save.dat")
        );
        assert_eq!(relative_to(r"C:\Gra\Ś", r"C:\Gra\ś").as_deref(), Some(""));
    }

    #[test]
    fn a_root_that_ends_inside_a_multibyte_letter_is_outside_without_a_panic() {
        // `ś` is two bytes; a root of five bytes ends inside it.
        assert_eq!(relative_to(r"C:\Ga", r"C:\Gś\x"), None);
        assert_eq!(relative_to(r"C:\Ga", r"C:\Ga\x").as_deref(), Some("x"));
    }

    #[test]
    fn whiteouts_hide_a_non_ascii_name_in_any_case() {
        let mut whiteouts = Whiteouts::default();
        whiteouts.hide(r"Ś\Plik.txt");
        assert!(whiteouts.hides(r"ś\plik.txt"));
        assert!(whiteouts.hides(r"Ś\PLIK.TXT"));
        assert!(whiteouts.hides_child("Ś", "PLIK.txt"));
        assert!(!whiteouts.hides(r"ś\inny.txt"));
    }

    #[test]
    fn a_whiteout_on_a_non_ascii_folder_hides_its_children() {
        let mut whiteouts = Whiteouts::default();
        whiteouts.hide(r"ś\folder");
        assert!(whiteouts.hides(r"Ś\Folder\a.txt"));
        assert!(whiteouts.hides(r"ś\FOLDER\b.txt"));
        assert!(!whiteouts.hides(r"ś\inny\a.txt"));
    }

    #[test]
    fn a_listing_shows_one_entry_for_a_name_spelled_in_two_cases() {
        let merged = merge_listing(
            vec![("Ś.txt".to_string(), 1)],
            Vec::new(),
            vec![("ś.txt".to_string(), 2)],
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].0, "Ś.txt");
    }

    #[test]
    fn a_copy_temporary_name_has_one_shape_and_no_other_name_does() {
        let name = copy_temp_name(4242, 7);
        assert_eq!(name, ".gobbo-copy-4242-7.tmp");
        assert!(is_copy_temp_name(&name));
        for other in [
            ".gobbo-copy-x-7.tmp",
            ".gobbo-copy-4242-.tmp",
            ".gobbo-copy-4242-7.txt",
            "a.gobbo-copy-4242-7.tmp",
            "gobbo-copy-4242-7.tmp",
            "save.tmp",
        ] {
            assert!(!is_copy_temp_name(other), "{other}");
        }
    }

    #[test]
    fn a_pattern_matches_a_non_ascii_name_in_any_case() {
        assert!(wildcard_match("ś*", "ŚWIAT.txt"));
        assert!(wildcard_match("*.TXT", "źródło.txt"));
    }
}
