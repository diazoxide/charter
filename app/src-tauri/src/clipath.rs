//! The palette's *Install `charter` command in PATH*: the app's own `charter`, linked into
//! `/usr/local/bin` on the operator's word (`charter_core::clipath` holds the rule).
//!
//! Only on macOS. A `.deb` already lays `/usr/bin/charter` down, and an AppImage runs from a
//! mount point that changes at every launch, so a link to its binary would be dead by the next
//! one — the row says so rather than making a link that stops working.

use charter_core::clipath::{self, Linked};

/// Links the app's `charter` into `/usr/local/bin`, asking macOS for an administrator's
/// password when that directory is not this user's to write. Answers the sentence to say.
#[tauri::command]
#[specta::specta]
pub fn install_cli_on_path() -> Result<String, String> {
    if !cfg!(target_os = "macos") {
        return Err(
            "charter puts itself on PATH from here on macOS only. On Linux the .deb \
                    installs /usr/bin/charter; with the AppImage, run the charter inside it \
                    by its path."
                .to_owned(),
        );
    }
    let binary = crate::charter_binary()
        .ok_or("this build has no `charter` beside the app, so there is nothing to put on PATH.")?;
    let link = std::path::Path::new(clipath::MACOS_DIR).join(clipath::COMMAND);
    let said = match clipath::link(&link, &binary) {
        Linked::Done(said) => said,
        Linked::Refused(why) => return Err(why),
        Linked::NeedsAdmin => as_administrator(&link, &binary)?,
    };
    let others = clipath::others(charter_core::profiles::home().as_deref(), &link);
    if others.is_empty() {
        return Ok(said);
    }
    let named: Vec<String> = others.iter().map(|p| p.display().to_string()).collect();
    Ok(format!(
        "{said} Another charter is at {} — a terminal whose PATH lists that directory first \
         still finds that one.",
        named.join(", ")
    ))
}

/// The one privileged step, through macOS's own password dialog: the same thing VS Code's
/// "Install 'code' command in PATH" does, and nothing else runs with the privilege.
///
/// `ln -sfn` is safe here only because [`clipath::link`] already decided the place is empty or
/// holds a link an app made; a regular file or somebody else's link never reaches this.
fn as_administrator(link: &std::path::Path, binary: &std::path::Path) -> Result<String, String> {
    let dir = link.parent().unwrap_or(std::path::Path::new("/"));
    let script = format!(
        "do shell script \"/bin/mkdir -p \" & quoted form of {} & \" && /bin/ln -sfn \" & \
         quoted form of {} & \" \" & quoted form of {} with administrator privileges",
        applescript_string(&dir.display().to_string()),
        applescript_string(&binary.display().to_string()),
        applescript_string(&link.display().to_string()),
    );
    let out = charter_core::forklock::output(
        std::process::Command::new("/usr/bin/osascript").args(["-e", &script]),
    )
    .map_err(|e| {
        format!("charter could not ask macOS for permission ({e}); nothing was changed.")
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
    match clipath::plan(link, binary) {
        clipath::Plan::AlreadyThere => Ok(clipath::said(link, binary, false)),
        _ => Err(format!(
            "macOS said yes, and {} still does not point at this app's charter.",
            link.display()
        )),
    }
}

/// `text` as an AppleScript string literal.
fn applescript_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_cannot_break_out_of_its_applescript_string() {
        assert_eq!(
            applescript_string(r#"/Apps/a "b" \c/charter"#),
            r#""/Apps/a \"b\" \\c/charter""#
        );
    }
}
