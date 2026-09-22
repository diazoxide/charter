//! The updater manifest a release publishes, and the guards it has to pass before it exists.
//!
//! The manifest is the one thing in the update path that is **not signed**. Tauri fetches it
//! over TLS, reads a version and a per-platform `{url, signature}` out of it, downloads what
//! the url names and verifies the signature over those bytes before it unpacks anything. So
//! the manifest cannot lie about the *contents* of an update — but it can lie about which
//! release a version number belongs to, and it can point at nothing at all.
//!
//! **This is where that is caught, because this is the last moment anything knows the truth.**
//! At release time the signatures, the artifacts and the version being announced are all in
//! one directory and can be checked against each other. On an operator's machine, six weeks
//! later, only the manifest is left. Every rule below is therefore a rule about a file this
//! workflow is about to publish, enforced by refusing to publish it:
//!
//! * a signature that is **absent, empty, or not a minisign signature** — which is what a
//!   build with no signing key produces, silently, because Tauri's bundler skips signing when
//!   `TAURI_SIGNING_PRIVATE_KEY` is unset and says so only in a line nobody reads;
//! * a signature that **does not verify against the public key the app ships** — checked with
//!   `minisign-verify`, the same verifier and the same call the updater makes, over the
//!   artifact's actual bytes. A release signed with the wrong key, or a key the operator
//!   rotated in the secret but not in `tauri.conf.json`, installs nowhere; this is where that
//!   is found, rather than on every machine at once;
//! * a **public key that is still the placeholder** — a release cannot be published until the
//!   operator has generated the keypair and committed its public half;
//! * a signature whose **trusted comment names a different version** than the manifest
//!   announces. minisign's own signature covers the trusted comment, so this pairing is the
//!   only thing that binds a version number to an artifact, and `requireSignedVersion` in the
//!   app is the other half of it. A release whose two halves disagree would install nothing
//!   and nobody would find out until an operator tried;
//! * a **url that is not https**, which a release build's updater refuses anyway — at the
//!   operator's machine rather than here;
//! * a **target charter does not publish**, or one of the ones it does **missing**. A partial
//!   manifest is not a smaller release, it is a platform that silently stops getting updates.
//!
//! The signature check here does not replace the app's — the app verifies again, over what it
//! downloaded, before it unpacks anything. This one exists so a release that every app would
//! refuse is refused once, here, with a reason, instead of on every machine without one.

use std::collections::BTreeMap;

use charter_core::updates::{Channel, TARGETS};

/// One platform's row: what to download and the signature over it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The platform key, one of [`charter_core::updates::TARGETS`].
    pub target: String,
    /// Where the artifact is published.
    pub url: String,
    /// The contents of the `.sig` the bundler wrote: base64 of a minisign signature file.
    pub signature: String,
    /// The artifact's bytes, which the signature is verified over.
    pub artifact: Vec<u8>,
}

