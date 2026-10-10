//! Files into chats (FM-9, #1103 F7): a file, a folder or a range of lines of a branch, typed
//! into a chat as a reference in its harness's own syntax, and never sent.
//!
//! **The window names a branch and a path; the core builds the text.** Every command here
//! takes the branch by its names and the path inside it, places it with
//! `purlis_core::reference::of` (confined by `files::place`), and has the chat's harness
//! adapter render it. Nothing the window sends is typed as it was sent.
//!
//! **Typed the way a curation prompt is** (ADR 0061): one bracketed paste with nothing after
//! it ([`purlis_core::reference::pasted`]). Into a running chat only at its prompt
//! ([`purlis_core::reference::may_type_into`]: never mid-turn, never while it asks the
//! operator something); as the first prompt of a chat "Start a chat here" opens, through
//! `curation::start_typed_where_it_can_be`, held until its harness is ready exactly as a
//! curation prompt is.
//!
//! **Where it cannot be typed, it is copied.** opencode (ADR 0061 measured why), a program
//! charter has no adapter for, and a chat that is not at its prompt now: the reference goes on
//! the clipboard, and the answer says why, so the operator can paste it themselves.

use std::path::Path;
use std::sync::Arc;

use purlis_core::engine::Size;
use purlis_core::harness::Harness;
use purlis_core::reference::{self, Lines, Now, Reference};

use crate::curation::{self, ChatTyped};
use crate::planes::{Held, PlaneId, Planes};

/// A range of lines, as the window names one: both ends counted from 1 and included.
#[derive(Debug, Clone, Copy, serde::Deserialize, specta::Type)]
pub struct LineRange {
    pub first: u32,
    pub last: u32,
}

impl From<LineRange> for Lines {
    fn from(range: LineRange) -> Self {
        Lines {
            first: range.first,
            last: range.last,
        }
    }
}

/// What became of a reference handed to a chat.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Handed {
    /// Typed into the chat, unsent: `text` is what was typed.
    Typed { text: String },
    /// Put on the clipboard instead, with the sentence saying why.
    Copied { text: String, why: String },
}

/// One file, folder or range of lines of a branch, typed into chat `session` as a reference in
/// its harness's syntax, and never sent — or copied, with the reason, where it cannot be typed
/// now.
// Refused, in the core's sentence, for a path the branch does not confine. Not a doc comment
// past here, because the generated bindings carry those.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
pub fn reference_into_chat(
    planes: tauri::State<'_, Planes>,
    clipboard: tauri::State<'_, crate::vaults::SystemClipboard>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: Option<String>,
    path: String,
    lines: Option<LineRange>,
    session: u32,
) -> Result<Handed, String> {
    let held = planes.held(&plane)?;
    let at = crate::piecefiles::branch(&workspace, &repo, &piece);
    into_chat(&held, at, &path, lines.map(Lines::from), session, &|text| {
        clipboard.put_text(text)
    })
}

/// [`reference_into_chat`], against a project the registry has already vouched for, with the
/// clipboard as `copy`.
pub fn into_chat(
    held: &Arc<Held>,
    at: purlis_core::files::Branch<'_>,
    path: &str,
    lines: Option<Lines>,
    session: u32,
    copy: &dyn Fn(&str) -> Result<(), String>,
) -> Result<Handed, String> {
    let chats = held.chats();
    let cwd = chats.chat_at(session).and_then(|chat| chat.cwd);
    let made = reference::of(held.root(), at, path, lines, cwd.as_deref())
        .map_err(|why| why.to_string())?;
    let harness = chats.harness(session);
    let text = rendered(harness, &made);
    let copied = |why: String| -> Result<Handed, String> {
        copy(&text)?;
        Ok(Handed::Copied {
            text: text.clone(),
            why,
        })
    };
    let Some(harness) = harness else {
        return copied(
            "purlis types a reference only into a chat on a harness it knows, so it is on \
             the clipboard"
                .to_owned(),
        );
    };
    if held.typed().waiting(session) {
        return copied(
            "the chat's first prompt has not been typed yet, so the reference is on the \
             clipboard"
                .to_owned(),
        );
    }
    let glance = held.board().glance(session);
    let readiness = chats.sessions().readiness(session)?;
    let now = Now {
        state: glance.state,
        asking: glance.asking,
        edits_lines: readiness.edits_lines,
    };
    if let Err(not_now) = reference::may_type_into(harness, now) {
        return copied(format!("{not_now}, so the reference is on the clipboard"));
    }
    // Judged on what is pasted: the reference and the space after it.
    if let Some(why) = harness.why_drawn_as_a_placeholder(&format!("{text} ")) {
        return copied(format!("{why}, so the reference is on the clipboard"));
    }
    // The operator's own act, so it goes where their keys go: it cancels a smart close as
    // typing does (ADR 0064).
    held.operator_input(session, reference::pasted(&text).as_bytes())?;
    Ok(Handed::Typed { text })
}

