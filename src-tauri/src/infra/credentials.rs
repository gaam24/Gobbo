//! The provider credentials in the OS keyring. A provider chooses the account name; the value never reaches the
//! settings file or the webview.

use crate::error::AppError;

/// The service name is the app identifier, so another program called "gobbo" cannot read a key.
const SERVICE: &str = crate::infra::storage::APP_ID;

/// Reads a saved credential. `Ok(None)` means none is saved; a keyring that is locked or denied is an error, not an
/// empty key, so the interface can tell the user why the saved key is not being used.
pub fn load(account: &str) -> Result<Option<String>, AppError> {
    let entry = keyring::Entry::new(SERVICE, account)
        .map_err(|error| key_error(error, "source.key_unreadable"))?;
    match entry.get_password() {
        Ok(key) if key.is_empty() => Ok(None),
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(key_error(error, "source.key_unreadable")),
    }
}

fn key_error(error: keyring::Error, code: &str) -> AppError {
    AppError::retryable(code).with("detail", error)
}

/// Stores (or clears) a credential in the OS keyring.
pub fn save(account: &str, key: &str) -> Result<(), AppError> {
    let entry = keyring::Entry::new(SERVICE, account)
        .map_err(|error| key_error(error, "source.key_save_failed"))?;
    if key.is_empty() {
        let _ = entry.delete_credential();
        return Ok(());
    }
    entry
        .set_password(key)
        .map_err(|error| key_error(error, "source.key_save_failed"))?;
    Ok(())
}