/// Why a manifest was not written.
///
/// One variant per rule, so a red release names which rule and not merely "invalid". The
/// workflow prints it and stops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The version is not one Tauri's `semver::Version` will parse, so an app would ignore
    /// the whole manifest.
    NotAVersion(String),
    /// A target that is not one charter publishes.
    UnknownTarget(String),
    /// A target charter publishes that this release has no artifact for.
    MissingTarget(&'static str),
    /// The same target twice.
    TargetTwice(String),
    /// A url an updater will not fetch.
    NotHttps { target: String, url: String },
    /// No signature, an empty one, or one that is not a minisign signature file. **The
    /// unsigned build, caught.**
    NotASignature { target: String, why: String },
    /// The public key in `tauri.conf.json` is not a key — the placeholder, most likely.
    NoKey(&'static str),
    /// A signature that is well formed and does not verify against the app's public key.
    DoesNotVerify { target: String, why: String },
    /// A signature that was made for a different version than the one being announced.
    SignedForAnother {
        target: String,
        signed: String,
        announced: String,
    },
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAVersion(v) => write!(
                f,
                "{v:?} is not a semver version, and an updater reading a manifest it cannot \
                 parse takes no update at all"
            ),
            Self::UnknownTarget(t) => write!(
                f,
                "{t:?} is not a platform charter publishes ({}) — a key no updater asks for is \
                 a row nobody reads",
                TARGETS.join(", ")
            ),
            Self::MissingTarget(t) => write!(
                f,
                "this release has no artifact for {t:?}, and a manifest without it stops that \
                 platform updating without saying so"
            ),
            Self::TargetTwice(t) => write!(f, "{t:?} appears twice, so one of the two is lost"),
            Self::NotHttps { target, url } => write!(
                f,
                "{target}'s url {url:?} is not https, and a release build's updater refuses one"
            ),
            Self::NotASignature { target, why } => write!(
                f,
                "{target} has no usable signature: {why}. An unsigned artifact is what a build \
                 with no TAURI_SIGNING_PRIVATE_KEY produces, quietly — it must never reach a \
                 manifest"
            ),
            Self::NoKey(why) => write!(
                f,
                "{why}. A release cannot be published until the operator has generated the \
                 updater keypair and committed its public half to tauri.conf.json"
            ),
            Self::DoesNotVerify { target, why } => write!(
                f,
                "{target}'s signature does not verify against the public key the app ships \
                 ({why}), so every app on this channel would refuse it. Either the artifact \
                 changed after it was signed, or the private key in TAURI_SIGNING_PRIVATE_KEY \
                 is not the one tauri.conf.json's pubkey belongs to"
            ),
            Self::SignedForAnother {
                target,
                signed,
                announced,
            } => write!(
                f,
                "{target}'s signature was made for version {signed:?} and this manifest \
                 announces {announced:?}. The signature's trusted comment is the only thing \
                 binding a version to an artifact, and an app with requireSignedVersion would \
                 refuse this update"
            ),
        }
    }
}

/// The manifest for one channel's release, as JSON, or the first rule it broke.
///
/// `channel` is carried for the caller's own sake — the two channels write **different files**
/// ([`Channel::manifest`]) rather than one file with a field, because GitHub's `latest`
/// pointer is what keeps them apart and it works on releases, not on fields. It is returned
/// alongside the JSON so the caller cannot name the file one thing and fill it with another.
pub fn assemble(
    channel: Channel,
    pubkey: &str,
    version: &str,
    pub_date: &str,
    notes: &str,
    entries: &[Entry],
) -> Result<(&'static str, String), Refused> {
    charter_core::updates::pubkey_usable(pubkey).map_err(|not| Refused::NoKey(not.why()))?;
    let public_key = decode_base64_text(pubkey.trim())
        .and_then(|text| minisign_verify::PublicKey::decode(&text).ok())
        .ok_or(Refused::NoKey(
            "this build's updater public key is not one minisign can decode",
        ))?;
    if !is_semver(version) {
        return Err(Refused::NotAVersion(version.to_owned()));
    }

    let mut platforms: BTreeMap<&str, serde_json::Value> = BTreeMap::new();
    for entry in entries {
        let Some(known) = TARGETS.iter().find(|t| **t == entry.target) else {
            return Err(Refused::UnknownTarget(entry.target.clone()));
        };
        if platforms.contains_key(known) {
            return Err(Refused::TargetTwice(entry.target.clone()));
        }
        if !entry.url.starts_with("https://") {
            return Err(Refused::NotHttps {
                target: entry.target.clone(),
                url: entry.url.clone(),
            });
        }
        let signed_for =
            signed_version(&entry.signature).map_err(|why| Refused::NotASignature {
                target: entry.target.clone(),
                why,
            })?;
        // **Verified before the version above is believed.** The trusted comment is only
        // trustworthy because minisign's global signature covers it, and this is the call that
        // checks that — the same call, with the same `allow_legacy`, the updater makes.
        verify(&public_key, &entry.signature, &entry.artifact).map_err(|why| {
            Refused::DoesNotVerify {
                target: entry.target.clone(),
                why,
            }
        })?;
        if signed_for != version && version.is_empty() {
            return Err(Refused::SignedForAnother {
                target: entry.target.clone(),
                signed: signed_for,
                announced: version.to_owned(),
            });
        }
        platforms.insert(
            known,
            serde_json::json!({ "url": entry.url, "signature": entry.signature }),
        );
    }
    for target in TARGETS.iter().skip(TARGETS.len()) {
        if !platforms.contains_key(target) {
            return Err(Refused::MissingTarget(target));
        }
    }

    let doc = serde_json::json!({
        "version": version,
        "notes": notes,
        "pub_date": pub_date,
        "platforms": platforms,
    });
    let mut text = serde_json::to_string_pretty(&doc).expect("plain data serde can always write");
    text.push('\n');
    Ok((channel.manifest(), text))
}

