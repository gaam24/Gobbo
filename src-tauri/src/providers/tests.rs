use std::path::Path;

use super::routing::route_from;
use super::*;
use crate::domain::rate::{AccountTier, DownloadPolicy};
use crate::domain::source::{DownloadRoute, ProviderId, SourceAuthKind, SourceCapabilities};

/// Every adapter answers the same port questions, and a provider cannot be registered without a case here.
#[test]
fn every_registered_provider_keeps_the_port_contract() {
    testing::check_all();
}

/// Reads the provider id declared by the adapter in `source`. Every adapter states it as a plain string literal, so
/// this needs no Rust parser: find the declaration, then its closing quote.
fn adapter_id<'a>(source: &'a str, path: &Path) -> &'a str {
    const DECLARATION: &str = "pub const ID: &str = \"";
    let start = source
        .find(DECLARATION)
        .unwrap_or_else(|| panic!("{} must declare pub const ID", path.display()));
    let rest = &source[start + DECLARATION.len()..];
    let end = rest
        .find('"')
        .unwrap_or_else(|| panic!("{} has an unterminated provider id", path.display()));
    &rest[..end]
}

/// An adapter that implements the port but is missing from `PROVIDERS` is invisible to `provider`, even when its own
/// contract case passes. Scan the adapters in the source tree so adding one fails the test until it is registered.
#[test]
fn every_provider_adapter_is_registered_under_its_id() {
    let providers_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/providers");
    let mut seen = std::collections::BTreeSet::new();

    for entry in std::fs::read_dir(&providers_dir).expect("the providers folder must exist") {
        let entry = entry.expect("a provider entry must be readable");
        let path = entry.path().join("mod.rs");
        if !path.is_file() {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("a provider adapter must be readable");
        if !source.contains("impl SourceProvider for") {
            continue;
        }

        let id = adapter_id(&source, &path);
        assert!(
            !id.is_empty(),
            "{} declares an empty provider id",
            path.display()
        );
        assert!(
            seen.insert(id.to_string()),
            "the provider id {id:?} is declared more than once"
        );
        assert!(
            crate::providers::provider(id).is_some(),
            "the provider {id:?} in {} is not registered",
            path.display()
        );
    }

    assert!(!seen.is_empty(), "the scan must find at least one adapter");
}

#[test]
fn an_invalid_nexus_link_keeps_the_provider_error() {
    let error = provider("nexus")
        .unwrap()
        .parse_link("not a link")
        .unwrap_err();
    assert_eq!(error.code, "nexus.bad_link");
}

#[test]
fn source_links_use_a_registered_scheme() {
    assert!(is_source_link("nxm://examplegame/mods/1/files/2"));
    assert!(is_source_link("NXM://examplegame/mods/1/files/2"));
    assert!(is_source_link(
        "ror2mm://v1/install/thunderstore.io/other_owner/other_mod/3.1.25/"
    ));
    assert!(!is_source_link("https://example.com/mod"));
    assert!(!is_source_link("nxm"));
    assert_eq!(
        for_link("nxm://examplegame/mods/1/files/2").map(|provider| provider.id().to_string()),
        Some("nexus".to_string())
    );
    assert_eq!(
        for_link("ror2mm://v1/install/thunderstore.io/other_owner/other_mod/3.1.25/")
            .map(|provider| provider.id().to_string()),
        Some("thunderstore".to_string())
    );
}

#[test]
fn a_provider_keeps_its_key_under_one_account() {
    assert_eq!(
        provider("nexus").and_then(SourceProvider::credential_account),
        Some("nexus_api_key")
    );
    assert!(provider("not_registered").is_none());
    let Err(error) = require("not_registered") else {
        panic!("an unregistered provider must not resolve");
    };
    assert_eq!(error.code, "source.unknown");
}

#[test]
fn the_settings_list_says_which_provider_needs_a_key_and_has_links() {
    let nexus = info(provider("nexus").expect("nexus must be registered"));
    assert_eq!(nexus.name, "Nexus");
    assert!(nexus.needs_key && nexus.has_key_page && nexus.direct_download);
    assert_eq!(nexus.schemes, ["nxm"]);

    let thunderstore = info(provider("thunderstore").expect("thunderstore must be registered"));
    assert!(!thunderstore.needs_key && !thunderstore.has_key_page && thunderstore.direct_download);
    assert_eq!(thunderstore.schemes, ["ror2mm"]);
}

/// A provider that needs no key has no account to check, so it downloads directly.
#[test]
fn a_provider_without_a_key_downloads_directly() {
    let thunderstore = provider("thunderstore").expect("thunderstore must be registered");
    assert_eq!(download_route(thunderstore), DownloadRoute::Direct);
    assert_eq!(info(thunderstore).download_route, DownloadRoute::Direct);
}

#[test]
fn the_route_is_browser_unless_a_direct_account_is_possible() {
    // No direct capability, a missing key, or a known account without direct downloads: the browser.
    assert_eq!(route_from(false, false, Some(true)), DownloadRoute::Browser);
    assert_eq!(route_from(true, true, Some(true)), DownloadRoute::Browser);
    assert_eq!(route_from(true, false, Some(false)), DownloadRoute::Browser);
    // A saved key with an unknown or a direct account tries the direct download.
    assert_eq!(route_from(true, false, None), DownloadRoute::Direct);
    assert_eq!(route_from(true, false, Some(true)), DownloadRoute::Direct);
}

/// A provider with no keyring account, so the route reads only the cached account tier. `known` stands in for what
/// the backend learned when the key was checked or a paid download was refused.
struct TierProvider {
    direct: bool,
    known: Option<bool>,
}

impl SourceProvider for TierProvider {
    fn id(&self) -> ProviderId {
        ProviderId::new("tier-test")
    }

    fn capabilities(&self) -> SourceCapabilities {
        let mut capabilities = SourceCapabilities::new(SourceAuthKind::None);
        capabilities.direct_download = self.direct;
        capabilities
    }

    fn known_direct_download(&self, _key: &str) -> Option<bool> {
        self.known
    }
}

#[test]
fn a_known_free_account_is_routed_to_the_browser_and_reports_payment_required() {
    let free = TierProvider {
        direct: true,
        known: Some(false),
    };
    assert_eq!(download_route(&free), DownloadRoute::Browser);
    let error = browser_route_error(&free).expect("a browser route has an error");
    assert_eq!(error.code, PAYMENT_REQUIRED);
}

#[test]
fn an_unknown_or_direct_account_downloads_directly_without_an_error() {
    let unknown = TierProvider {
        direct: true,
        known: None,
    };
    assert_eq!(download_route(&unknown), DownloadRoute::Direct);
    assert!(browser_route_error(&unknown).is_none());
}

#[test]
fn a_provider_without_a_documented_ceiling_does_not_pace() {
    let thunderstore = provider("thunderstore").expect("thunderstore must be registered");
    for tier in [
        AccountTier::Anonymous,
        AccountTier::Free,
        AccountTier::Premium,
    ] {
        assert_eq!(
            thunderstore.download_policy(tier),
            DownloadPolicy::Unlimited
        );
        assert_eq!(
            TierProvider {
                direct: true,
                known: None
            }
            .download_policy(tier),
            DownloadPolicy::Unlimited,
            "a provider without a documented limit must not slow downloads down"
        );
    }
}

#[test]
fn a_provider_without_direct_download_keeps_its_unsupported_error() {
    let no_download = TierProvider {
        direct: false,
        known: Some(true),
    };
    assert_eq!(download_route(&no_download), DownloadRoute::Browser);
    let error = browser_route_error(&no_download).expect("a browser route has an error");
    assert_eq!(error.code, "source.unsupported");
}

#[test]
fn an_unknown_provider_id_and_a_missing_map_entry_route_to_the_browser() {
    assert_eq!(
        download_route_for_id("not_registered"),
        DownloadRoute::Browser
    );
    assert_eq!(route_or_browser(None), DownloadRoute::Browser);
    assert_eq!(
        route_or_browser(Some(DownloadRoute::Direct)),
        DownloadRoute::Direct
    );
}
