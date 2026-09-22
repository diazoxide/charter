//! Where the app takes its next version from, and what has to be true before it takes one.
//!
//! [`crate::adopt`] answers "what charter is this", and its one sentence —
//! [`crate::adopt::THE_APP_MOVES_IT`] — says the app is what moves it. This module is the
//! other half of that sentence: **how** the app moves itself, which stream it moves along,
//! and the shape of the things that have to be right before a downloaded bundle is allowed
//! to replace the one that is running.
//!
//! Nothing here reaches the network, and nothing here is Tauri. The app's
//! `app/src-tauri/src/updates.rs` is the only caller that does either; this is the part that
//! can be tested without a runner, a release or a keypair, and every rule it states is a rule
//! the app asks it about rather than restating.
//!
//! # The two channels
//!
//! **stable** is cut from a `v*` tag and from nothing else. **dev** is cut from any green
//! `main`. They are two manifests, two release pages and one app: the channel names which
//! [`Channel::endpoint`] the updater is pointed at, and everything after that is identical —
//! same verification, same keypair, same installer.
//!
//! **The default is stable, and it is the default in the strong sense**: it is what an
//! unreadable store, an unknown word, a store charter will not open and a platform with no
//! store at all all answer. Every way of failing to find out lands on stable, because the one
//! that must never happen by accident is a machine finding itself on dev.
//!
//! # Signing is not optional, and there is no flag that makes it optional
//!
//! Tauri's updater verifies a minisign signature over the downloaded bundle before it unpacks
//! anything, and `minisign-verify` is an ordinary dependency of the plugin rather than a
//! feature — there is no build of it that skips the check. What charter adds on top is the
//! part Tauri leaves to the application:
//!
//! * **[`pubkey_usable`]** — the app refuses to offer an update at all when the public key it
//!   was built with is not a public key. Without it the failure arrives at the END of an
//!   update: charter announces a new version, downloads twenty megabytes, and only then finds
//!   it cannot verify them. A charter that cannot verify says so before it asks.
//! * **`requireSignedVersion`** in `tauri.conf.json`, which is off by default upstream. The
//!   manifest is fetched over TLS but is **not signed**, so without it anyone who can serve a
//!   crafted manifest can pair a made-up `version` with the url and signature of a genuine
//!   OLDER release and force a downgrade onto a build with a known hole. With it, the version
//!   is read out of the signature's trusted comment, which minisign's own signature covers.
//!   charter-app has published nothing yet, so every release it will ever have carries that
//!   comment and nothing is left behind by turning it on from the first one.
//!
//! [`pubkey_usable`] is a **shape** check and says so. It cannot tell the operator's key from
//! an attacker's — nothing in a binary can, which is why the key is in the binary. What it
//! catches is the case this repository would otherwise ship by accident: a placeholder, an
//! empty string, or the untrusted-comment line pasted in instead of the key.

/// The repository the app updates itself from.
pub const REPO: &str = "https://github.com/diazoxide/charter-app";

/// The one tag that is not a version, and the whole of the dev channel's address.
///
/// GitHub has no release without a tag, so a rolling channel needs a name to hang assets on.
/// `dev` is that name: **the operator creates it once, by hand**, and nothing in CI ever
/// creates or moves a tag. That is not squeamishness about one extra ref — the standing rule
/// is that charter is prepared unasked and tagged only on the operator's word, and a workflow
/// that can write one tag is a workflow that has taken the decision about whether to write
/// tags. The release workflow refuses, with the command to run, when this release is missing.
///
/// It is a **prerelease**, which is what keeps the two channels apart on GitHub's side:
/// `/releases/latest/download/` resolves to the newest release that is *not* a prerelease, so
/// a dev build can never become what a stable machine downloads, however recent it is.
pub const DEV_TAG: &str = "dev";

/// Which stream this machine takes charter from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Channel {
    /// Cut from a `v*` tag, on the operator's word. The default everywhere.
    #[default]
    Stable,
    /// Cut from any green `main`.
    Dev,
}

impl Channel {
    /// Every channel there is, for anything that enumerates them.
    pub const ALL: [Channel; 2] = [Channel::Stable, Channel::Dev];

