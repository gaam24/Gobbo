//! The system handlers of the providers' download links. The app settings say which providers Gobbo takes; this
//! module makes the system agree, checks it at startup and after the window comes back, and reports what the system
//! does now.

use tauri::AppHandle;
use tauri_plugin_deep_link::DeepLinkExt;

use crate::domain::source::{SourceLinkStatus, SourceLinksReport};
use crate::error::{AppError, ErrorKind};
use crate::providers;

/// What to do with one scheme so the system matches the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Register,
    Unregister,
    Leave,
}

/// The action for a scheme the settings take or give up, and the handler the system has now.
const fn action(taken: bool, registered: bool) -> Action {
    match (taken, registered) {
        (true, false) => Action::Register,
        (false, true) => Action::Unregister,
        _ => Action::Leave,
    }
}

/// The provider and scheme pairs the settings take, in the registry order. Every provider with links is included when
/// its id is in `taken`, so a new provider needs no change here.
fn taken_schemes(taken: &[String]) -> Vec<(String, &'static str)> {
    providers::providers()
        .iter()
        .filter(|provider| taken.iter().any(|id| id.as_str() == provider.id().as_str()))
        .flat_map(|provider| {
            let id = provider.id().as_str().to_string();
            provider
                .schemes()
                .iter()
                .map(move |&scheme| (id.clone(), scheme))
        })
        .collect()
}

/// Makes Gobbo the handler of every scheme the settings take, when another program has it. A failure for one scheme
/// does not stop the others; the answer says what the system does now and which providers Gobbo took back.
pub fn repair(app: &AppHandle, taken: &[String]) -> SourceLinksReport {
    let before = status(app, taken);
    #[cfg(any(windows, target_os = "linux"))]
    {
        let links = app.deep_link();

        for status in &before {
            // Best effort: the status below says which schemes are still not Gobbo's.
            if !status.registered {
                let _ = links.register(&status.scheme);
            }
        }
    }

    let after = status(app, taken);
    let repaired = repaired_providers(&before, &after);
    SourceLinksReport {
        statuses: after,
        repaired,
    }
}

/// The providers that had a scheme another program handled before the repair and Gobbo handles after it.
fn repaired_providers(before: &[SourceLinkStatus], after: &[SourceLinkStatus]) -> Vec<String> {
    let mut repaired: Vec<String> = Vec::new();

    for previous in before.iter().filter(|status| !status.registered) {
        let now = after.iter().any(|status| {
            status.provider == previous.provider
                && status.scheme == previous.scheme
                && status.registered
        });

        if now && !repaired.contains(&previous.provider) {
            repaired.push(previous.provider.clone());
        }
    }

    repaired
}

fn link_error(scheme: &str, detail: impl std::fmt::Display) -> AppError {
    AppError::new("source.links_failed")
        .with("scheme", scheme)
        .with("detail", detail)
        .kind(ErrorKind::Permanent)
}

/// The handler the system has for each scheme the settings take, without changing anything.
#[cfg(any(windows, target_os = "linux"))]
pub fn status(app: &AppHandle, taken: &[String]) -> Vec<SourceLinkStatus> {
    let links = app.deep_link();
    taken_schemes(taken)
        .into_iter()
        .map(|(provider, scheme)| SourceLinkStatus {
            provider,
            scheme: scheme.to_string(),
            registered: links.is_registered(scheme).unwrap_or(false),
        })
        .collect()
}

/// The handler the system has for each scheme the settings take, without changing anything.
#[cfg(not(any(windows, target_os = "linux")))]
pub fn status(_app: &AppHandle, _taken: &[String]) -> Vec<SourceLinkStatus> {
    Vec::new()
}

/// Makes the system handlers match the settings. A registration that fails stops the save, so the switch does not
/// claim a handler the system does not have.
#[cfg(any(windows, target_os = "linux"))]
pub fn apply(app: &AppHandle, taken: &[String]) -> Result<(), AppError> {
    let links = app.deep_link();

    for provider in providers::providers() {
        let wanted = taken.iter().any(|id| id.as_str() == provider.id().as_str());

        for &scheme in provider.schemes() {
            let registered = links.is_registered(scheme).unwrap_or(false);

            match action(wanted, registered) {
                Action::Register => {
                    links
                        .register(scheme)
                        .map_err(|error| link_error(scheme, error))?;
                }
                Action::Unregister => {
                    links
                        .unregister(scheme)
                        .map_err(|error| link_error(scheme, error))?;
                }
                Action::Leave => {}
            }
        }
    }

    Ok(())
}

/// Makes the system handlers match the settings. Nothing can be registered on this platform.
#[cfg(not(any(windows, target_os = "linux")))]
pub fn apply(_app: &AppHandle, _taken: &[String]) -> Result<(), AppError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Action, action, repaired_providers, taken_schemes};
    use crate::domain::source::SourceLinkStatus;

    fn status(provider: &str, scheme: &str, registered: bool) -> SourceLinkStatus {
        SourceLinkStatus {
            provider: provider.to_string(),
            scheme: scheme.to_string(),
            registered,
        }
    }

    #[test]
    fn a_handler_changes_only_when_the_setting_and_the_system_disagree() {
        assert_eq!(action(true, false), Action::Register);
        assert_eq!(action(false, true), Action::Unregister);
        assert_eq!(action(true, true), Action::Leave);
        assert_eq!(action(false, false), Action::Leave);
    }

    #[test]
    fn every_provider_with_links_is_checked_when_taken() {
        let schemes = taken_schemes(&["nexus".to_string(), "thunderstore".to_string()]);
        assert!(schemes.contains(&("nexus".to_string(), "nxm")));
        assert!(schemes.contains(&("thunderstore".to_string(), "ror2mm")));
        assert_eq!(schemes.len(), 2);
    }

    #[test]
    fn a_provider_left_out_is_not_checked() {
        assert_eq!(
            taken_schemes(&["thunderstore".to_string()]),
            vec![("thunderstore".to_string(), "ror2mm")]
        );
        assert!(taken_schemes(&[]).is_empty());
    }

    #[test]
    fn a_provider_is_named_once_when_more_than_one_scheme_is_taken_back() {
        let before = [
            status("nexus", "nxm", false),
            status("nexus", "nxm-alt", false),
            status("thunderstore", "ror2mm", true),
        ];
        let after = before
            .iter()
            .map(|status| SourceLinkStatus {
                registered: true,
                ..status.clone()
            })
            .collect::<Vec<_>>();
        assert_eq!(repaired_providers(&before, &after), ["nexus"]);
    }
}
