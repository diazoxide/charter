//! The palette's *Install `charter` command in PATH*: the app's own command line, linked into
//! `/usr/local/bin` as `purlis` and, for the rename's window, as `charter` (RN-3), on the
//! operator's word (`purlis_core::clipath` holds the rule).
//!
//! Only on macOS. A `.deb` already lays `/usr/bin/purlis` and `/usr/bin/charter` down, and an AppImage runs from a
//! mount point that changes at every launch, so a link to its binary would be dead by the next
//! one — the row says so rather than making a link that stops working.

use purlis_core::clipath::{self, Linked};

/// Links the app's command line into `/usr/local/bin` as `purlis` and as `charter`, asking
/// macOS for an administrator's password once when that directory is not this user's to write.
/// Answers the sentence to say.
#[tauri::command]
#[specta::specta]
pub fn install_cli_on_path() -> Result<String, String> {
    if !cfg!(target_os = "macos") {
        return Err(
            "purlis puts itself on PATH from here on macOS only. On Linux the .deb \
                    installs /usr/bin/purlis and /usr/bin/charter; with the AppImage, run the \
                    purlis inside it by its path."
                .to_owned(),
        );
    }
    let binary = crate::charter_binary()
        .ok_or("this build has no `purlis` beside the app, so there is nothing to put on PATH.")?;
    let dir = std::path::Path::new(clipath::MACOS_DIR);
    let mut said = Vec::new();
    let mut refused = Vec::new();
    let mut for_admin = Vec::new();
    for (link, answer) in clipath::link_all(dir, &binary) {
        match answer {
            Linked::Done(done) => said.push((link, done)),
            Linked::Refused(why) => refused.push(why),
            Linked::NeedsAdmin => for_admin.push(link),
        }
    }
    if !for_admin.is_empty() {
        match as_administrator(&for_admin, &binary) {
            Ok(done) => said.extend(done),
            Err(why) => refused.push(why),
        }
    }
    if said.is_empty() {
        return Err(refused.join(" "));
    }
    let home = purlis_core::profiles::home();
    let mut sentences = Vec::new();
    for (link, done) in said {
        sentences.push(done);
        let others = clipath::others(home.as_deref(), &link);
        if !others.is_empty() {
            let named: Vec<String> = others.iter().map(|p| p.display().to_string()).collect();
            sentences.push(format!(
                "Another one is at {} — a terminal whose PATH lists that directory first \
                 still finds that one.",
                named.join(", ")
            ));
        }
    }
    sentences.extend(refused);
    Ok(sentences.join(" "))
}

/// The one privileged step, through macOS's own password dialog: the same thing VS Code's
/// "Install 'code' command in PATH" does, and nothing else runs with the privilege.
///
/// `ln -sfn` is safe here only because [`clipath::link`] already decided each place is empty
/// or holds a link an app made; a regular file or somebody else's link never reaches this.
///
/// Every link in one script, so the operator is asked for their password once, not per name.
fn as_administrator(
    links: &[std::path::PathBuf],
    binary: &std::path::Path,
) -> Result<Vec<(std::path::PathBuf, String)>, String> {
    let script = admin_script(links, binary);
    let out = purlis_core::forklock::output(
        std::process::Command::new("/usr/bin/osascript").args(["-e", &script]),
    )
    .map_err(|e| {
        format!("purlis could not ask macOS for permission ({e}); nothing was changed.")
    })?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr);
        // -128 is AppleScript's "User canceled".
        return Err(if said.contains("-128") {
            "Cancelled — nothing was changed.".to_owned()
        } else {
            format!(
                "macOS did not make the link ({}); nothing was changed.",
                said.trim()
            )
        });
    }
    links
        .iter()
        .map(|link| match clipath::plan(link, binary) {
            clipath::Plan::AlreadyThere => Ok((link.clone(), clipath::said(link, binary, false))),
            _ => Err(format!(
                "macOS said yes, and {} still does not point at this app's command line.",
                link.display()
            )),
        })
        .collect()
}

/// The `osascript` line that makes `links` with the privilege: the directory each is in, then
/// one `ln -sfn` per link, joined so a failure stops the rest.
fn admin_script(links: &[std::path::PathBuf], binary: &std::path::Path) -> String {
    let quoted = |path: &std::path::Path| {
        format!(
            "quoted form of {}",
            applescript_string(&path.display().to_string())
        )
    };
    let mut steps = Vec::new();
    for link in links {
        let dir = link.parent().unwrap_or(std::path::Path::new("/"));
        steps.push(format!("\"/bin/mkdir -p \" & {}", quoted(dir)));
        steps.push(format!(
            "\"/bin/ln -sfn \" & {} & \" \" & {}",
            quoted(binary),
            quoted(link)
        ));
    }
    format!(
        "do shell script {} with administrator privileges",
        steps.join(" & \" && \" & ")
    )
}

/// `text` as an AppleScript string literal.
fn applescript_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_password_prompt_makes_every_link() {
        let links = [
            std::path::PathBuf::from("/usr/local/bin/purlis"),
            std::path::PathBuf::from("/usr/local/bin/charter"),
        ];
        let script = admin_script(
            &links,
            std::path::Path::new("/Apps/p.app/Contents/MacOS/purlis"),
        );
        assert_eq!(
            script,
            r#"do shell script "/bin/mkdir -p " & quoted form of "/usr/local/bin" & " && " & "/bin/ln -sfn " & quoted form of "/Apps/p.app/Contents/MacOS/purlis" & " " & quoted form of "/usr/local/bin/purlis" & " && " & "/bin/mkdir -p " & quoted form of "/usr/local/bin" & " && " & "/bin/ln -sfn " & quoted form of "/Apps/p.app/Contents/MacOS/purlis" & " " & quoted form of "/usr/local/bin/charter" with administrator privileges"#
        );
    }

    #[test]
    fn a_path_cannot_break_out_of_its_applescript_string() {
        assert_eq!(
            applescript_string(r#"/Apps/a "b" \c/charter"#),
            r#""/Apps/a \"b\" \\c/charter""#
        );
    }
}
