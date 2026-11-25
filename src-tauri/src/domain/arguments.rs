//! Game arguments and environment variables, as the settings hold them. Plain text, parsed the same on every platform.

use crate::error::AppError;

/// Environment variables from the settings: one per line, as `NAME=value`. Empty lines are skipped.
/// The value may contain `=` and spaces. When a pair of quotes surrounds it, the quotes are removed.
pub fn parse_env(text: &str) -> Result<Vec<(String, String)>, AppError> {
    let mut vars = Vec::new();

    for (index, line) in text.lines().enumerate() {
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        let number = index + 1;
        let (name, value) = line
            .split_once('=')
            .ok_or_else(|| AppError::user("launch.env_no_equals").with("line", number))?;
        let name = name.trim();

        if name.is_empty() || name.contains(char::is_whitespace) {
            return Err(AppError::user("launch.env_bad_name").with("line", number));
        }

        vars.push((name.to_string(), unquote(value.trim()).to_string()));
    }

    Ok(vars)
}

/// Game parameters: separated by spaces. A pair of double quotation marks keeps the words between them as one
/// parameter, and the marks are removed, so `-config "C:\My Games\a.ini"` passes one path. A backslash is never an
/// escape, so a Windows path reaches the game as it is. A quotation mark that is never closed takes the rest of the text.
pub fn parse_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;

    for c in text.chars() {
        if c == '"' {
            quoted = !quoted;
            started = true;
        } else if c.is_whitespace() && !quoted {
            if started {
                args.push(std::mem::take(&mut current));
                started = false;
            }
        } else {
            current.push(c);
            started = true;
        }
    }

    if started {
        args.push(current);
    }

    args
}

/// The `WINEDLLOVERRIDES` value that makes Wine load each DLL from the game folder before its own copy (`n,b`), for the
/// DLLs that `existing` does not name yet. `existing` is the value as it is (`name[,name]=mode;...`) and it is kept; a
/// name compares without case and without `.dll`. `None` when every DLL is named already.
pub fn native_first_overrides(existing: Option<&str>, dlls: &[&str]) -> Option<String> {
    let mut parts: Vec<String> = existing
        .unwrap_or_default()
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    let mut named: Vec<String> = parts
        .iter()
        .filter_map(|part| part.split_once('='))
        .flat_map(|(names, _)| names.split(','))
        .map(override_name)
        .collect();
    let mut added = false;

    for dll in dlls {
        let name = override_name(dll);

        if !name.is_empty() && !named.contains(&name) {
            parts.push(format!("{}=n,b", dll.trim()));
            named.push(name);
            added = true;
        }
    }

    added.then(|| parts.join(";"))
}

/// A DLL name as Wine compares it in an override: lowercase, without spaces and without the `.dll` suffix.
fn override_name(name: &str) -> String {
    let name = name.trim().to_ascii_lowercase();
    name.strip_suffix(".dll").unwrap_or(&name).to_string()
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_lines_are_name_and_value() {
        let vars = parse_env("WINEDLLOVERRIDES=version=n,b\n\n  DXVK_HUD = fps \n").unwrap();
        assert_eq!(
            vars,
            vec![
                ("WINEDLLOVERRIDES".to_string(), "version=n,b".to_string()),
                ("DXVK_HUD".to_string(), "fps".to_string()),
            ]
        );
    }

    #[test]
    fn one_pair_of_quotes_around_a_value_is_removed() {
        let vars = parse_env("WINEDLLOVERRIDES=\"version=n,b\"").unwrap();
        assert_eq!(
            vars,
            vec![("WINEDLLOVERRIDES".to_string(), "version=n,b".to_string())]
        );
    }

    #[test]
    fn a_line_without_a_name_or_equals_sign_is_refused() {
        let missing = parse_env("version").unwrap_err();
        assert_eq!(missing.code, "launch.env_no_equals");
        assert_eq!(missing.params["line"], "1");
        let empty_name = parse_env("OK=1\n=2").unwrap_err();
        assert_eq!(empty_name.code, "launch.env_bad_name");
        assert_eq!(empty_name.params["line"], "2");
        let spaced = parse_env("A B=1").unwrap_err();
        assert_eq!(spaced.code, "launch.env_bad_name");
        assert_eq!(spaced.params["line"], "1");
    }

    #[test]
    fn a_proxy_dll_is_loaded_native_unless_the_settings_already_name_it() {
        let winhttp = ["winhttp.dll"];
        assert_eq!(
            native_first_overrides(None, &winhttp).as_deref(),
            Some("winhttp.dll=n,b")
        );
        assert_eq!(
            native_first_overrides(Some("version=n,b"), &winhttp).as_deref(),
            Some("version=n,b;winhttp.dll=n,b")
        );
        assert_eq!(
            native_first_overrides(Some("version=n,b;"), &winhttp).as_deref(),
            Some("version=n,b;winhttp.dll=n,b")
        );
        assert_eq!(
            native_first_overrides(None, &["winhttp.dll", "version.dll"]).as_deref(),
            Some("winhttp.dll=n,b;version.dll=n,b")
        );
        // The user's own choice stays, whatever its spelling: another mode, case, no suffix, or a list of names.
        assert_eq!(
            native_first_overrides(Some("winhttp.dll=d"), &winhttp),
            None
        );
        assert_eq!(native_first_overrides(Some("WINHTTP=b"), &winhttp), None);
        assert_eq!(
            native_first_overrides(Some("version,winhttp.dll=n"), &winhttp),
            None
        );
        assert_eq!(native_first_overrides(Some("winhttp.dll=n,b"), &[]), None);
    }

    #[test]
    fn arguments_are_split_on_spaces() {
        assert_eq!(
            parse_args("  -skipStartScreen   -windowed "),
            vec!["-skipStartScreen", "-windowed"]
        );
        assert!(parse_args("").is_empty());
        assert!(parse_args("   ").is_empty());
    }

    #[test]
    fn a_quoted_value_with_spaces_is_one_argument() {
        assert_eq!(
            parse_args(r#"-config "C:\My Games\settings.ini" -windowed"#),
            vec![r"-config", r"C:\My Games\settings.ini", "-windowed"]
        );
        assert_eq!(
            parse_args(r#"-name="Example's Game" -x"#),
            vec!["-name=Example's Game", "-x"]
        );
        assert_eq!(parse_args(r#""" -x"#), vec!["", "-x"]);
    }

    #[test]
    fn backslashes_and_apostrophes_are_kept_as_they_are() {
        assert_eq!(
            parse_args(r#"-path "C:\Games\Example's Game\" -x"#),
            vec!["-path", r"C:\Games\Example's Game\", "-x"]
        );
    }

    #[test]
    fn a_quotation_mark_that_is_never_closed_takes_the_rest() {
        assert_eq!(
            parse_args(r#"-title "My Game -windowed"#),
            vec!["-title", "My Game -windowed"]
        );
    }
}