/// `made` in `harness`'s syntax, or in plain words for a program charter has no adapter for.
fn rendered(harness: Option<Harness>, made: &Reference) -> String {
    harness.map_or_else(|| made.plain(), |harness| harness.reference(made))
}

/// A chat "Start a chat here" opened: what the window needs to put its tab on the right strip.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct StartedHere {
    pub session: u32,
    /// The chat's name, which is its number.
    pub name: String,
    /// What its tab says: `About <file>`.
    pub label: String,
    /// The harness, by the word the plane calls it.
    pub harness: Option<String>,
    /// The workspace it is filed under.
    pub workspace: Option<String>,
    /// The reference, as typed or copied.
    pub text: String,
    /// Why it was copied rather than typed, or none when it is typed once the harness starts.
    pub copied: Option<String>,
}

/// Opens a chat on the project's default profile in the branch's own folder, with a reference
/// to one of its files or folders — or a range of lines of a file — typed as its first prompt
/// once its harness has started, and never sent. Where the harness cannot be typed into, the
/// chat still opens and the reference is put on the clipboard.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
pub fn start_chat_here(
    planes: tauri::State<'_, Planes>,
    clipboard: tauri::State<'_, crate::vaults::SystemClipboard>,
    plane: PlaneId,
    workspace: String,
    repo: String,
    piece: Option<String>,
    path: String,
    lines: Option<LineRange>,
    columns: u16,
    rows: u16,
) -> Result<StartedHere, String> {
    let held = planes.held(&plane)?;
    let at = crate::piecefiles::branch(&workspace, &repo, &piece);
    here(
        &held,
        at,
        &path,
        lines.map(Lines::from),
        Size { columns, rows },
        &|text| clipboard.put_text(text),
    )
}

/// [`start_chat_here`], against a project the registry has already vouched for.
pub fn here(
    held: &Arc<Held>,
    at: purlis_core::files::Branch<'_>,
    path: &str,
    lines: Option<Lines>,
    size: Size,
    copy: &dyn Fn(&str) -> Result<(), String>,
) -> Result<StartedHere, String> {
    let root = held.root().to_path_buf();
    // The branch's folder is found by the bounded reader's child, so this process starts no git
    // for it (#1189).
    let folder = purlis_core::files::root(&crate::reader(), &root, at)
        .and_then(|found| found.folder(""))
        .map_err(|why| why.to_string())?;
    let made =
        reference::of(&root, at, path, lines, Some(&folder)).map_err(|why| why.to_string())?;
    let (profile, kind) = curation::default_profile(&root, "no chat was started")?;
    let text = rendered(Harness::of_kind(&kind), &made);
    let label = label_for(path);
    let started = curation::start_typed_where_it_can_be(
        held,
        ChatTyped {
            profile,
            persona: None,
            cwd: folder.clone(),
            label: label.clone(),
            prompt: text.clone(),
            then_a_space: true,
        },
        size,
        |_, harness, _| Ok(harness.and_then(Harness::ready_to_type)),
        |why| format!("{why}, so the reference would not be readable and no chat was started"),
    )?;
    let copied = if started.typed {
        None
    } else {
        copy(&text)?;
        let harness = started
            .ready
            .harness
            .map_or("this profile's program", Harness::title);
        Some(format!(
            "purlis cannot type into {harness} yet, so the reference is on the clipboard"
        ))
    };
    Ok(StartedHere {
        session: started.session,
        name: started.name,
        label,
        harness: started.ready.harness.map(|h| h.name().to_owned()),
        workspace: purlis_core::workspaces::Plane::open(&root)
            .workspace_of(started.ready.cwd.as_deref().unwrap_or(&folder)),
        text,
        copied,
    })
}

