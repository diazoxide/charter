//! Whether the record a launch reads was written by this clone, on this device (V43).
//!
//! **A chat's id is minted once per clone and device** (ADR 0066, amended by V43). A project
//! copied with `cp -R`, rsync or a backup restore carries `.charter/app/reopen.json` with it,
//! and with it every open chat's id: two clones would then hold one chat, and anything keyed on
//! the id (events, session records, ADR 0088's chat links) would merge the two. So the record
//! says which clone and which device wrote it ([`CloneSeat`]), and the launch that reads it decides,
//! before any chat starts, which of four things it is looking at ([`Arrival`]).
//!
//! **Where it looks.** At the old root the record names, and at every project this machine
//! remembers opening (the machine store's recents, D-V43x), because the original may have moved
//! since it was copied: after `cp -R a b; mv a c`, `a` is gone, and only `c` still holds the
//! chats. Whichever of `b` and `c` launches first keeps the ids, and the other mints new ones.
//!
//! **What it cannot tell.** A copy whose original is unreachable (deleted, or on a volume that
//! is not mounted) **and** is not among the projects this machine remembers reads as a move,
//! and keeps its ids. V43 accepts that: on this machine the evidence is gone.
//!
//! **What deciding by the device first costs.** V43 names the device id as what catches a
//! same-path copy to another machine, so another device always means a copy, and new ids are
//! minted when the project moves to another machine, when the machine store is reset (a new
//! device id), and when the same project is opened under a different `CHARTER_CONFIG_HOME`
//! (a dev or isolated build). Each of those is a new run of history under new ids, never a
//! merge of two.

use std::path::{Path, PathBuf};

use super::Record;
use crate::plane::CloneKey;

/// The clone and the device that wrote a record: the record's `clone` key on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneSeat {
    /// The clone-key (ADR 0079 §8) of [`Self::root`].
    pub key: CloneKey,
    /// The clone's canonical root. Kept beside the key because a key cannot be followed back
    /// to a directory, and telling a copy from a move means looking at the old one.
    pub root: PathBuf,
    /// The device id of the machine that wrote the record (ADR 0066), or `None` where its
    /// machine store had none (ADR 0031).
    pub device: Option<String>,
}

impl CloneSeat {
    /// The seat of the clone at `root`, written from `device`. `root` is canonicalised once,
    /// here, and the key is taken from that spelling.
    pub fn of(root: &Path, device: Option<String>) -> Self {
        let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        Self {
            key: CloneKey::of_canonical(&root),
            root,
            device,
        }
    }
}

/// What a launch found about where its record came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    /// The clone and the device that wrote it, perhaps under another spelling of the same
    /// directory (a firmlink, a bind mount, a hand-edited root): the ids are kept.
    Same,
    /// A record written before V43, with no clone-key. It adopts the clone it is opened in and
    /// keeps its ids: re-minting every operator's open chats at the upgrade would cost more
    /// than the copies it might catch.
    Legacy,
    /// Another clone-key, and no other clone this machine can reach holds these chats: the
    /// project moved. The ids are kept, and the record names the clone it is in now.
    Moved,
    /// Another clone still holds these chats, or another device wrote the record: the project
    /// was copied. Every chat gets a new id, minted on this device.
    Copied,
}

/// The record `record` as this launch, in the clone and on the device `here` names, puts it
/// back, and which [`Arrival`] it was. `known` answers every project root this machine
/// remembers opening, and is asked only when the clone-key differs and the old root holds none
/// of these chats. Writes nothing: the caller writes what this returns once its chats are back.
///
/// **Another device decides first, and nothing is read for it.** A same-path copy to another
/// machine has the same clone-key, so only the device id catches it. A device that is unknown
/// on either side decides nothing, and the clone-key does.
///
/// **On another clone-key, the record is compared with other clones' records**: the old root
/// first, then every root in `known`. A root that is the same directory as this one is never
/// another clone. **A chat one of them shares is enough**: the original may have opened or
/// closed others since the copy was made, and one shared id is already one chat in two clones.
pub fn arrive(
    record: Record,
    here: &CloneSeat,
    known: impl FnOnce() -> Vec<PathBuf>,
) -> (Arrival, Record) {
    let Some(was) = record.clone_seat.clone() else {
        return (Arrival::Legacy, seated(record, here));
    };
    let another_device = matches!(
        (&was.device, &here.device),
        (Some(was), Some(here)) if was != here
    );
    if another_device {
        return (Arrival::Copied, reminted(record, here));
    }
    if was.key == here.key {
        return (Arrival::Same, record);
    }
    if same_directory(&was.root, &here.root) {
        return (Arrival::Same, seated(record, here));
    }
    let another = |root: &Path| !same_directory(root, &here.root) && holds_any_of(root, &record);
    let another_holds_them = another(&was.root) || known().iter().any(|root| another(root));
    if another_holds_them {
        return (Arrival::Copied, reminted(record, here));
    }
    (Arrival::Moved, seated(record, here))
}

