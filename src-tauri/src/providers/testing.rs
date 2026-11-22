//! The contract every provider keeps. A provider supplies one `Case` with public sample values; `check_all` proves
//! the port's invariants and that every registered provider has a case. A new adapter cannot join the registry without
//! answering the same questions as the existing ones.

use super::{SourceDownloadRequest, SourceDownloaded, SourceProvider, info, providers};
use crate::domain::library::ArchiveLabel;
use crate::domain::source::{ProviderId, SourceAuthKind, SourceCapabilities, SourceRef};
use crate::error::AppError;
use std::cell::RefCell;

/// Public sample values for one provider: a mod page, a deep link and an archive name, with what they must become.
pub struct Case {
    pub provider: &'static dyn SourceProvider,
    /// A mod id the provider accepts.
    pub mod_id: &'static str,
    /// A game key and the page URL the provider must build for `mod_id`.
    pub page: Option<(&'static str, &'static str)>,
    /// A deep link the provider owns and the source it must parse to.
    pub link: Option<(&'static str, SourceRef)>,
    /// An archive name the provider recognises and the source it must give.
    pub archive: Option<(&'static str, SourceRef)>,
    /// A file id and the display version it must give, when the provider's file id is a version.
    pub version: Option<(&'static str, &'static str)>,
}

/// The sample cases of every registered provider. A provider that is missing here fails `check_all`.
fn cases() -> Vec<Case> {
    vec![
        Case {
            provider: &super::nexus::NexusProvider,
            mod_id: "35686",
            page: Some((
                "skyrimspecialedition",
                "https://www.nexusmods.com/skyrimspecialedition/mods/35686",
            )),
            link: Some((
                "nxm://skyrimspecialedition/mods/35686/files/123456",
                SourceRef::new("nexus", "35686").with_file("123456"),
            )),
            archive: Some((
                "Example Mod-35686-1-0-1584112564.zip",
                SourceRef::new("nexus", "35686"),
            )),
            version: None,
        },
        Case {
            provider: &super::thunderstore::ThunderstoreProvider,
            mod_id: "owner_name-package_name",
            page: Some((
                "riskofrain2",
                "https://thunderstore.io/c/riskofrain2/p/owner_name/package_name/",
            )),
            link: Some((
                "ror2mm://v1/install/thunderstore.io/owner_name/package_name/1.0.0/",
                SourceRef::new("thunderstore", "owner_name-package_name").with_file("1.0.0"),
            )),
            archive: Some((
                "owner_name-package_name-1.0.0.zip",
                SourceRef::new("thunderstore", "owner_name-package_name").with_file("1.0.0"),
            )),
            version: Some(("1.0.0", "1.0.0")),
        },
    ]
}

/// Checks every case, then that no registered provider is left without one.
pub fn check_all() {
    let cases = cases();

    for case in &cases {
        check(case);
    }

    let covered: Vec<String> = cases
        .iter()
        .map(|case| case.provider.id().as_str().to_string())
        .collect();
    for registered in providers() {
        let id = registered.id();
        assert!(
            covered.iter().any(|known| known == id.as_str()),
            "provider {id} is registered but has no contract case in providers/testing.rs"
        );
    }
}

/// Checks the port's invariants for one provider. The message names the provider and the rule, so a new adapter fails
/// with an answer instead of a puzzle.
pub fn check(case: &Case) {
    let adapter = case.provider;
    let id = adapter.id();
    let id = id.as_str();
    let named = |rule: &str| format!("provider {id}: {rule}");

    assert!(!id.is_empty(), "a provider needs an id");
    assert!(
        id.bytes().all(|byte| byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || byte == b'_'
            || byte == b'-'),
        "{}",
        named("the id must be lower case ASCII letters, digits, '_' or '-'")
    );
    assert!(
        super::provider(id).is_some_and(|found| found.id().as_str() == id),
        "{}",
        named("the registry does not return this provider by its id")
    );
    assert!(
        !adapter.display_name().trim().is_empty(),
        "{}",
        named("the display name is empty")
    );

    let capabilities = adapter.capabilities();
    assert_eq!(
        capabilities.auth == SourceAuthKind::None,
        adapter.credential_account().is_none(),
        "{}",
        named("the auth capability and the saved-key account disagree")
    );
    assert_eq!(
        capabilities.deep_link,
        !adapter.schemes().is_empty(),
        "{}",
        named("the deep-link capability and the link schemes disagree")
    );

    for &scheme in adapter.schemes() {
        assert!(
            !scheme.is_empty()
                && scheme
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'-'),
            "{}",
            named("a link scheme is not a URL scheme")
        );
        assert!(
            super::for_link(&format!("{scheme}://example.invalid"))
                .is_some_and(|found| found.id().as_str() == id),
            "{}",
            named("a scheme does not find this provider")
        );
    }

    if let Some(page) = adapter.key_page() {
        assert!(
            page.starts_with("https://"),
            "{}",
            named("the key page is not an HTTPS address")
        );
    }

    let described = info(adapter);
    assert_eq!(described.id, id);
    assert_eq!(described.name, adapter.display_name());
    assert_eq!(described.needs_key, adapter.credential_account().is_some());
    assert_eq!(described.has_key_page, adapter.key_page().is_some());
    assert_eq!(
        described.schemes,
        adapter
            .schemes()
            .iter()
            .map(|scheme| (*scheme).to_string())
            .collect::<Vec<_>>()
    );

    assert!(
        !adapter.valid_mod_id("") && !adapter.valid_mod_id("   "),
        "{}",
        named("an empty mod id must be refused")
    );
    assert!(
        adapter.valid_mod_id(case.mod_id),
        "{}",
        named("the sample mod id is refused")
    );

    if let Some((game_key, expected)) = case.page {
        assert_eq!(
            adapter.page_url(game_key, case.mod_id, None).as_deref(),
            Some(expected),
            "{}",
            named("the page URL of the sample mod is wrong")
        );
        assert!(
            adapter.page_url(game_key, " ", None).is_none(),
            "{}",
            named("an invalid mod id still gives a page URL")
        );
    }

    if let Some((url, expected)) = &case.link {
        let parsed = adapter
            .parse_link(url)
            .unwrap_or_else(|error| panic!("{}: {error}", named("the sample link does not parse")));
        assert_eq!(
            parsed.source,
            *expected,
            "{}",
            named("the sample link parses to another source")
        );
    }

    if let Some((file_name, expected)) = &case.archive {
        assert_eq!(
            adapter.source_from_archive_name(file_name),
            Some(expected.clone()),
            "{}",
            named("the sample archive name gives another source")
        );
    }

    if let Some((file_id, expected)) = case.version {
        assert_eq!(
            adapter.version_of_file(file_id),
            Some(expected),
            "{}",
            named("the sample file id gives another version")
        );
    }
}

// -----------------------------------------------------------------------------------------------------------------
// The test-only provider seam. The whole `testing` module is gated with `#[cfg(test)]` in `mod.rs`, so none of this can
// be compiled into a shipped build. It is configuration for tests, not game state.
// -----------------------------------------------------------------------------------------------------------------

/// The overrides installed on this test thread, newest last, each with its own token. Per-thread state keeps tests on
/// different worker threads isolated; the guard's `Drop` clears the entry so a later test reusing the same worker
/// thread cannot see it. The token makes nested overrides unwind correctly even if guards drop out of order.
struct Overrides {
    next: u64,
    entries: Vec<(u64, String, &'static dyn SourceProvider)>,
}

thread_local! {
    static OVERRIDES: RefCell<Overrides> = RefCell::new(Overrides { next: 0, entries: Vec::new() });
}

/// The override installed for `id` on this thread, when one is present. [`super::provider`] asks this first and falls
/// back to the real registry when it answers `None`, so an id with no override resolves exactly as before.
pub(super) fn get(id: &str) -> Option<&'static dyn SourceProvider> {
    OVERRIDES.with(|cell| {
        cell.borrow()
            .entries
            .iter()
            .rev()
            .find(|(_, key, _)| key == id)
            .map(|(_, _, provider)| *provider)
    })
}

/// Installs `provider` as the override for `id` until the returned guard is dropped.
fn install(id: &str, provider: &'static dyn SourceProvider) -> OverrideGuard {
    let token = OVERRIDES.with(|cell| {
        let mut overrides = cell.borrow_mut();
        overrides.next += 1;
        let token = overrides.next;
        overrides.entries.push((token, id.to_string(), provider));
        token
    });
    OverrideGuard { token }
}

/// Keeps one provider override installed while it is alive, and removes that one on drop.
pub struct OverrideGuard {
    token: u64,
}

impl Drop for OverrideGuard {
    fn drop(&mut self) {
        OVERRIDES.with(|cell| {
            cell.borrow_mut()
                .entries
                .retain(|(token, _, _)| *token != self.token);
        });
    }
}

/// The configuration of a [`FakeProvider`]. The defaults describe one generic provider file; a test changes only the
/// parts it needs — the archive entries for its game, the progress chunks, the cancel callback.
pub struct FakeConfig {
    pub id: &'static str,
    pub archive: &'static str,
    pub entries: &'static [(&'static str, &'static str)],
    pub chunks: &'static [(u64, u64)],
    pub label: Option<ArchiveLabel>,
    pub version: Option<&'static str>,
    /// Called before each progress update, so a test can stop the queue from inside the transfer.
    pub on_chunk: Option<Box<dyn Fn() + Send + Sync>>,
}

impl Default for FakeConfig {
    fn default() -> Self {
        Self {
            id: "example-provider",
            archive: "Example Provider File-1001-1.0.0.zip",
            entries: &[("Mods/modA/content/x.ws", "x")],
            chunks: &[(5, 10), (10, 10)],
            label: Some(ArchiveLabel {
                name: Some("Example Provider File".to_string()),
                version: "1.0.0".to_string(),
            }),
            version: Some("2.0.0"),
            on_chunk: None,
        }
    }
}

/// Installs a network-free fake provider for `config.id` and returns the guard that ends the override. The fake is
/// leaked on purpose: the registry holds `&'static` providers and a test process is short-lived. The guard removes the
/// reference when the test ends.
pub fn install_fake(config: FakeConfig) -> OverrideGuard {
    let id = config.id;
    let fake: &'static dyn SourceProvider = Box::leak(Box::new(FakeProvider {
        id: ProviderId::new(id),
        archive: config.archive.to_string(),
        entries: config
            .entries
            .iter()
            .map(|(name, text)| (name.to_string(), text.to_string()))
            .collect(),
        chunks: config.chunks.to_vec(),
        label: config.label,
        version: config.version,
        on_chunk: config.on_chunk,
    }));
    install(id, fake)
}