    /// The word charter writes down and reads back.
    pub fn name(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Dev => "dev",
        }
    }

    /// The channel this word names, or `None` for a word charter does not know.
    ///
    /// **Exact, and deliberately not lenient.** No trimming, no case folding, no prefix match:
    /// this reads a value charter itself wrote, and every other spelling is a file somebody
    /// else edited or a file that is corrupt. A reader that accepted `DEV `, `Dev` or `devel`
    /// would be deciding, on a guess, to put a machine on the channel that is not the safe
    /// one. `None` becomes [`Channel::Stable`] at the call site, with the reason recorded.
    pub fn named(word: &str) -> Option<Channel> {
        Channel::ALL.into_iter().find(|c| c.name() == word)
    }

    /// The manifest this channel's app reads, as it is named on the release page.
    pub fn manifest(self) -> &'static str {
        match self {
            Channel::Stable => "latest.json",
            Channel::Dev => "dev.json",
        }
    }

    /// The URL the updater is pointed at.
    ///
    /// **Both are `https`, and that is load-bearing rather than tidy**: Tauri refuses a
    /// non-https endpoint outright in a release build, and the only way past that refusal is
    /// `dangerousInsecureTransportProtocol`, which this repository does not set anywhere. The
    /// manifest is not signed, so the transport is the only thing standing between a hostile
    /// network and a manifest of its choosing — and `requireSignedVersion` is what stands
    /// behind it when the transport is beaten anyway.
    ///
    /// Stable reads GitHub's own "latest" pointer, which skips prereleases, so it resolves to
    /// the newest `v*` release and never to [`DEV_TAG`].
    pub fn endpoint(self) -> String {
        match self {
            Channel::Stable => format!("{REPO}/releases/latest/download/{}", self.manifest()),
            Channel::Dev => format!(
                "{REPO}/releases/download/{DEV_TAG}/{}",
                Channel::Dev.manifest()
            ),
        }
    }
}

/// The platform keys the manifest is written with, and the only ones charter publishes.
///
/// Tauri looks for `{os}-{arch}-{installer}` first and `{os}-{arch}` second, and the
/// difference between the two decides what happens to an install charter cannot update:
///
/// * **`darwin-aarch64`** — the bare key, because a Mac has one artifact. A `.app` and a
///   `.dmg` both report the `app` installer, so the bare key catches both and there is
///   nothing else it could wrongly catch.
/// * **`linux-x86_64-appimage`** — the qualified key, and the qualification is the guard. An
///   AppImage is a file in the operator's home that the app may replace; a `.deb` is owned by
///   `dpkg`, is root-owned, and must be updated by `apt` and not by charter. Publishing the
///   bare `linux-x86_64` key would make a `.deb` install fall back to it and then hand
///   AppImage bytes to `install_deb`. Publishing only the qualified key makes that install
///   answer *no update for this target*, which is the truth.
///
/// **An Intel Mac and an arm64 Linux are not here**, because the release matrix has no runner
/// for either. That is a gap and it is named rather than papered over: an operator on one gets
/// *no update available*, for ever, until a runner is added and a key with it.
pub const TARGETS: [&str; 2] = ["darwin-aarch64", "linux-x86_64-appimage"];

/// Why a public key is not one charter will offer an update against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotAKey {
    /// Nothing was configured, or the placeholder this repository ships was left in place.
    Unset,
    /// It is not base64, so it is not the `.pub` file minisign wrote.
    NotBase64,
    /// It decoded, but not to a minisign public key.
    NotMinisign,
}

impl NotAKey {
    /// What charter says out loud, in one sentence that names the remedy.
    pub fn why(self) -> &'static str {
        match self {
            NotAKey::Unset => {
                "this build carries no updater public key, so it cannot check that an update \
                 came from the operator — and charter will not install one it cannot check"
            }
            NotAKey::NotBase64 => {
                "this build's updater public key is not base64, so it is not the `.pub` file \
                 `tauri signer generate` wrote — charter will not install an update it cannot \
                 check"
            }
            NotAKey::NotMinisign => {
                "this build's updater public key is base64 but does not decode to a minisign \
                 public key — the untrusted-comment line is often pasted in place of the key \
                 below it"
            }
        }
    }
}