/// `record`, saying it was written from `here`.
fn seated(record: Record, here: &CloneSeat) -> Record {
    Record {
        clone_seat: Some(here.clone()),
        ..record
    }
}

/// Whether `one` and `other` are one directory: the same device and inode, whatever path
/// reached them. A path that does not resolve is no directory at all.
fn same_directory(one: &Path, other: &Path) -> bool {
    let (Ok(one), Ok(other)) = (std::fs::metadata(one), std::fs::metadata(other)) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        one.dev() == other.dev() && one.ino() == other.ino()
    }
    #[cfg(not(unix))]
    {
        let _ = (one, other);
        false
    }
}

/// Whether the record at `root` names any chat `record` does. Read through the guarded
/// [`super::read_or_refusal`] (no link, a plain file, bounded), and only its ids are compared:
/// nothing it names is run. A record there that is refused or unreadable holds none, because a
/// refused record is not evidence of a copy.
fn holds_any_of(root: &Path, record: &Record) -> bool {
    // A root that is gone holds nothing, and is not read.
    if !root.is_dir() {
        return false;
    }
    let Ok(there) = super::read_or_refusal(root) else {
        return false;
    };
    let ids = |r: &Record| -> Vec<String> {
        r.chats
            .iter()
            .filter_map(|chat| chat.identity.id.clone())
            .collect()
    };
    let theirs = ids(&there);
    ids(record).iter().any(|id| theirs.contains(id))
}

/// `record` with every chat given a new id on `here`'s device. The run is cleared too, as the
/// original's: the start that puts the chat back begins one of the copy's own. Everything else
/// a chat is (its number, name, conversation and `resumed_from`) is the copy's as much as the
/// original's, and is kept.
fn reminted(record: Record, here: &CloneSeat) -> Record {
    let chats = record
        .chats
        .into_iter()
        .map(|chat| super::Chat {
            identity: super::Identity {
                id: Some(super::mint()),
                device: here.device.clone(),
                run: None,
                ..chat.identity
            },
            ..chat
        })
        .collect();
    seated(Record { chats, ..record }, here)
}

/// [`CloneSeat`] as `reopen.json` keeps it: `{"key", "root", "device"}`.
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct CloneSeatOnDisk {
    #[serde(default)]
    key: String,
    #[serde(default)]
    root: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    device: String,
}

impl CloneSeatOnDisk {
    /// The seat as it is written, or nothing where its root is not UTF-8. JSON holds a string,
    /// and a lossy spelling of the root would name another directory, so such a clone writes no
    /// seat and every launch of it reads as a record from before V43: its ids are kept.
    pub(super) fn of(seat: &CloneSeat) -> Option<Self> {
        Some(Self {
            key: seat.key.to_string(),
            root: seat.root.to_str()?.to_owned(),
            device: seat.device.clone().unwrap_or_default(),
        })
    }

