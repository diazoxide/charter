//! Whether the record a launch reads was written by this clone, on this device (V43).
//!
//! **A chat's id is minted once per clone and device** (ADR 0066, amended by V43). A project
//! copied with `cp -R`, rsync or a backup restore carries `.charter/app/reopen.json` with it,
//! and with it every open chat's id: two clones would then hold one chat, and anything keyed on
//! the id (events, session records, ADR 0088's chat links) would merge the two. So the record
//! says which clone and which device wrote it ([`CloneMark`]), and the launch that reads it
//! decides, before any chat starts, which of four things it is looking at ([`Arrival`]).
//!
//! **What it cannot tell.** A copy whose original has been deleted, or sits on a volume that is
//! not mounted, reads as a move and keeps its ids. No live clone holds them then, so nothing
//! merges from here on; records the original wrote before it went still name them. V43 accepts
//! that, and no other answer is available on one machine: the evidence is gone.

use std::path::{Path, PathBuf};

use super::Record;

/// The clone and the device that wrote a record: the record's `clone` key on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneMark {
    /// The clone-key (ADR 0079 §8): [`crate::plane::clone_key`] of [`Self::root`].
    pub key: String,
    /// The clone's canonical root. Kept beside the key because a key cannot be followed back
    /// to a directory, and telling a copy from a move means looking at the old one.
    pub root: PathBuf,
    /// The device id of the machine that wrote the record (ADR 0066), or `None` where its
    /// machine store had none (ADR 0031).
    pub device: Option<String>,
}

impl CloneMark {
    /// The mark of the clone at `root`, written from `device`.
    pub fn of(root: &Path, device: Option<String>) -> Self {
        let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        Self {
            key: crate::plane::clone_key(&root),
            root,
            device,
        }
    }
}

/// What a launch found about where its record came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    /// The clone and the device that wrote it: nothing changes.
    Same,
    /// A record written before V43, with no clone-key. It adopts the clone it is opened in and
    /// keeps its ids: re-minting every operator's open chats at the upgrade would cost more
    /// than the copies it might catch.
    Legacy,
    /// Another clone-key, and the old root no longer holds these chats: the project moved. The
    /// ids are kept, and the record names the clone it is in now.
    Moved,
    /// The old root still holds these chats, or another device wrote the record: the project
    /// was copied. Every chat gets a new id, minted on this device.
    Copied,
}

/// The record `record` as this launch, in the clone and on the device `here` names, puts it
/// back: [`Arrival`] says which case it was. Reads the old root's record when the clone-key
/// differs, and writes nothing — the caller writes what this returns once its chats are back.
///
/// **Another device decides first.** A same-path copy to another machine has the same
/// clone-key, so only the device id catches it. A device that is unknown on either side decides
/// nothing, and the path does. **A chat the copy shares with its original is enough**: the
/// original may have opened or closed others since the copy was made, and one shared id is
/// already one chat in two clones.
pub fn arrive(record: Record, here: &CloneMark) -> (Arrival, Record) {
    let Some(was) = record.clone.clone() else {
        let adopted = Record {
            clone: Some(here.clone()),
            ..record
        };
        return (Arrival::Legacy, adopted);
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
    if still_holds_these_chats(&was.root, &record) {
        return (Arrival::Copied, reminted(record, here));
    }
    let moved = Record {
        clone: Some(here.clone()),
        ..record
    };
    (Arrival::Moved, moved)
}

/// Whether the record at `root` names any chat `record` does. A record there that is refused or
/// unreadable holds none: a refused record is not evidence of a copy.
fn still_holds_these_chats(root: &Path, record: &Record) -> bool {
    let Ok(there) = super::read_or_refusal(root) else {
        return false;
    };
    record
        .chats
        .iter()
        .filter_map(|chat| chat.identity.id.as_deref())
        .any(|id| {
            there
                .chats
                .iter()
                .any(|chat| chat.identity.id.as_deref() == Some(id))
        })
}

/// `record` with every chat given a new id on `here`'s device. The run is cleared too, as the
/// original's: the start that puts the chat back begins one of the copy's own.
fn reminted(record: Record, here: &CloneMark) -> Record {
    Record {
        chats: record
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
            .collect(),
        clone: Some(here.clone()),
        ..record
    }
}

/// [`CloneMark`] as `reopen.json` keeps it: `{"key", "root", "device"}`.
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct CloneOnDisk {
    #[serde(default)]
    key: String,
    #[serde(default)]
    root: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    device: String,
}

impl From<&CloneMark> for CloneOnDisk {
    fn from(mark: &CloneMark) -> Self {
        Self {
            key: mark.key.clone(),
            root: mark.root.display().to_string(),
            device: mark.device.clone().unwrap_or_default(),
        }
    }
}

impl CloneOnDisk {
    /// Held to what charter writes: a key of 16 lowercase hex characters and an absolute root.
    /// Anything else reads as no mark, which is a record that adopts the clone it is opened in.
    pub(super) fn held(self) -> Option<CloneMark> {
        let key_ok = self.key.len() == 16
            && self
                .key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !key_ok || !Path::new(&self.root).is_absolute() {
            return None;
        }
        Some(CloneMark {
            key: self.key,
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
            clone: Some(CloneMark::of(root, device.map(str::to_owned))),
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
        let here = CloneMark::of(root, device.map(str::to_owned));
        let (arrival, record) = arrive(read(root), &here);
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
            on_disk.clone,
            Some(CloneMark::of(&copy, Some(DEVICE.into())))
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
            on_disk.clone,
            Some(CloneMark::of(&after, Some(DEVICE.into())))
        );
        let text = std::fs::read_to_string(path(&after)).unwrap();
        assert!(
            text.contains(&crate::plane::clone_key(&after)),
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
            on_disk.clone,
            Some(CloneMark::of(&root, Some(OTHER_DEVICE.into())))
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
        assert_eq!(read(&root).clone, None, "a record with no clone-key");

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
            on_disk.clone,
            Some(CloneMark::of(&root, Some(OTHER_DEVICE.into())))
        );
    }

    #[test]
    fn a_copy_whose_original_is_gone_keeps_the_ids_as_a_move_would() {
        // The residual case V43 accepts: with the original deleted or unmounted, nothing on
        // this machine can tell the copy from a move, and the ids it keeps collide with no
        // live clone's. Records the original wrote before it went still name them.
        let dir = tempfile::tempdir().unwrap();
        let (original, copy) = (dir.path().join("a"), dir.path().join("b"));
        a_project_with_a_chat(&original, Some(DEVICE));
        copied(&original, &copy);
        std::fs::remove_dir_all(&original).unwrap();

        let (arrival, on_disk) = launched(&copy, Some(DEVICE));

        assert_eq!(arrival, Arrival::Moved);
        assert_eq!(on_disk.chats[0].identity.id.as_deref(), Some(CHAT));
        assert_eq!(
            on_disk.clone,
            Some(CloneMark::of(&copy, Some(DEVICE.into())))
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
        let key = crate::plane::clone_key(&root);

        for (from, to) in [(key.as_str(), "-rf"), ("\"root\": \"/", "\"root\": \"")] {
            std::fs::write(path(&root), text.replacen(from, to, 1)).unwrap();
            assert_eq!(read(&root).clone, None, "{from} -> {to}");
        }
    }
}