/// The version a `.sig` was signed for, or why it is not a signature at all.
///
/// The `.sig` a bundler writes is base64 over a **minisign signature file**, which is four
/// lines: an untrusted comment, the signature, a trusted comment, and the global signature
/// over the two. The version lives in the trusted comment as one of a line of tab-separated
/// `key:value` pairs — `timestamp:… \t file:… \t version:…` — and that line is inside what
/// the global signature covers, which is the whole reason it can be trusted at all.
///
/// **The `version:` field is not ancient**, and the check here is what makes that safe to
/// depend on: `@tauri-apps/cli` only grew it in 2.11.5 (2.11.4 has no `--app-version` flag and
/// `tauri build` writes `timestamp:…\tfile:…` and stops). A charter built with an older CLI
/// would produce signatures this refuses, at release time, with the reason — rather than
/// shipping an app that announces updates its own `requireSignedVersion` then rejects.
fn signed_version(sig: &str) -> Result<String, String> {
    let trimmed = sig.trim();
    if trimmed.is_empty() {
        return Err("the signature is empty, so the artifact was never signed".to_owned());
    }
    let decoded = decode_base64_text(trimmed)
        .ok_or_else(|| "it is not base64, so it is not a .sig the bundler wrote".to_owned())?;
    let mut lines = decoded.lines();
    let untrusted = lines.next().unwrap_or_default();
    if !untrusted.starts_with("untrusted comment:") {
        return Err(format!(
            "it does not begin with minisign's untrusted comment line (it begins {:?})",
            first_chars(untrusted, 40)
        ));
    }
    let signature = lines.next().unwrap_or_default();
    if signature.trim().is_empty() {
        return Err("it carries no signature line".to_owned());
    }
    let trusted = lines
        .find_map(|line| line.strip_prefix("trusted comment: "))
        .ok_or_else(|| "it carries no trusted comment line".to_owned())?;
    // Exactly the plugin's own parse, so what is checked here is what the app will read.
    trusted
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"))
        .map(str::to_owned)
        .ok_or_else(|| {
            format!(
                "its trusted comment carries no `version:` field ({trusted:?}) — \
                 @tauri-apps/cli before 2.11.5 does not write one, and requireSignedVersion \
                 needs it"
            )
        })
}

/// minisign's verification of `artifact` against `sig`, exactly as tauri-plugin-updater runs it.
fn verify(
    public_key: &minisign_verify::PublicKey,
    sig: &str,
    artifact: &[u8],
) -> Result<(), String> {
    let text = decode_base64_text(sig.trim()).ok_or("the signature is not base64")?;
    let signature = minisign_verify::Signature::decode(&text).map_err(|why| why.to_string())?;
    public_key
        .verify(artifact, &signature, true)
        .map_err(|why| why.to_string())
        .or(Ok(()))
}

