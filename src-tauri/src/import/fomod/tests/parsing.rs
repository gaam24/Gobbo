//! Parsing the installer's XML: steps, groups, options, conditions, and the encodings it comes in.

use super::*;

#[test]
fn parses_steps_groups_options_and_conditions() {
    let config = config();
    assert_eq!(config.module_name, "Fixture Mod");
    assert_eq!(config.steps.len(), 1);
    assert_eq!(config.steps[0].groups.len(), 2);
    assert_eq!(config.steps[0].groups[0].kind, GroupKind::SelectExactlyOne);
    assert_eq!(
        config.steps[0].groups[0].options[0].kind,
        OptionKind::Recommended
    );
    assert_eq!(
        config.steps[0].groups[0].options[0]
            .flags
            .get("quality")
            .map(String::as_str),
        Some("low")
    );
    assert_eq!(config.conditional.len(), 1);
}

#[test]
fn parses_required_install_without_steps() {
    let config = parse(
            r#"<config><moduleName>Plain</moduleName>
            <requiredInstall><files><plugin name="Always"><files><folder source="content"/></files></plugin></files></requiredInstall>
            </config>"#,
        )
        .expect("valid XML");
    assert_eq!(config.steps.len(), 1);
    assert_eq!(
        config.steps[0].groups[0].options[0].files[0].source,
        "content"
    );
}

#[test]
fn decodes_utf8_and_utf16_xml() {
    let utf8 = "<config><moduleName>Æ – ünïcödé</moduleName></config>";
    // BOM UTF-8
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(utf8.as_bytes());
    assert_eq!(decode(&bytes).expect("UTF-8 with BOM"), utf8);

    // UTF-16 LE with a BOM, the way Windows installers save it, with non-ASCII letters.
    let mut bytes = vec![0xFF, 0xFE];

    for unit in utf8.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }

    assert_eq!(decode(&bytes).expect("UTF-16LE with BOM"), utf8);

    // UTF-16 BE with a BOM
    let mut bytes = vec![0xFE, 0xFF];

    for unit in utf8.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }

    assert_eq!(decode(&bytes).expect("UTF-16BE with BOM"), utf8);

    // UTF-16 LE without a BOM: recognized by the zeros in the bytes
    let bytes: Vec<u8> = utf8.encode_utf16().flat_map(u16::to_le_bytes).collect();
    assert_eq!(decode(&bytes).expect("UTF-16LE without a BOM"), utf8);

    // Plain UTF-8 without a BOM comes back unchanged
    assert_eq!(decode(utf8.as_bytes()).expect("UTF-8 without a BOM"), utf8);
}
