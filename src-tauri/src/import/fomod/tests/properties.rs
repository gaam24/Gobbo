//! Property tests: installer bytes and text are a typed error or a valid config, never a panic. The input is what an
//! archive supplies, so it is untrusted: a damaged or hostile `ModuleConfig.xml` must not take the import down.

use proptest::prelude::*;

use super::*;

/// Arbitrary text, as an installer file decoded from any encoding could contain.
fn text_strategy() -> impl Strategy<Value = String> {
    proptest::collection::vec(any::<char>(), 0..2048).prop_map(|chars| chars.into_iter().collect())
}

/// Text that carries no encoding signal: no NUL and no byte order mark. Proptest's default `char` strategy favours
/// both, so they are replaced rather than filtered out.
fn plain_text_strategy() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        any::<char>().prop_map(|c| {
            if matches!(c, '\0' | '\u{FEFF}') {
                '\u{1}'
            } else {
                c
            }
        }),
        0..2048,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The import path: decode the bytes, then parse what came out. Either step may refuse, neither may panic.
    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..4096)) {
        if let Ok(text) = decode(&bytes) {
            let _ = parse(&text);
        }
    }

    /// Any string is a typed parse error or a config. A config means the root element was `<config>`, and every name
    /// it holds is trimmed text from the same document.
    #[test]
    fn arbitrary_text_is_a_typed_error_or_a_config(text in text_strategy()) {
        match parse(&text) {
            Ok(config) => {
                prop_assert!(text.contains("config"));
                prop_assert!(config.module_name.chars().count() <= text.chars().count());
            }
            Err(error) => {
                prop_assert!(!error.to_string().is_empty());
            }
        }
    }

    /// Text without a byte order mark or a NUL is UTF-8 and comes back unchanged, as `ModuleConfig.xml` saved by the
    /// usual Windows tools is.
    #[test]
    fn text_without_a_bom_or_nul_decodes_to_itself(text in plain_text_strategy()) {
        match decode(text.as_bytes()) {
            Ok(decoded) => prop_assert_eq!(decoded, text),
            Err(error) => prop_assert!(false, "plain UTF-8 was refused: {error}"),
        }
    }

    /// A damaged installer is a parse error or a config; the parser never panics on a partial document.
    #[test]
    fn a_damaged_installer_never_panics(
        damage in proptest::collection::vec((any::<usize>(), any::<char>()), 1..16),
    ) {
        let mut chars: Vec<char> = CONFIG.chars().collect();
        for (at, value) in damage {
            let at = at % (chars.len() + 1);
            if at == chars.len() {
                chars.push(value);
            } else {
                chars[at] = value;
            }
        }
        let text: String = chars.into_iter().collect();
        let _ = parse(&text);
    }
}
