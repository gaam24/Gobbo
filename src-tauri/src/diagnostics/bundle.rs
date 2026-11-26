//! The support bundle: one zip with the files a report needs, with the user's paths and secrets removed. It never holds
//! a mod, a game file or a saved credential.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use crate::context::Data;
use crate::error::{AppError, io_at};
use crate::infra::clock::now_seconds;
use crate::infra::log::FILE_NAME as LOG_FILE_NAME;
use crate::launch::report::REPORT_FILE as REPORT_FILE_NAME;

/// The folder under the data root. It is not a mod folder and never part of a deployment.
const SUPPORT_DIR: &str = "support";
/// The newest part of the log that a bundle keeps. A long session does not make the bundle huge.
const LOG_TAIL: usize = 1_500_000;

/// Writes a support bundle and returns its path. `game` adds the definition, the setup check and the last run; without
/// it the bundle still has the log, the version and the system information.
pub fn create(root: &Data, game: Option<&str>) -> Result<String, AppError> {
    let dir = root.dir.join(SUPPORT_DIR);
    fs::create_dir_all(&dir).map_err(io_at(&dir))?;
    let path = dir.join(format!("gobbo-support-{}.zip", now_seconds()));
    let mut writer = zip::ZipWriter::new(File::create(&path).map_err(io_at(&path))?);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let replacements = replacements(root, game);
    let redact = |text: &str| redact_bundle_text(text, &replacements);

    add(
        &mut writer,
        options,
        "system.txt",
        &redact(&system_text(root)),
    )?;

    if let Some(log) = read_tail(&root.dir.join(LOG_FILE_NAME), LOG_TAIL) {
        add(&mut writer, options, LOG_FILE_NAME, &redact(&log))?;
    }

    if let Some(game_id) = game {
        let definition = crate::games::definition_path(game_id);

        if let Ok(text) = fs::read_to_string(&definition) {
            add(
                &mut writer,
                options,
                &format!("game/{game_id}/game.toml"),
                &redact(&text),
            )?;
        }

        if let Ok(report) = crate::status::preflight::preflight(root, game_id) {
            add_json(&mut writer, options, "preflight.json", &report, &redact)?;
        }

        if let Some(report) = crate::launch::report::last_run(root, game_id) {
            add_json(&mut writer, options, REPORT_FILE_NAME, &report, &redact)?;
        }
    }

    add(
        &mut writer,
        options,
        "README.txt",
        "This bundle is made for a Gobbo support report. It holds the application log, the version, the system\n\
         information and, when a game was selected, its definition, setup check and last run. Paths are replaced with\n\
         <home>, <data> and <game>; API keys and signed download links are removed by the log redaction. It does not\n\
         contain mods, game files or saved passwords.\n",
    )?;
    writer.finish().map_err(AppError::unexpected)?;
    crate::infra::log::info(format!("support bundle written to {}", path.display()));
    Ok(path.display().to_string())
}

fn add(
    writer: &mut zip::ZipWriter<File>,
    options: zip::write::SimpleFileOptions,
    name: &str,
    text: &str,
) -> Result<(), AppError> {
    writer
        .start_file(name, options)
        .map_err(AppError::unexpected)?;
    writer
        .write_all(text.as_bytes())
        .map_err(AppError::unexpected)
}

/// Adds `value` to the bundle as pretty JSON, after `redact` has removed the user's paths and secrets from the text.
fn add_json<T: serde::Serialize>(
    writer: &mut zip::ZipWriter<File>,
    options: zip::write::SimpleFileOptions,
    name: &str,
    value: &T,
    redact: &impl Fn(&str) -> String,
) -> Result<(), AppError> {
    let text = serde_json::to_string_pretty(value).map_err(AppError::unexpected)?;
    add(writer, options, name, &redact(&text))
}

fn system_text(root: &Data) -> String {
    format!(
        "version: {}\nos: {}\narch: {}\ndata root: {}\ngenerated: {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        root.dir.display(),
        now_seconds()
    )
}

/// Reads the newest `limit` bytes of a file. A missing or unreadable file is simply left out of the bundle.
fn read_tail(path: &Path, limit: usize) -> Option<String> {
    let bytes = fs::read(path).ok()?;

    if bytes.len() <= limit {
        return Some(String::from_utf8_lossy(&bytes).into_owned());
    }

    let start = bytes.len() - limit;
    Some(String::from_utf8_lossy(&bytes[start..]).into_owned())
}