    /// Held to what charter writes: a [`CloneKey`] and an absolute root. Anything else reads as
    /// no seat, which is a record that adopts the clone it is opened in.
    pub(super) fn held(self) -> Option<CloneSeat> {
        let key = CloneKey::parse(&self.key)?;
        if !Path::new(&self.root).is_absolute() {
            return None;
        }
        Some(CloneSeat {
            key,
            root: PathBuf::from(self.root),
            device: super::a_ulid(&self.device),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{CHAT, DEVICE, RUN};
    use super::super::*;

    const OTHER_DEVICE: &str = "01K6E8ZK6V4Q9T0N3M2B1C5D7K";

    /// A project at `root` whose record holds one chat with ids, written by that clone on
    /// `device`, as an app that knows V43 leaves it.
    fn a_project_with_a_chat(root: &Path, device: Option<&str>) {
        std::fs::create_dir_all(root).unwrap();
        let record = Record {
            chats: vec![Chat {
                program: "/bin/cat".to_owned(),
                number: Some(1),
                identity: Identity {
                    id: Some(CHAT.to_owned()),
                    device: device.map(str::to_owned),
                    run: Some(RUN.to_owned()),
                    resumed_from: None,
                },
                ..Default::default()
            }],
            clone_seat: Some(CloneSeat::of(root, device.map(str::to_owned))),
            ..Default::default()
        };
        write(root, &record).expect("the record is written");
    }

    /// `from` copied whole to `to`, as `cp -R` leaves it.
    fn copied(from: &Path, to: &Path) {
        let file = path(to);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::copy(path(from), file).unwrap();
    }

    /// Arrives at `root` on `device`, writes what the launch would, and reads it back.
    fn launched(root: &Path, device: Option<&str>) -> (Arrival, Record) {
        launched_knowing(root, device, &[])
    }

    /// [`launched`], on a machine that remembers opening `known`.
    fn launched_knowing(root: &Path, device: Option<&str>, known: &[&Path]) -> (Arrival, Record) {
        let here = CloneSeat::of(root, device.map(str::to_owned));
        let known: Vec<PathBuf> = known.iter().map(|root| root.to_path_buf()).collect();
        let (arrival, record) = arrive(read(root), &here, || known);
        write(root, &record).expect("the record is written");
        (arrival, read(root))
    }

    #[test]
    fn a_copy_whose_original_still_holds_its_chats_gets_new_ids_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let (original, copy) = (dir.path().join("a"), dir.path().join("b"));
        a_project_with_a_chat(&original, Some(DEVICE));
        copied(&original, &copy);

        let (arrival, on_disk) = launched(&copy, Some(DEVICE));

        assert_eq!(arrival, Arrival::Copied);
        let identity = &on_disk.chats[0].identity;
        let id = identity.id.as_deref().expect("an id");
        assert_ne!(id, CHAT, "the copy's chat has an id of its own");
        assert_eq!(a_ulid(id).as_deref(), Some(id));
        assert_eq!(identity.device.as_deref(), Some(DEVICE));
        assert_eq!(identity.run, None, "the original's run is not the copy's");
        assert_eq!(
            on_disk.clone_seat,
            Some(CloneSeat::of(&copy, Some(DEVICE.into())))
        );
        assert_eq!(
            read(&original).chats[0].identity.id.as_deref(),
            Some(CHAT),
            "the original is left as it was"
        );
    }

    #[test]
    fn a_moved_project_keeps_its_ids_and_records_the_clone_it_is_in_now() {
        let dir = tempfile::tempdir().unwrap();
        let (before, after) = (dir.path().join("a"), dir.path().join("b"));
        a_project_with_a_chat(&before, Some(DEVICE));
        std::fs::rename(&before, &after).unwrap();

        let (arrival, on_disk) = launched(&after, Some(DEVICE));

        assert_eq!(arrival, Arrival::Moved);
        let identity = &on_disk.chats[0].identity;
        assert_eq!(identity.id.as_deref(), Some(CHAT));
        assert_eq!(identity.device.as_deref(), Some(DEVICE));
        assert_eq!(identity.run.as_deref(), Some(RUN));
        assert_eq!(
            on_disk.clone_seat,
            Some(CloneSeat::of(&after, Some(DEVICE.into())))
        );
        let text = std::fs::read_to_string(path(&after)).unwrap();
        assert!(
            text.contains(crate::plane::CloneKey::of(&after).as_str()),
            "the clone it is in now is what is written: {text}"
        );
    }

    #[test]
    fn a_copy_to_the_same_path_on_another_device_gets_new_ids_from_that_device() {
        // Another machine is simulated by another device id at the same root: the path, and
        // so the clone-key, cannot tell the two apart, and the device id is what does.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("a");
        a_project_with_a_chat(&root, Some(DEVICE));

        let (arrival, on_disk) = launched(&root, Some(OTHER_DEVICE));

        assert_eq!(arrival, Arrival::Copied);
        let identity = &on_disk.chats[0].identity;
        assert!(identity.id.is_some() && identity.id.as_deref() != Some(CHAT));
        assert_eq!(identity.device.as_deref(), Some(OTHER_DEVICE));
        assert_eq!(
            on_disk.clone_seat,
            Some(CloneSeat::of(&root, Some(OTHER_DEVICE.into())))
        );
    }

    #[test]
    fn a_record_from_before_v43_adopts_the_clone_it_is_in_and_keeps_its_ids() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("a");
        a_project_with_a_chat(&root, Some(DEVICE));
        let text = std::fs::read_to_string(path(&root)).unwrap();
        let mut legacy: serde_json::Value = serde_json::from_str(&text).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("clone")
            .expect("V43 wrote one");
        std::fs::write(path(&root), legacy.to_string()).unwrap();
        assert_eq!(read(&root).clone_seat, None, "a record with no clone-key");

        let (arrival, on_disk) = launched(&root, Some(OTHER_DEVICE));

        assert_eq!(arrival, Arrival::Legacy);
        let identity = &on_disk.chats[0].identity;
        assert_eq!(
            identity.id.as_deref(),
            Some(CHAT),
            "nothing is minted again"
        );
        assert_eq!(identity.device.as_deref(), Some(DEVICE));
        assert_eq!(identity.run.as_deref(), Some(RUN));
        assert_eq!(
            on_disk.clone_seat,
            Some(CloneSeat::of(&root, Some(OTHER_DEVICE.into())))
        );
    }

    #[test]
    fn a_copy_whose_original_is_gone_and_unregistered_keeps_the_ids_as_a_move_would() {
        // The residual case V43 accepts: with the original deleted or unmounted, and no
        // project this machine remembers holding the chats, nothing on this machine can tell
        // the copy from a move. Records the original wrote before it went still name them.
        let dir = tempfile::tempdir().unwrap();
        let (original, copy) = (dir.path().join("a"), dir.path().join("b"));
        a_project_with_a_chat(&original, Some(DEVICE));
        copied(&original, &copy);
        std::fs::remove_dir_all(&original).unwrap();

        let (arrival, on_disk) = launched(&copy, Some(DEVICE));

        assert_eq!(arrival, Arrival::Moved);
        assert_eq!(on_disk.chats[0].identity.id.as_deref(), Some(CHAT));
        assert_eq!(
            on_disk.clone_seat,
            Some(CloneSeat::of(&copy, Some(DEVICE.into())))
        );
    }

    #[test]
    fn a_project_moved_off_a_path_another_project_now_holds_keeps_its_ids() {
        let dir = tempfile::tempdir().unwrap();
        let (before, after) = (dir.path().join("a"), dir.path().join("b"));
        a_project_with_a_chat(&before, Some(DEVICE));
        std::fs::rename(&before, &after).unwrap();
        std::fs::create_dir_all(&before).unwrap();
        write(&before, &Record::default()).expect("a new project where the old one was");

        let (arrival, on_disk) = launched(&after, Some(DEVICE));

        assert_eq!(arrival, Arrival::Moved);
        assert_eq!(on_disk.chats[0].identity.id.as_deref(), Some(CHAT));
    }

    #[test]
    fn a_launch_in_the_clone_and_on_the_device_that_wrote_the_record_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("a");
        a_project_with_a_chat(&root, Some(DEVICE));
        let before = read(&root);

        let (arrival, on_disk) = launched(&root, Some(DEVICE));

        assert_eq!(arrival, Arrival::Same);
        assert_eq!(on_disk, before);
    }

