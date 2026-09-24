//! The keyed fingerprint: where its key lives, that it is the key's HMAC and nothing else, and
//! that a plane without a key prints no fingerprint at all.

use super::*;
use crate::secrets::{Ctx, Env};

/// A plane whose state directory is `tmp/.charter`.
fn plane(tmp: &tempfile::TempDir) -> Ctx {
    Ctx::new(tmp.path(), Env::of(&[]))
}

/// `HMAC-SHA256` under the key bytes 0, 1, … 31, computed independently of this crate:
/// `hmac.new(bytes(range(32)), value, sha256).hexdigest()[:12]` in Python.
const KNOWN_KEY_HUNTER2: &str = "8629d712da36";
const KNOWN_KEY_EMPTY: &str = "d38b42096d80";

fn known_key(ctx: &Ctx) {
    std::fs::create_dir_all(&ctx.state).unwrap();
    let key: Vec<u8> = (0u8..32).collect();
    std::fs::write(ctx.state.join(KEY_FILE), key).unwrap();
}

#[test]
fn the_key_is_the_state_directorys_fingerprint_key_file() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = plane(&tmp);
    assert_eq!(key_path(&ctx), tmp.path().join(".charter/fingerprint.key"));
}

#[test]
fn a_fingerprint_is_the_first_twelve_hex_of_the_hmac_under_the_planes_key() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = plane(&tmp);
    known_key(&ctx);
    assert_eq!(
        fingerprint(&ctx, "hunter2").as_deref(),
        Some(KNOWN_KEY_HUNTER2)
    );
    assert_eq!(fingerprint(&ctx, "").as_deref(), Some(KNOWN_KEY_EMPTY));
    assert_eq!(
        masked(&ctx, "hunter2"),
        format!("1–15 bytes · fp:{KNOWN_KEY_HUNTER2}")
    );
}

#[test]
fn a_plane_with_no_key_gets_one_of_32_bytes_at_0600_and_keeps_using_it() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = plane(&tmp);
    let first = fingerprint(&ctx, "hunter2").expect("a key is made on first use");
    assert_eq!(first.len(), 12);
    assert!(first.chars().all(|c| c.is_ascii_hexdigit()), "{first}");
    let key = std::fs::read(key_path(&ctx)).unwrap();
    assert_eq!(key.len(), KEY_BYTES);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(key_path(&ctx))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    // The same value under the same key; a different value differs.
    assert_eq!(
        fingerprint(&ctx, "hunter2").as_deref(),
        Some(first.as_str())
    );
    assert_ne!(
        fingerprint(&ctx, "hunter3").as_deref(),
        Some(first.as_str())
    );
    assert_eq!(
        std::fs::read(key_path(&ctx)).unwrap(),
        key,
        "never regenerated"
    );
}

#[test]
fn a_key_file_of_the_wrong_length_is_replaced_never_used_short() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = plane(&tmp);
    std::fs::create_dir_all(&ctx.state).unwrap();
    std::fs::write(key_path(&ctx), b"short").unwrap();
    let fp = fingerprint(&ctx, "hunter2").unwrap();
    let key = std::fs::read(key_path(&ctx)).unwrap();
    assert_eq!(key.len(), KEY_BYTES);
    let mut mac = Hmac::<Sha256>::new_from_slice(b"short").unwrap();
    mac.update(b"hunter2");
    let short: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_ne!(fp, short[..12], "the short key was not used");
}

#[test]
fn a_plane_whose_key_can_be_neither_read_nor_made_prints_only_the_size_band() {
    let tmp = tempfile::tempdir().unwrap();
    // The state directory's place is taken by a file, so no key can be made under it.
    let blocked = tmp.path().join("blocked");
    std::fs::write(&blocked, b"").unwrap();
    let state = blocked.join("state").to_string_lossy().into_owned();
    let ctx = Ctx::new(tmp.path(), Env::of(&[("CHARTER_HOME", &state)]));
    assert_eq!(fingerprint(&ctx, "hunter2"), None);
    assert_eq!(masked(&ctx, "hunter2"), "1–15 bytes");
    assert_eq!(masked(&ctx, ""), "empty");
}