/// The placeholder `tauri.conf.json` carries until the operator generates the keypair.
///
/// It is spelled so it cannot be mistaken for a key by a person or by [`pubkey_usable`], and
/// it exists at all because the updater plugin's config **requires** the field: leaving it out
/// is a config the plugin refuses to deserialise, which is an app that does not start. A
/// placeholder that fails this function is a build whose updater is off and says so; an empty
/// string would be the same thing said quietly.
pub const PUBKEY_UNSET: &str = "UNSET-THE-OPERATOR-GENERATES-THIS-SEE-docs-updating.md";

/// Whether this build can verify an update at all.
///
/// **A shape check, and only a shape check.** It reads the string Tauri was configured with
/// and answers whether that string is a minisign public key — it cannot and does not answer
/// whose. The key in a binary is the trust root; nothing inside the binary can audit its own
/// trust root, and pretending otherwise would be the kind of claim ADR 0013 is about.
///
/// What it is for is the failure this repository would otherwise ship: a build that announces
/// updates, downloads them, and discovers at the last step that the placeholder in
/// `tauri.conf.json` was never replaced. charter asks this before it offers, so the operator
/// is told by a build that will not update rather than by an update that will not install.
///
/// minisign's public key file is two lines — an untrusted comment, then base64 — and Tauri's
/// `pubkey` is the base64 of that **whole file**. So a usable key decodes from base64 to text
/// that contains a second base64 line beginning `RW`, which is minisign's `Ed` algorithm tag
/// followed by a key id. Checking the tag rather than only "it decoded" is what separates a
/// key from the comment line pasted in on its own, which is the mistake that looks right.
pub fn pubkey_usable(configured: &str) -> Result<(), NotAKey> {
    let trimmed = configured.trim();
    if trimmed.is_empty() || trimmed == PUBKEY_UNSET {
        return Err(NotAKey::Unset);
    }
    let bytes = base64_bytes(trimmed).ok_or(NotAKey::NotBase64)?;
    // A `.pub` file is text. Bytes that are not are not one, which is a wrong key rather
    // than a wrong encoding — the base64 itself was fine.
    let decoded = String::from_utf8(bytes).map_err(|_| NotAKey::NotMinisign)?;
    // The key line, not the comment line: minisign writes the comment first and the key
    // second, and a `.pub` pasted in by hand often loses one of them.
    let key_line = decoded
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("untrusted comment:"))
        .ok_or(NotAKey::NotMinisign)?;
    // `RW` is minisign's Ed25519 signature algorithm tag, base64-encoded at the front of the
    // key blob: `Ed` is `0x45 0x64`, and every minisign public key starts with it. A key line
    // that decodes to fewer than the 2 tag + 8 id + 32 key bytes is not one either.
    let key_bytes = base64_bytes(key_line).ok_or(NotAKey::NotMinisign)?;
    if key_bytes.len() != 42 || &key_bytes[..2] != b"Ed" {
        return Err(NotAKey::NotMinisign);
    }
    Ok(())
}

/// Standard base64 (RFC 4648, with padding) to bytes, or `None`.
///
/// Written out rather than taken from a crate because charter-core has no base64 dependency
/// and this is the only place in it that needs one — sixteen lines against a dependency in
/// the shipped binary, for a decoder whose whole contract is "reject anything that is not
/// exactly this alphabet".
fn base64_bytes(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    let mut padding = 0usize;
    let mut chars = 0usize;
    for ch in text.bytes() {
        if ch == b'=' {
            padding += 1;
            continue;
        }
        // Padding is only ever the last one or two characters, so anything after it is junk.
        if padding > 0 {
            return None;
        }
        let value = match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a' + 26,
            b'0'..=b'9' => ch - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(value);
        bits += 6;
        chars += 1;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xff) as u8);
        }
    }
    // Counted in CHARACTERS and not in output bytes, which is the check that catches a
    // truncated encoding: `AAAAA` yields three whole bytes with six zero bits left over, so
    // every test that looks only at the output passes it. Four characters to a quantum,
    // padding included, and a remainder of one character is a quantum that cannot exist.
    if padding > 2 || chars % 4 == 1 || (chars + padding) % 4 != 0 {
        return None;
    }
    // Whatever is left over after the last whole byte has to be the encoder's zero fill.
    // Without this, two different strings decode to the same bytes and a key can be spelled
    // more than one way.
    if bits > 0 && (acc & ((1 << bits) - 1)) != 0 {
        return None;
    }
    Some(out)
}

