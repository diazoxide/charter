//! The masked shape of a secret — `charter/secrets/fingerprint.py`: a keyed fingerprint and a
//! size band.
//!
//! `secret get` without `--reveal` answers "is it present", "is it the same value the other
//! vault holds" and "did the whole token land" — and must NOT answer "is the value
//! `Summer2024!`". An unkeyed digest plus an exact length is an offline oracle for a guessed
//! value (#436), so:
//!
//! - **the digest is keyed**: `HMAC-SHA256(plane key, value)`, the key 32 random bytes kept
//!   0600 at `<state>/fingerprint.key`, generated on first use. Comparable within one plane,
//!   uncomputable without the key;
//! - **the size is a band**, not a count.
//!
//! A plane whose key cannot be read or made prints NO fingerprint — never an unkeyed one.

use std::path::{Path, PathBuf};

use hmac::{Hmac, KeyInit as _, Mac};
use sha2::Sha256;

use super::Ctx;

/// 256 bits. A key file of any other length is regenerated, never used short.
pub const KEY_BYTES: usize = 32;

/// The key's file name under the state directory.
pub const KEY_FILE: &str = "fingerprint.key";

/// Where the plane's fingerprint key lives.
pub fn key_path(ctx: &Ctx) -> PathBuf {
    ctx.state.join(KEY_FILE)
}

/// `_key`: the plane's key, generating it on first use; `None` when it can be neither read
/// nor made.
fn key(p: &Path) -> Option<Vec<u8>> {
    if let Ok(existing) = std::fs::read(p)
        && existing.len() == KEY_BYTES
    {
        return Some(existing);
    }
    let parent = p.parent()?;
    super::make_private_dir(parent).ok()?;
    let mut new = [0u8; KEY_BYTES];
    getrandom::fill(&mut new).ok()?;
    // Replaced whole, 0600 before a byte of the key lands, and never through a link (#434):
    // a link at the key is refused, and a crash leaves the old key rather than a short one
    // the next run would silently regenerate.
    crate::rewrite::replace(parent, p, &new, crate::rewrite::Mode::Secret).ok()?;
    Some(new.to_vec())
}

/// `fingerprint`: 12 hex characters of `HMAC-SHA256(key, value)`, or `None` with no key.
pub fn fingerprint(ctx: &Ctx, value: &str) -> Option<String> {
    let k = key(&key_path(ctx))?;
    let mut mac = Hmac::<Sha256>::new_from_slice(&k).ok()?;
    mac.update(value.as_bytes());
    let digest = mac.finalize().into_bytes();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    Some(hex[..12].to_string())
}

/// `size_band`: `empty`, `1–15 bytes`, `16–31 bytes`, … `1024+ bytes`, counted in UTF-8 bytes.
pub fn size_band(value: &str) -> String {
    let n = value.len();
    if n == 0 {
        return "empty".into();
    }
    if n < 16 {
        return "1–15 bytes".into();
    }
    if n >= 1024 {
        return "1024+ bytes".into();
    }
    let lo = 1usize << (usize::BITS - 1 - n.leading_zeros());
    format!("{lo}–{} bytes", (lo << 1) - 1)
}

/// `masked`: the size band, and the keyed fingerprint when this plane has a key.
pub fn masked(ctx: &Ctx, value: &str) -> String {
    match fingerprint(ctx, value) {
        Some(fp) => format!("{} · fp:{fp}", size_band(value)),
        None => size_band(value),
    }
}

#[cfg(test)]
#[path = "fingerprint_tests.rs"]
mod tests;
