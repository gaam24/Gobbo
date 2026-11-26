use crate::commands::helpers::blocking;
use crate::domain::source::{SourceAccount, SourceProviderInfo};
use crate::error::AppError;
use crate::providers;

/// Every provider this build knows, in the order the settings list them.
#[tauri::command]
#[specta::specta]
pub fn source_providers() -> Vec<SourceProviderInfo> {
    providers::providers()
        .iter()
        .map(|provider| providers::info(*provider))
        .collect()
}

/// Opens the page where the user makes a provider's key, in the browser. The provider owns the URL.
#[tauri::command]
#[specta::specta]
pub async fn open_source_key_page(provider: String) -> Result<(), AppError> {
    blocking(move || {
        let url = providers::require(&provider)?
            .key_page()
            .ok_or_else(|| providers::unsupported_by(&provider))?;
        tauri_plugin_opener::open_url(url, None::<&str>).map_err(AppError::unexpected)
    })
    .await
}

/// Whether a provider key is saved. A provider that needs no key answers `true`.
#[tauri::command]
#[specta::specta]
pub async fn source_key_saved(provider: String) -> Result<bool, AppError> {
    blocking(move || Ok(providers::saved_key(providers::require(&provider)?)?.is_some())).await
}

/// Stores (or clears) a provider key in the OS credential store.
#[tauri::command]
#[specta::specta]
pub async fn save_source_key(provider: String, key: String) -> Result<(), AppError> {
    blocking(move || {
        let source = providers::require(&provider)?;
        let Some(account) = source.credential_account() else {
            return Ok(());
        };
        // A replaced or cleared key makes its cached account tier stale. Drop it so the next download validates the
        // key now in the credential store instead of reusing the old account's pacing.
        if provider == crate::providers::nexus::ID
            && let Ok(Some(old_key)) = providers::saved_key(source)
        {
            crate::providers::nexus::forget_tier(&old_key);
        }
        crate::infra::credentials::save(account, &key)
    })
    .await
}

/// Checks a provider key at the source; without an argument it checks the saved one.
#[tauri::command]
#[specta::specta]
pub async fn validate_source_key(
    provider: String,
    api_key: Option<String>,
) -> Result<SourceAccount, AppError> {
    blocking(move || {
        let source = providers::require(&provider)?;
        let key = match api_key.filter(|key| !key.trim().is_empty()) {
            Some(key) => key,
            None if source.credential_account().is_none() => {
                return Err(providers::unsupported_by(&provider));
            }
            None => providers::required_key(source)?,
        };
        source.validate_key(&key)
    })
    .await
}