/// A provider that needs no network. `download` writes a small archive and its source sidecar, reports the configured
/// chunks and honours a `false` from progress; `version_of_file` and `archive_name` answer the configured values. Every
/// other port method keeps its unsupported or empty default.
struct FakeProvider {
    id: ProviderId,
    archive: String,
    entries: Vec<(String, String)>,
    chunks: Vec<(u64, u64)>,
    label: Option<ArchiveLabel>,
    version: Option<&'static str>,
    on_chunk: Option<Box<dyn Fn() + Send + Sync>>,
}

impl SourceProvider for FakeProvider {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }

    fn display_name(&self) -> &'static str {
        "Example Provider"
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities::new(SourceAuthKind::None)
    }

    fn schemes(&self) -> &'static [&'static str] {
        &["nxm"]
    }

    fn download(
        &self,
        request: SourceDownloadRequest<'_>,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<SourceDownloaded, AppError> {
        let entries: Vec<(&str, &str)> = self
            .entries
            .iter()
            .map(|(name, text)| (name.as_str(), text.as_str()))
            .collect();
        crate::test_support::make_zip(request.directory, &self.archive, &entries);
        let path = request.directory.join(&self.archive);
        crate::infra::sidecar::write_source(&path, request.source.clone());

        for &(received, total) in &self.chunks {
            if let Some(on_chunk) = &self.on_chunk {
                on_chunk();
            }

            if !progress(received, total) {
                break;
            }
        }

        Ok(SourceDownloaded {
            path,
            source: request.source,
        })
    }

    fn version_of_file<'a>(&self, _file_id: &'a str) -> Option<&'a str> {
        self.version
    }

    fn archive_name(&self, _file_name: &str) -> Option<ArchiveLabel> {
        self.label.clone()
    }
}