/// What a "Start a chat here" chat's tab says: `About <name>`, held to a chat name's length.
fn label_for(path: &str) -> String {
    let leaf = Path::new(path).file_name().map_or_else(
        || path.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let whole = format!("About {leaf}");
    let most = purlis_core::reopen::MOST_LABEL;
    if whole.chars().count() <= most {
        return whole;
    }
    let mut cut: String = whole.chars().take(most.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const SIZE: Size = Size {
        columns: 80,
        rows: 24,
    };

    /// A project with a workspace `shop` holding one clone, `shop`, and a profile `work` of
    /// `kind`, the default, running a stand-in harness that waits on its input.
    struct Project {
        _dir: tempfile::TempDir,
        root: std::path::PathBuf,
    }

    impl Project {
        fn new(kind: &str) -> Self {
            let dir = tempfile::tempdir().expect("a directory");
            let root = std::fs::canonicalize(dir.path()).expect("it resolves");
            std::fs::write(root.join(purlis_core::plane::MANIFEST), "schema = 1\n").unwrap();
            let clone = root.join("workspaces/shop/shop");
            std::fs::create_dir_all(&clone).unwrap();
            std::fs::write(root.join("workspaces/shop/workspace.md"), "# shop\n").unwrap();
            git(&clone, &["init", "-q", "-b", "main", "."]);
            std::fs::write(clone.join("README.md"), "# shop\n").unwrap();
            git(&clone, &["add", "-A"]);
            let program = stand_in::program(
                &root,
                "harness-stand-in",
                "#!/bin/sh\nstty raw -echo\nexec cat > /dev/null\n",
            );
            std::fs::write(
                root.join(purlis_core::profiles::LOCAL_FILE),
                format!(
                    "[harness]\ndefault = \"work\"\n\n[harness.work]\nkind = {kind:?}\n\
                     command = [{:?}]\n",
                    program.display().to_string()
                ),
            )
            .unwrap();
            let set = purlis_core::profiles::current(&root);
            let work = set.get("work").expect("the profile reads");
            purlis_core::profiletrust::record_launched(
                &root,
                "work",
                &purlis_core::profiletrust::fingerprint(work),
            )
            .expect("approved");
            Self { _dir: dir, root }
        }
    }

    fn git(dir: &Path, args: &[&str]) {
        let ran = purlis_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(["-c", "user.email=t@e.invalid", "-c", "user.name=t"])
                .args(args),
        )
        .unwrap();
        assert!(ran.status.success(), "git {args:?}: {ran:?}");
    }

    fn planes() -> Planes {
        Planes::telling(Arc::new(|_| {}), crate::Shipped::default(), None)
    }

    fn shop() -> purlis_core::files::Branch<'static> {
        purlis_core::files::Branch::repo("shop", "shop")
    }

    #[cfg(unix)]
    #[test]
    fn start_a_chat_here_holds_the_reference_as_its_first_prompt_on_claude_code() {
        let project = Project::new("claude");
        let planes = planes();
        let held = planes.held(&planes.open(&project.root)).expect("held");
        let copied = Mutex::new(Vec::<String>::new());

        let started = here(&held, shop(), "README.md", None, SIZE, &|text| {
            copied.lock().unwrap().push(text.to_owned());
            Ok(())
        })
        .expect("it starts");

        assert_eq!(started.text, "@README.md");
        assert_eq!(started.copied, None);
        assert_eq!(started.label, "About README.md");
        assert_eq!(started.workspace.as_deref(), Some("shop"));
        assert!(
            held.typed().waiting(started.session),
            "held until it starts"
        );
        assert!(copied.lock().unwrap().is_empty());

        // A chat whose first prompt is still waiting is never typed into: the reference is
        // copied, and the waiting prompt is left as it was.
        let handed = into_chat(&held, shop(), "README.md", None, started.session, &|text| {
            copied.lock().unwrap().push(text.to_owned());
            Ok(())
        })
        .expect("it answers");
        assert!(matches!(handed, Handed::Copied { ref text, .. } if text == "@README.md"));
        assert!(held.typed().waiting(started.session));

        // Typed with a space after it, inside the one paste, so the operator's first word does
        // not stick to the mention.
        let typed = Mutex::new(String::new());
        held.typed().type_now(started.session, |paste| {
            *typed.lock().unwrap() = paste.to_owned();
            Ok(())
        });
        assert_eq!(*typed.lock().unwrap(), "\x1b[200~@README.md \x1b[201~");
        held.close_chat(started.session).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn start_a_chat_here_on_opencode_starts_it_and_copies_the_reference() {
        let project = Project::new("opencode");
        let planes = planes();
        let held = planes.held(&planes.open(&project.root)).expect("held");
        let copied = Mutex::new(Vec::<String>::new());

        let started = here(&held, shop(), "README.md", None, SIZE, &|text| {
            copied.lock().unwrap().push(text.to_owned());
            Ok(())
        })
        .expect("it starts");

        assert_eq!(started.text, "@README.md");
        assert!(started.copied.as_deref().unwrap().contains("opencode"));
        assert_eq!(*copied.lock().unwrap(), vec!["@README.md".to_owned()]);
        assert!(
            !held.typed().waiting(started.session),
            "nothing is held to type"
        );
        held.close_chat(started.session).unwrap();
    }

    #[test]
    fn a_path_out_of_the_branch_starts_nothing() {
        let project = Project::new("claude");
        let planes = planes();
        let held = planes.held(&planes.open(&project.root)).expect("held");

        for path in ["../workspace.md", ".git/config", "/etc/passwd"] {
            let refused = here(&held, shop(), path, None, SIZE, &|_| Ok(()));
            assert!(refused.is_err(), "{path} started a chat");
        }
        assert!(held.chats().open_now().is_empty());
    }

    #[test]
    fn a_tab_is_labelled_by_the_file_it_is_about() {
        assert_eq!(label_for("src/main.rs"), "About main.rs");
        assert_eq!(label_for("src"), "About src");
        assert!(label_for(&"x".repeat(500)).chars().count() <= purlis_core::reopen::MOST_LABEL);
    }
}
