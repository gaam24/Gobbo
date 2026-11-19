//! Which downloads are paced, and how. The decision lives with the providers, not in `domain` or `infra`: a lower
//! layer receives a policy, never a provider name.

use std::time::Duration;

use crate::domain::rate::{AccountTier, DownloadPolicy};

/// More than 200 downloads in five minutes can get a free account's IP suspended for ten minutes. The number comes
/// from the community downloaders that pace whole collections; it is deliberately a ceiling, not a target.
const FREE_MAX: u32 = 200;
const FREE_WINDOW: Duration = Duration::from_mins(5);

/// The pacing rule for one provider and account tier. Anything without a documented limit stays unlimited: slowing
/// those downloads down would only make the user wait for no reason.
pub fn download_policy(provider: &str, tier: AccountTier) -> DownloadPolicy {
    if provider == super::nexus::ID && tier == AccountTier::Free {
        DownloadPolicy::WindowBudget {
            max: FREE_MAX,
            window: FREE_WINDOW,
        }
    } else {
        DownloadPolicy::Unlimited
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_free_tier_of_the_provider_with_a_known_ceiling_is_paced() {
        assert_eq!(
            download_policy(super::super::nexus::ID, AccountTier::Free),
            DownloadPolicy::WindowBudget {
                max: 200,
                window: Duration::from_mins(5)
            }
        );
        assert_eq!(
            download_policy(super::super::nexus::ID, AccountTier::Premium),
            DownloadPolicy::Unlimited
        );
    }

    #[test]
    fn an_unknown_provider_and_direct_files_are_never_paced() {
        for provider in [
            super::super::thunderstore::ID,
            "example-provider",
            "collection",
        ] {
            for tier in [
                AccountTier::Anonymous,
                AccountTier::Free,
                AccountTier::Premium,
            ] {
                assert_eq!(
                    download_policy(provider, tier),
                    DownloadPolicy::Unlimited,
                    "{provider} must not be slowed down by another provider's limit"
                );
            }
        }
    }
}