/// The strings that are replaced in every text file, longest first so a nested path does not leave a prefix behind.
fn replacements(root: &Data, game: Option<&str>) -> Vec<(String, String)> {
    let mut replacements = vec![
        (root.dir.display().to_string(), "<data>".to_string()),
        (
            crate::games::paths::home().display().to_string(),
            "<home>".to_string(),
        ),
    ];

    if let Some(game_id) = game {
        let settings = crate::context::load_settings(&root.game(game_id));

        if !settings.game_root.trim().is_empty() {
            replacements.push((settings.game_root.trim().to_string(), "<game>".to_string()));
        }
    }

    replacements.sort_by_key(|entry| std::cmp::Reverse(entry.0.len()));
    replacements
}

/// Removes both the user's paths and the HTTP secrets (API keys, tokens, signed links) from text before it goes into
/// the bundle. The secret pass runs first: a definition, a report or a log line can carry either kind, and neither may
/// reach the support report.
fn redact_bundle_text(text: &str, replacements: &[(String, String)]) -> String {
    redact_text(&crate::infra::http::redact(text), replacements)
}

fn redact_text(text: &str, replacements: &[(String, String)]) -> String {
    let mut value = text.to_string();

    for (from, to) in replacements {
        if !from.is_empty() {
            value = value.replace(from, to);
        }
    }

    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;
    use std::io::Read;

    #[test]
    fn a_bundle_holds_the_log_and_redacts_the_home_path() {
        let base = scratch!("support-bundle");
        let root = Data::new(base.join("data"));
        fs::create_dir_all(&root.dir).unwrap();
        let home = crate::games::paths::home();
        fs::write(
            root.dir.join(LOG_FILE_NAME),
            format!("log line at {}\n", home.display()),
        )
        .unwrap();

        let path = create(&root, None).unwrap();
        let file = fs::File::open(&path).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let mut log = String::new();
        zip.by_name(LOG_FILE_NAME)
            .unwrap()
            .read_to_string(&mut log)
            .unwrap();
        assert!(log.contains("<home>"), "{log}");
        assert!(!log.contains(&home.display().to_string()));
        let mut system = String::new();
        zip.by_name("system.txt")
            .unwrap()
            .read_to_string(&mut system)
            .unwrap();
        assert!(system.contains(env!("CARGO_PKG_VERSION")));
        assert!(zip.by_name("README.txt").is_ok());
    }

    #[test]
    fn a_bundle_masks_api_keys_tokens_and_signed_links() {
        let base = scratch!("support-bundle-secrets");
        let root = Data::new(base.join("data"));
        fs::create_dir_all(&root.dir).unwrap();
        let secrets = "key=top-secret&token=abc123&sig=deadbeef&X-Amz-Signature=cafebabe";
        fs::write(
            root.dir.join(LOG_FILE_NAME),
            format!("GET https://cdn.example.com/a.zip?{secrets}&page=2\n"),
        )
        .unwrap();

        let path = create(&root, None).unwrap();
        let mut zip = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        let mut log = String::new();
        zip.by_name(LOG_FILE_NAME)
            .unwrap()
            .read_to_string(&mut log)
            .unwrap();
        assert!(log.contains("key=***"));
        assert!(log.contains("token=***"));
        assert!(log.contains("sig=***"));
        assert!(log.contains("X-Amz-Signature=***"));
        assert!(log.contains("page=2"));
        for secret in ["top-secret", "abc123", "deadbeef", "cafebabe"] {
            assert!(!log.contains(secret), "the bundle kept a secret: {log}");
        }
    }

    #[test]
    fn a_definition_text_with_a_secret_is_redacted_before_it_is_written() {
        let base = scratch!("support-definition-secret");
        let root = Data::new(base.join("data"));
        let replacements = replacements(&root, None);
        let definition =
            "# generated\ndownload_url = \"https://cdn.example.com/a.zip?key=hidden&expires=99\"\n";
        let redacted = redact_bundle_text(definition, &replacements);

        assert!(!redacted.contains("hidden"));
        assert!(redacted.contains("key=***"));
        assert!(redacted.contains("expires=***"));
        assert!(redacted.contains("download_url"));
    }
}