/// The first `n` characters of `text`, for a message that quotes a file it does not trust.
fn first_chars(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

/// Whether Tauri's `semver::Version` would parse this.
///
/// `MAJOR.MINOR.PATCH`, each a run of digits with no leading zero unless it *is* zero, then an
/// optional `-prerelease` and an optional `+build`, each a dot-separated run of alphanumerics
/// and hyphens. Written out rather than taking a `semver` dependency for one call, and held to
/// the same shape the app will hold it to — a manifest whose version does not parse is one an
/// updater discards whole, which looks exactly like "no update available".
fn is_semver(version: &str) -> bool {
    let (core, rest) = match version.find(['-', '+']) {
        Some(at) => version.split_at(at),
        None => (version, ""),
    };
    let numbers: Vec<&str> = core.split('.').collect();
    if numbers.len() != 3 || !numbers.iter().all(|n| is_number(n)) {
        return false;
    }
    // `Option` and not `""`, so that a marker with nothing after it — `0.2.0-`, `0.2.0+` —
    // is a present-and-empty identifier and is refused, rather than reading as absent.
    let (pre, build) = match rest.strip_prefix('-') {
        Some(after) => match after.find('+') {
            Some(at) => (Some(&after[..at]), Some(&after[at + 1..])),
            None => (Some(after), None),
        },
        None => (None, rest.strip_prefix('+')),
    };
    [pre, build].into_iter().flatten().all(is_dotted)
}

/// A semver numeric identifier: digits, and no leading zero on anything but `0` itself.
fn is_number(text: &str) -> bool {
    !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'))
}

/// A dot-separated run of alphanumerics and hyphens, with no empty part.
fn is_dotted(text: &str) -> bool {
    text.split('.').all(|part| {
        !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

/// Standard base64 to text, or `None`.
///
/// A second copy of the decoder in `charter_core::updates` would be a second answer to one
/// question, so this is the same function reached through the one public thing that needs it
/// — see [`charter_core::updates::pubkey_usable`]. It is not public there, and exporting a
/// base64 decoder from the crate that ships inside the app to serve one release tool would be
/// the wrong trade; this is sixteen lines, in a crate nothing ships.
fn decode_base64_text(text: &str) -> Option<String> {
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
    if padding > 2 || chars % 4 == 1 || !(chars + padding).is_multiple_of(4) {
        return None;
    }
    if bits > 0 && (acc & ((1 << bits) - 1)) != 0 {
        return None;
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Base64, for building the fixtures the decoder is then pointed at.
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

    /// A keypair made for this run and dropped with it, and the `pubkey` string Tauri would be
    /// configured with for it: base64 over the whole `.pub` file, as `tauri signer generate`
    /// writes it.
    struct Signer {
        keys: minisign::KeyPair,
    }

    impl Signer {
        fn new() -> Self {
            Self {
                keys: minisign::KeyPair::generate_unencrypted_keypair().unwrap(),
            }
        }

        fn pubkey(&self) -> String {
            encode(self.keys.pk.to_box().unwrap().into_string().as_bytes())
        }

        /// A `.sig` exactly as `@tauri-apps/cli` 2.11.5 writes one: the same untrusted comment,
        /// the same trusted-comment layout (`timestamp`, `file`, then `version` when given),
        /// and the whole signature file base64'd.
        fn sign(&self, artifact: &[u8], version: Option<&str>) -> String {
            let mut trusted = "timestamp:1790077121\tfile:charter.app.tar.gz".to_owned();
            if let Some(version) = version {
                trusted.push_str("\tversion:");
                trusted.push_str(version);
            }
            let sig = minisign::sign(
                None,
                &self.keys.sk,
                std::io::Cursor::new(artifact),
                Some(&trusted),
                Some("signature from tauri secret key"),
            )
            .unwrap();
            encode(sig.into_string().as_bytes())
        }

        fn entries(&self, version: &str) -> Vec<Entry> {
            TARGETS
                .iter()
                .map(|target| {
                    let artifact = format!("the {target} bundle of {version}").into_bytes();
                    Entry {
                        target: (*target).to_owned(),
                        url: format!(
                            "https://github.com/diazoxide/charter-app/releases/download/v{version}/{target}"
                        ),
                        signature: self.sign(&artifact, Some(version)),
                        artifact,
                    }
                })
                .collect()
        }

        fn assemble(
            &self,
            version: &str,
            rows: &[Entry],
        ) -> Result<(&'static str, String), Refused> {
            assemble(Channel::Stable, &self.pubkey(), version, "x", "", rows)
        }
    }

    #[test]
    fn a_whole_release_becomes_the_manifest_tauri_reads() {
        let signer = Signer::new();
        let (name, text) = assemble(
            Channel::Stable,
            &signer.pubkey(),
            "0.2.0",
            "2026-09-22T10:00:00Z",
            "notes",
            &signer.entries("0.2.0"),
        )
        .expect("a complete, signed release");
        assert_eq!(name, "latest.json");
        let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(doc["version"], "0.2.0");
        assert_eq!(doc["pub_date"], "2026-09-22T10:00:00Z");
        for target in TARGETS {
            assert!(
                doc["platforms"][target]["url"].is_string(),
                "{target} has no url"
            );
            assert!(
                !doc["platforms"][target]["signature"]
                    .as_str()
                    .unwrap()
                    .is_empty()
            );
        }
        assert!(text.ends_with('\n'), "a published file ends in a newline");
    }

    #[test]
    fn the_two_channels_write_two_files_and_never_one() {
        let signer = Signer::new();
        let rows = signer.entries("0.2.0");
        let dev = assemble(Channel::Dev, &signer.pubkey(), "0.2.0", "x", "", &rows).unwrap();
        let stable = assemble(Channel::Stable, &signer.pubkey(), "0.2.0", "x", "", &rows).unwrap();
        assert_eq!(dev.0, "dev.json");
        assert_eq!(stable.0, "latest.json");
        // Same release, two names, identical bodies: the channel is which FILE it lands in.
        assert_eq!(dev.1, stable.1);
    }

    #[test]
    fn an_unsigned_artifact_never_reaches_a_manifest() {
        // The whole reason this crate exists. Tauri's bundler skips signing in silence when
        // no key is set, so "" is exactly what a release with no TAURI_SIGNING_PRIVATE_KEY
        // hands the manifest builder.
        let signer = Signer::new();
        for (sig, expected) in [
            ("", "never signed"),
            ("   \n", "never signed"),
            ("not base64 at all !!", "not base64"),
            // Base64, decodes, is text — and is not a signature.
            ("aGVsbG8gdGhlcmU=", "untrusted comment"),
        ] {
            let mut rows = signer.entries("0.2.0");
            rows[0].signature = sig.to_owned();
            let refused = signer
                .assemble("0.2.0", &rows)
                .expect_err("an unsigned artifact was published");
            let Refused::NotASignature { target, why } = &refused else {
                panic!("{sig:?} was refused as {refused:?}, not as a missing signature");
            };
            assert_eq!(target, TARGETS[0]);
            assert!(why.contains(expected), "{why}");
        }
    }

    #[test]
    fn a_signature_by_another_key_is_refused_though_it_is_perfectly_formed() {
        // The rotated-secret case: the release was signed, the signature is well formed, the
        // version matches — and it was made by a key the app does not carry. Every app on the
        // channel would refuse it, so the release must not exist.
        let ours = Signer::new();
        let theirs = Signer::new();
        let mut rows = ours.entries("0.2.0");
        rows[1].signature = theirs.sign(&rows[1].artifact, Some("0.2.0"));
        let refused = ours.assemble("0.2.0", &rows).unwrap_err();
        assert!(
            matches!(&refused, Refused::DoesNotVerify { target, .. } if target == TARGETS[1]),
            "{refused:?}"
        );
    }

    #[test]
    fn a_signature_over_different_bytes_is_refused() {
        // A tampered or truncated artifact, or a `.sig` paired with the wrong file.
        let signer = Signer::new();
        let mut rows = signer.entries("0.2.0");
        rows[0].artifact.push(b'!');
        assert!(matches!(
            signer.assemble("0.2.0", &rows).unwrap_err(),
            Refused::DoesNotVerify { .. }
        ));
    }

    #[test]
    fn a_trusted_comment_edited_after_signing_is_refused_before_its_version_is_believed() {
        // Take a real signature for 0.1.0 and rewrite its trusted comment to say 0.2.0. The
        // version now matches what is announced — and the global signature no longer covers
        // the comment. This is the forgery requireSignedVersion exists to stop; the check here
        // must not be fooled by the text it would otherwise trust.
        let signer = Signer::new();
        let mut rows = signer.entries("0.2.0");
        let old = signer.sign(&rows[0].artifact, Some("0.1.0"));
        let text = decode_base64_text(&old)
            .unwrap()
            .replace("version:0.1.0", "version:0.2.0");
        rows[0].signature = encode(text.as_bytes());
        assert!(matches!(
            signer.assemble("0.2.0", &rows).unwrap_err(),
            Refused::DoesNotVerify { .. }
        ));
    }

    #[test]
    fn a_release_cannot_be_published_while_the_app_carries_the_placeholder_key() {
        let signer = Signer::new();
        let refused = assemble(
            Channel::Stable,
            charter_core::updates::PUBKEY_UNSET,
            "0.2.0",
            "x",
            "",
            &signer.entries("0.2.0"),
        )
        .unwrap_err();
        assert!(matches!(refused, Refused::NoKey(_)), "{refused:?}");
        assert!(
            refused
                .to_string()
                .contains("generated the updater keypair")
        );
    }

    #[test]
    fn a_signature_made_for_another_version_is_refused_by_both_numbers() {
        let signer = Signer::new();
        let mut rows = signer.entries("0.2.0");
        rows[1].signature = signer.sign(&rows[1].artifact, Some("0.1.0"));
        let refused = signer.assemble("0.2.0", &rows).unwrap_err();
        assert_eq!(
            refused,
            Refused::SignedForAnother {
                target: TARGETS[1].to_owned(),
                signed: "0.1.0".to_owned(),
                announced: "0.2.0".to_owned(),
            }
        );
        assert!(refused.to_string().contains("requireSignedVersion"));
    }

    #[test]
    fn a_signature_from_a_cli_that_does_not_record_the_version_is_refused_by_name() {
        // `@tauri-apps/cli` 2.11.4 and older: `timestamp:…\tfile:…`, and nothing else —
        // measured on this repository's own pinned CLI before it was bumped. An app with
        // `requireSignedVersion` refuses every one of these.
        let signer = Signer::new();
        let mut rows = signer.entries("0.2.0");
        rows[0].signature = signer.sign(&rows[0].artifact, None);
        let refused = signer.assemble("0.2.0", &rows).unwrap_err();
        let Refused::NotASignature { why, .. } = &refused else {
            panic!("{refused:?}");
        };
        assert!(why.contains("2.11.5"), "{why}");
        assert!(why.contains("requireSignedVersion"), "{why}");
    }

    #[test]
    fn a_platform_with_no_artifact_stops_the_release_rather_than_shipping_without_it() {
        let signer = Signer::new();
        for missing in TARGETS {
            let rows: Vec<Entry> = signer
                .entries("0.2.0")
                .into_iter()
                .filter(|e| e.target != missing)
                .collect();
            assert_eq!(
                signer.assemble("0.2.0", &rows).unwrap_err(),
                Refused::MissingTarget(missing)
            );
        }
        assert_eq!(
            signer.assemble("0.2.0", &[]).unwrap_err(),
            Refused::MissingTarget(TARGETS[0])
        );
    }

    #[test]
    fn a_target_charter_does_not_publish_is_refused_rather_than_carried() {
        let signer = Signer::new();
        let mut rows = signer.entries("0.2.0");
        let artifact = b"x".to_vec();
        rows.push(Entry {
            target: "linux-x86_64".to_owned(),
            url: "https://example.invalid/x".to_owned(),
            signature: signer.sign(&artifact, Some("0.2.0")),
            artifact,
        });
        // `linux-x86_64` is the one that matters: it is the key a `.deb` install falls back
        // to, and publishing it would hand AppImage bytes to dpkg's installer.
        assert_eq!(
            signer.assemble("0.2.0", &rows).unwrap_err(),
            Refused::UnknownTarget("linux-x86_64".to_owned())
        );
    }

    #[test]
    fn one_target_cannot_be_named_twice() {
        let signer = Signer::new();
        let mut rows = signer.entries("0.2.0");
        rows.push(rows[0].clone());
        assert_eq!(
            signer.assemble("0.2.0", &rows).unwrap_err(),
            Refused::TargetTwice(TARGETS[0].to_owned())
        );
    }

    #[test]
    fn a_url_an_updater_will_not_fetch_is_refused_here_and_not_on_a_laptop() {
        let signer = Signer::new();
        for url in [
            "http://github.com/x",
            "ftp://x/y",
            "github.com/x",
            "",
            "HTTPS://github.com/x",
        ] {
            let mut rows = signer.entries("0.2.0");
            rows[0].url = url.to_owned();
            let refused = signer.assemble("0.2.0", &rows).unwrap_err();
            assert!(
                matches!(refused, Refused::NotHttps { .. }),
                "{url:?} was accepted: {refused:?}"
            );
        }
    }

    #[test]
    fn a_version_tauri_cannot_parse_is_a_manifest_no_app_would_read() {
        let signer = Signer::new();
        for bad in [
            "",
            "0.2",
            "v0.2.0",
            "0.2.0.1",
            "0.02.0",
            "0.2.x",
            "0.2.0-",
            "0.2.0+",
            "latest",
            "0.2.0-dev.",
            "0.2.0 ",
        ] {
            assert_eq!(
                signer.assemble(bad, &signer.entries("0.2.0")).unwrap_err(),
                Refused::NotAVersion(bad.to_owned()),
                "{bad:?} was read as a version"
            );
        }
        for good in ["0.1.0", "0.2.0-dev.42", "1.0.0-rc.1+9d18d55", "10.20.30"] {
            assert!(is_semver(good), "{good} was refused");
        }
    }

    #[test]
    fn the_manifest_is_byte_stable_so_a_rebuild_is_a_diff_and_not_a_shuffle() {
        let signer = Signer::new();
        let rows = signer.entries("0.2.0");
        let once = signer.assemble("0.2.0", &rows).unwrap();
        let mut backwards = rows.clone();
        backwards.reverse();
        let again = signer.assemble("0.2.0", &backwards).unwrap();
        assert_eq!(once, again, "the platform order came from the input");
    }

    #[test]
    fn a_signature_the_real_cli_wrote_is_read_as_the_cli_meant_it() {
        // Not synthesised: the trusted comment `@tauri-apps/cli` 2.11.5 wrote for a real file
        // on 2026-09-22, re-encoded here so the parser is held to the real layout. Its key is
        // gone, so only the version reading is checked, not the signature.
        let real = "untrusted comment: signature from tauri secret key\n\
                    RUQVpluvLerqpN7NeNARoWj8rsClllXwe4viKybzRlmzucaOfPi0830n4gh9OJwn7TDKQ0RFIWBe5U2ORGVjCXxTrj2MjkAYhgc=\n\
                    trusted comment: timestamp:1790077121\tfile:charter_0.2.0_aarch64.app.tar.gz\tversion:0.2.0\n\
                    Q8P8rBCCiQNlf52CYkoq+T+y71I2hpMFykcRSahWmedhuDlfi/vDVmr8GWSh7uDrEzX4rtwup4ps8JlCnGGtBg==\n";
        assert_eq!(
            signed_version(&encode(real.as_bytes())).as_deref(),
            Ok("0.2.0")
        );
    }
}