/// Whether a build should go looking for an update on its own.
///
/// **Neither a test build nor a development build asks the network.** The scenario suite
/// starts the app dozens of times in a run, and a fenced build is exactly the build that must
/// not reach anything outside its own tree (charter-app#129) — a check is a reach out of it,
/// it is slow, and it is one more thing that fails when GitHub does. A debug build is the
/// developer's own, whose version is always `0.1.0` and would be offered every release there
/// has ever been.
///
/// Handed its two facts rather than reading `cfg!` itself, so the rule is testable in both
/// directions from one build. The app passes [`crate::fence::FENCED`] and
/// `cfg!(debug_assertions)`.
pub fn checks_on_its_own(fenced: bool, debug_build: bool) -> bool {
    !fenced && !debug_build
}

/// How long after a launch the first automatic check happens.
///
/// Not at launch: a launch already has a window to draw, a plane to read and sessions to
/// start, and the network call that is least urgent of all of them should not be in that
/// crowd. A minute is long enough to be out of it and short enough that an operator who
/// leaves charter open all day is told the same day.
pub const FIRST_CHECK_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

/// How often after that.
///
/// charter releases on the operator's word, not on a schedule, so this is a poll for something
/// that happens a few times a month. Six hours costs four requests a day against a static
/// file and means an operator who never quits charter still hears about a release the same
/// day.
pub const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_charter_did_not_write_is_not_a_channel() {
        assert_eq!(Channel::named("stable"), Some(Channel::Stable));
        assert_eq!(Channel::named("dev"), Some(Channel::Dev));
        // Every one of these would put a machine on a channel it did not ask for, if the
        // reader were lenient in the direction that looks helpful.
        for word in [
            "", "DEV", "Dev", " dev", "dev ", "dev\n", "devel", "development", "beta", "nightly",
            "STABLE", "latest",
        ] {
            assert_eq!(Channel::named(word), None, "{word:?} was read as a channel");
        }
    }

    #[test]
    fn the_default_is_stable_and_every_channel_round_trips_its_word() {
        assert_eq!(Channel::default(), Channel::Stable);
        for channel in Channel::ALL {
            assert_eq!(Channel::named(channel.name()), Some(channel));
        }
        assert_ne!(Channel::Stable.name(), Channel::Dev.name());
    }

    #[test]
    fn both_endpoints_are_https_and_the_two_channels_never_read_one_file() {
        let stable = Channel::Stable.endpoint();
        let dev = Channel::Dev.endpoint();
        for url in [&stable, &dev] {
            assert!(
                url.starts_with("https://"),
                "{url} is not https, and a release build refuses a non-https endpoint"
            );
        }
        assert_ne!(stable, dev);
        assert_ne!(Channel::Stable.manifest(), Channel::Dev.manifest());
        // Stable goes through GitHub's `latest` pointer, which skips prereleases — that is
        // what keeps the rolling dev release from ever becoming what a stable machine reads.
        assert!(stable.contains("/releases/latest/download/"), "{stable}");
        assert!(!stable.contains(DEV_TAG), "{stable} can resolve to the dev release");
        assert!(dev.contains(&format!("/releases/download/{DEV_TAG}/")), "{dev}");
    }

    /// A real minisign public key file, as `tauri signer generate` writes it, base64'd whole.
    ///
    /// Generated for this test and for nothing else: it has no private half anywhere, which
    /// is the point — the test needs a key SHAPED like the operator's, and must never contain
    /// one that is.
    fn a_public_key() -> String {
        // `Ed`, an eight byte key id, a thirty-two byte key — minisign's layout, built here
        // rather than pasted so the shape under test is stated and not copied.
        let mut blob = b"Ed".to_vec();
        blob.extend((0..40u8).map(|i| i.wrapping_mul(13).wrapping_add(7)));
        let file = format!(
            "untrusted comment: minisign public key 6E9C2BBF1A3D4E57\n{}\n",
            encode(&blob)
        );
        encode(file.as_bytes())
    }

    /// Base64 for the tests, so the decoder under test is never its own oracle.
    fn encode(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(ALPHABET[((n >> (18 - 6 * i)) & 0x3f) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    #[test]
    fn a_real_public_key_is_usable() {
        assert_eq!(pubkey_usable(&a_public_key()), Ok(()));
    }

    #[test]
    fn the_placeholder_this_repository_ships_is_not_a_key() {
        assert_eq!(pubkey_usable(PUBKEY_UNSET), Err(NotAKey::Unset));
        assert_eq!(pubkey_usable(""), Err(NotAKey::Unset));
        assert_eq!(pubkey_usable("   \n "), Err(NotAKey::Unset));
        // And the sentence names the remedy rather than only the fault.
        assert!(NotAKey::Unset.why().contains("will not install"));
    }

    #[test]
    fn the_comment_line_on_its_own_is_the_mistake_that_looks_right() {
        // What an operator pastes when they copy the first line of the `.pub` file. It is
        // base64, it decodes, it is text — and it is not a key.
        let comment_only = encode(b"untrusted comment: minisign public key 6E9C2BBF1A3D4E57\n");
        assert_eq!(pubkey_usable(&comment_only), Err(NotAKey::NotMinisign));
    }

    #[test]
    fn a_key_that_is_not_minisigns_is_refused() {
        // Right length, wrong algorithm tag: minisign's `Ed` is the first two bytes of every
        // public key it writes, and a blob that is merely 42 bytes long is not one.
        let mut wrong = vec![b'X', b'Y'];
        wrong.extend(std::iter::repeat_n(0u8, 40));
        let file = format!("untrusted comment: x\n{}\n", encode(&wrong));
        assert_eq!(pubkey_usable(&encode(file.as_bytes())), Err(NotAKey::NotMinisign));

        // Right tag, wrong length: 8 bytes of key id and 32 of key is the whole of it.
        let mut short = vec![b'E', b'd'];
        short.extend(std::iter::repeat_n(0u8, 20));
        let file = format!("untrusted comment: x\n{}\n", encode(&short));
        assert_eq!(pubkey_usable(&encode(file.as_bytes())), Err(NotAKey::NotMinisign));
    }

    #[test]
    fn something_that_is_not_base64_at_all_is_refused_as_that() {
        for junk in [
            "not base64!",
            "RWQ*****",
            "-----BEGIN PUBLIC KEY-----",
            "AAAA=AAA",  // padding in the middle
            "AAAAA",     // not a whole quantum
            "QQ==QQ==",  // two quanta, the first padded
        ] {
            assert_eq!(
                pubkey_usable(junk),
                Err(NotAKey::NotBase64),
                "{junk:?} decoded as base64"
            );
        }
    }

    #[test]
    fn base64_that_decodes_to_bytes_that_are_not_text_is_not_a_key() {
        // The encoding was fine; what came out was not a `.pub` file, so the answer names
        // the key and not the base64.
        assert_eq!(
            pubkey_usable(&encode(&[0xff, 0xfe, 0xfd])),
            Err(NotAKey::NotMinisign)
        );
    }

    #[test]
    fn a_test_build_and_a_development_build_never_reach_the_network_on_their_own() {
        assert!(!checks_on_its_own(true, false), "a fenced build checked");
        assert!(!checks_on_its_own(false, true), "a debug build checked");
        assert!(!checks_on_its_own(true, true));
        assert!(checks_on_its_own(false, false), "a shipped build did not check");
    }

    #[test]
    fn the_linux_key_is_qualified_and_the_macos_key_is_not() {
        // The whole of why: a `.deb` install must find NO entry rather than fall back to one
        // holding AppImage bytes. Tauri looks for `{os}-{arch}-{installer}` and then
        // `{os}-{arch}`, so a bare `linux-x86_64` key is what a deb install would land on.
        assert!(TARGETS.contains(&"linux-x86_64-appimage"));
        assert!(
            !TARGETS.contains(&"linux-x86_64"),
            "the bare linux key would be served to a .deb install"
        );
        assert!(TARGETS.contains(&"darwin-aarch64"));
    }

    #[test]
    fn base64_round_trips_every_byte() {
        // The decoder is hand-written, so it is checked against every length that has a
        // different amount of padding and against every byte value.
        for len in 0..=64usize {
            let bytes: Vec<u8> = (0..len).map(|i| (i * 7 % 256) as u8).collect();
            assert_eq!(base64_bytes(&encode(&bytes)).as_deref(), Some(&bytes[..]));
        }
        let every: Vec<u8> = (0..=255u8).collect();
        assert_eq!(base64_bytes(&encode(&every)).as_deref(), Some(&every[..]));
    }
}
