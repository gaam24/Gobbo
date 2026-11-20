//! Page texts as plain text. Nexus mixes `BBCode`, a few HTML tags and HTML entities into descriptions, and the app
//! shows only the words.

/// The longest description kept: enough for the panel, and a page cannot make the library file huge.
const DESCRIPTION_MAX: usize = crate::domain::DESCRIPTION_MAX;

/// A page text as plain text: Nexus mixes `BBCode`, a few HTML tags (`<br />`) and HTML entities. Tags go, line breaks
/// stay, and runs of empty lines shrink to one.
pub(super) fn plain_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(at) = rest.find(['[', '<']) {
        out.push_str(&rest[..at]);
        let open = rest.as_bytes()[at];
        let close = if open == b'[' { ']' } else { '>' };
        let tag = rest[at + 1..]
            .find(close)
            .map(|end| &rest[at + 1..at + 1 + end])
            .filter(|tag| is_markup(tag));
        if let Some(tag) = tag {
            let name = tag.trim_start_matches('/').to_ascii_lowercase();

            if open == b'[' && name == "img" && !tag.starts_with('/') {
                // A picture is no text: its address between the tags goes with it.
                let after = &rest[at + tag.len() + 2..];
                rest = find_ignore_ascii_case(after, "[/img]")
                    .map_or(after, |end| &after[end + "[/img]".len()..]);
                continue;
            }

            if (name == "*" || name == "li") && !tag.starts_with('/') {
                out.push_str("\n• ");
            } else if name.starts_with("br") || name == "line" || name == "p" {
                out.push('\n');
            }

            rest = &rest[at + tag.len() + 2..];
        } else {
            out.push(char::from(open));
            rest = &rest[at + 1..];
        }
    }

    out.push_str(rest);
    let decoded = decode_entities(&out);
    let mut lines: Vec<&str> = Vec::new();

    for line in decoded.lines().map(str::trim) {
        if line.is_empty() && lines.last().is_none_or(|last| last.is_empty()) {
            continue;
        }

        lines.push(line);
    }

    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }

    let text = lines.join("\n");

    match text.char_indices().nth(DESCRIPTION_MAX) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text,
    }
}

/// Finds `needle` in `haystack` without lowercasing the whole string. The tag names are ASCII.
fn find_ignore_ascii_case(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

/// The tags Nexus pages use, `BBCode` and HTML alike. Other text in brackets, such as `[F1]`, stays as written.
const MARKUP: [&str; 42] = [
    "*", "a", "b", "br", "center", "code", "color", "colour", "div", "em", "email", "font", "h1",
    "h2", "h3", "h4", "heading", "hr", "i", "img", "left", "li", "line", "list", "ol", "p",
    "quote", "right", "s", "size", "span", "spoiler", "strong", "sub", "sup", "table", "td", "th",
    "tr", "u", "ul", "url",
];

/// Whether the text between brackets is a known tag, maybe closing, maybe with a value.
fn is_markup(tag: &str) -> bool {
    let name = tag.strip_prefix('/').unwrap_or(tag);
    let name = name
        .split(['=', ' '])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    tag.len() <= 400 && MARKUP.iter().any(|known| name.eq_ignore_ascii_case(known))
}

/// The HTML entities Nexus texts use.
pub(super) fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let entity = rest[at + 1..]
            .find(';')
            .filter(|end| *end <= 8)
            .map(|end| &rest[at + 1..at + 1 + end]);
        let decoded = entity.and_then(|entity| match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix('#')
                .and_then(|code| {
                    code.strip_prefix('x').map_or_else(
                        || code.parse().ok(),
                        |hex| u32::from_str_radix(hex, 16).ok(),
                    )
                })
                .and_then(char::from_u32),
        });

        if let (Some(entity), Some(decoded)) = (entity, decoded) {
            out.push(decoded);
            rest = &rest[at + entity.len() + 2..];
        } else {
            out.push('&');
            rest = &rest[at + 1..];
        }
    }

    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_keeps_brackets_that_are_not_tags() {
        assert_eq!(
            plain_text("Press [F1] or [Ctrl + A]"),
            "Press [F1] or [Ctrl + A]"
        );
        assert_eq!(plain_text("a < b and 1&2 &#65;&#x42;"), "a < b and 1&2 AB");
        assert_eq!(plain_text("[b]x[/b]"), "x");
        assert_eq!(
            plain_text("<ul><li>one</li><li>two</li></ul>"),
            "• one\n• two"
        );
    }

    #[test]
    fn a_picture_in_a_description_leaves_no_text() {
        assert_eq!(
            plain_text("[center][img]https://x/y.jpg[/img]\nText after"),
            "Text after"
        );
        assert_eq!(plain_text("[img]https://x/y.jpg"), "https://x/y.jpg");
    }
}