    #[test]
    fn a_device_that_is_not_known_on_either_side_is_left_to_the_path_to_decide() {
        // A machine store that refuses (ADR 0031) has no device id: the clone-key still
        // catches a copy, and a same-path launch cannot be told from the one that wrote it.
        let dir = tempfile::tempdir().unwrap();
        let (original, copy) = (dir.path().join("a"), dir.path().join("b"));
        a_project_with_a_chat(&original, None);
        copied(&original, &copy);

        assert_eq!(launched(&original, Some(DEVICE)).0, Arrival::Same);
        assert_eq!(launched(&copy, None).0, Arrival::Copied);
    }

    #[test]
    fn a_mark_that_is_not_one_charter_writes_reads_as_none() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("a");
        a_project_with_a_chat(&root, Some(DEVICE));
        let text = std::fs::read_to_string(path(&root)).unwrap();
        let key = crate::plane::CloneKey::of(&root).to_string();

        for (from, to) in [(key.as_str(), "-rf"), ("\"root\": \"/", "\"root\": \"")] {
            std::fs::write(path(&root), text.replacen(from, to, 1)).unwrap();
            assert_eq!(read(&root).clone_seat, None, "{from} -> {to}");
        }
    }

    /// `cp -R a b; mv a c`, with `a` launched before it on this machine, then `first` and
    /// `second` launched in that order: what each one's record holds after its launch.
    fn copied_then_moved(first: &str, second: &str) -> (Record, Record) {
        let dir = tempfile::tempdir().unwrap();
        let at = |name: &str| dir.path().join(name);
        a_project_with_a_chat(&at("a"), Some(DEVICE));
        copied(&at("a"), &at("b"));
        std::fs::rename(at("a"), at("c")).unwrap();

        let (one, _) = launched_knowing(&at(first), Some(DEVICE), &[&at("a")]);
        let (two, _) = launched_knowing(&at(second), Some(DEVICE), &[&at(first), &at("a")]);

        assert_eq!(
            (one, two),
            (Arrival::Moved, Arrival::Copied),
            "{first} then {second}"
        );
        (read(&at(first)), read(&at(second)))
    }

    #[test]
    fn a_copy_whose_original_moved_since_is_still_a_copy_whichever_launches_first() {
        for (first, second) in [("c", "b"), ("b", "c")] {
            let (kept, minted) = copied_then_moved(first, second);
            assert_eq!(
                kept.chats[0].identity.id.as_deref(),
                Some(CHAT),
                "{first} keeps it"
            );
            let id = minted.chats[0].identity.id.clone();
            assert!(
                id.is_some() && id.as_deref() != Some(CHAT),
                "{second}: {id:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_same_directory_under_another_path_is_the_same_clone_and_keeps_its_ids() {
        // A firmlink, a bind mount or a hand-edited `clone.root` names this directory by
        // another path, so the key differs, and the old root is this clone itself: reading its
        // record finds these very chats. That is never a copy.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("a");
        a_project_with_a_chat(&root, Some(DEVICE));
        let link = dir.path().join("another-name-for-a");
        std::os::unix::fs::symlink(&root, &link).unwrap();
        let text = std::fs::read_to_string(path(&root)).unwrap();
        let canonical = std::fs::canonicalize(&root).unwrap();
        let edited = text
            .replacen(
                &format!("\"root\": \"{}\"", canonical.display()),
                &format!("\"root\": \"{}\"", link.display()),
                1,
            )
            .replacen(
                crate::plane::CloneKey::of(&root).as_str(),
                crate::plane::CloneKey::of_canonical(&link).as_str(),
                1,
            );
        assert_ne!(edited, text, "the seat was edited");
        std::fs::write(path(&root), edited).unwrap();

        for _ in 0..2 {
            let (arrival, on_disk) = launched_knowing(&root, Some(DEVICE), &[&link, &root]);
            assert_eq!(arrival, Arrival::Same);
            assert_eq!(on_disk.chats[0].identity.id.as_deref(), Some(CHAT));
            assert_eq!(
                on_disk.clone_seat,
                Some(CloneSeat::of(&root, Some(DEVICE.into())))
            );
        }
    }

    #[test]
    fn a_copy_s_chats_keep_everything_but_the_ids_they_shared() {
        let dir = tempfile::tempdir().unwrap();
        let (original, copy) = (dir.path().join("a"), dir.path().join("b"));
        std::fs::create_dir_all(&original).unwrap();
        let chat = Chat {
            program: "claude".to_owned(),
            number: Some(7),
            name: "ide.7".to_owned(),
            resume: Some(SessionId::new("0b1c2d3e-4f5a-6b7c-8d9e-0f1a2b3c4d5e").unwrap()),
            label: Some("triage".to_owned()),
            identity: Identity {
                id: Some(CHAT.to_owned()),
                device: Some(DEVICE.to_owned()),
                run: Some(RUN.to_owned()),
                resumed_from: Some(super::super::tests::RESUMED.to_owned()),
            },
            ..Default::default()
        };
        let record = Record {
            chats: vec![chat.clone()],
            dealt: 9,
            clone_seat: Some(CloneSeat::of(&original, Some(DEVICE.into()))),
            ..Default::default()
        };
        write(&original, &record).unwrap();
        copied(&original, &copy);

        let (arrival, on_disk) = launched(&copy, Some(DEVICE));

        assert_eq!(arrival, Arrival::Copied);
        let back = &on_disk.chats[0];
        assert_eq!(back.number, Some(7));
        assert_eq!(back.name, "ide.7");
        assert_eq!(back.label.as_deref(), Some("triage"));
        assert_eq!(back.resume, chat.resume);
        assert_eq!(
            back.identity.resumed_from.as_deref(),
            Some(super::super::tests::RESUMED)
        );
        assert_eq!(on_disk.dealt, 9);
    }
}
