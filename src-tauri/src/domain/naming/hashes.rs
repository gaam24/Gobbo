//! Stable hashes used as short ids for families, sources and profile data.

use sha2::{Digest, Sha256};

/// A short, stable suffix for a family id. It keeps two mods with the same display name apart.
pub fn short_hash(value: &str) -> String {
    // Eight hex characters are the first four bytes; do not format the whole digest and cut the string.
    digest_prefix(value, 4)
}

/// A longer stable key for a source path kept in the per-profile data store.
pub fn path_hash(value: &str) -> String {
    digest_prefix(value, 8)
}

/// Lowercase hex of the first `bytes` bytes of the SHA-256 of `value`.
fn digest_prefix(value: &str, bytes: usize) -> String {
    let digest = Sha256::digest(value.as_bytes());
    faster_hex::hex_string(&digest[..bytes])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_short_and_stable() {
        let hash = short_hash("nexus:12345");
        assert_eq!(hash.len(), 8);
        assert_eq!(hash, "0670d9ab");
        assert_ne!(hash, short_hash("nexus:67890"));
    }
}
