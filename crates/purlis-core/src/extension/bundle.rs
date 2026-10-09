//! The app bundle this program shipped in, and the built-in extensions inside it (#1366).
//!
//! The app learns where its built-ins are from Tauri's resource path. The `purlis` binary is a
//! sidecar in the same bundle, so it can find the same directory from where it is:
//!
//! - **macOS**: `purlis.app/Contents/MacOS/purlis` keeps them in
//!   `purlis.app/Contents/Resources/extensions`.
//! - **Linux**: `<usr>/bin/purlis` keeps them in `<usr>/lib/purlis/extensions`. In a `.deb`
//!   `<usr>` is `/usr`; in an AppImage it is the image's own `usr`, wherever it is mounted.
//!
//! **From this binary's real path and from nothing else.** The executable's path is resolved
//! through every link first, so a link to the binary on `PATH` finds the bundle the binary is
//! in, not the folder the link is in. No environment variable, setting, record or plane takes
//! part: a chat can set any of those, and a built-in is trusted with no approval (ADR 0041,
//! amended 2026-09-25 and 2026-10-09). This is stricter than Tauri's own answer, which on Linux
//! falls back to the `APPDIR` variable.
//!
//! **Not inside a bundle means no built-ins.** A development build in `target/`, a binary copied
//! out of its bundle, or a layout with a link anywhere between the executable's directory and
//! the extensions directory each get [`BuiltIn::none`]. A built-in that is missing is the
//! fail-closed answer; one trusted at a guessed path is not.

use std::path::{Path, PathBuf};

use super::BuiltIn;

/// The directory inside a bundle's resources that holds one directory per built-in extension.
pub const BUILT_IN_DIR: &str = "extensions";

/// The directory below `<usr>/lib` that a Linux bundle keeps its resources in: the app's
/// product name (`productName` in `tauri.conf.json`), which is the name Tauri's resource path
/// and its `.deb` and AppImage bundlers both use.
pub const LINUX_RESOURCES: &str = "purlis";

/// How a bundle lays out its executable and its resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// A macOS `.app`: `Contents/MacOS/<binary>` beside `Contents/Resources`.
    MacApp,
    /// A `.deb` or an AppImage: `usr/bin/<binary>` beside `usr/lib/purlis`.
    LinuxUsr,
}

impl Layout {
    /// The layout the app ships on this platform, or `None` where it ships no bundle.
    pub fn native() -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Self::MacApp)
        } else if cfg!(target_os = "linux") {
            Some(Self::LinuxUsr)
        } else {
            None
        }
    }

    /// Where a bundle of this layout keeps its resources, for the executable at `exe`; `None`
    /// when `exe` is not where this layout puts one.
    fn resources_for(self, exe: &Path) -> Option<PathBuf> {
        let dir = exe.parent()?;
        match self {
            Self::MacApp => {
                let contents = dir.parent()?;
                let app = contents.parent()?;
                let shaped = dir.file_name()? == "MacOS"
                    && contents.file_name()? == "Contents"
                    && app.extension()? == "app";
                shaped.then(|| contents.join("Resources"))
            }
            Self::LinuxUsr => {
                if dir.file_name()? != "bin" {
                    return None;
                }
                Some(dir.parent()?.join("lib").join(LINUX_RESOURCES))
            }
        }
    }
}

impl BuiltIn {
    /// The built-in extensions of the bundle that holds the executable at `exe`, laid out as
    /// `layout`; [`BuiltIn::none`] when `exe` is not inside such a bundle.
    ///
    /// `exe` is resolved through every link first. The extensions directory must then be a
    /// directory whose real path is the one the layout names, so a link anywhere from the
    /// executable's directory down to it refuses the whole bundle.
    pub fn beside(layout: Layout, exe: &Path) -> Self {
        let Some(dir) = exe
            .canonicalize()
            .ok()
            .and_then(|exe| layout.resources_for(&exe))
            .map(|resources| resources.join(BUILT_IN_DIR))
        else {
            return Self::none();
        };
        match dir.canonicalize() {
            Ok(real) if real == dir && real.is_dir() => Self::at(real),
            _ => Self::none(),
        }
    }

    /// The built-in extensions of the bundle this program shipped in, found from its own
    /// executable ([`Self::beside`]); [`BuiltIn::none`] outside a bundle and on a platform the
    /// app ships no bundle for.
    pub fn of_this_program() -> Self {
        let Some(layout) = Layout::native() else {
            return Self::none();
        };
        std::env::current_exe().map_or_else(|_| Self::none(), |exe| Self::beside(layout, &exe))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A scratch directory by its real path, as `beside` compares real paths (`/var` is a link
    /// on macOS).
    fn scratch() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a scratch directory");
        let real = dir.path().canonicalize().expect("its real path");
        (dir, real)
    }

    /// An executable file at `at`, and its parents.
    fn exe(at: &Path) -> PathBuf {
        std::fs::create_dir_all(at.parent().expect("a parent")).expect("its directory");
        std::fs::write(at, "").expect("the binary");
        at.to_path_buf()
    }

    fn dir(at: &Path) -> PathBuf {
        std::fs::create_dir_all(at).expect("the directory");
        at.to_path_buf()
    }

    #[test]
    fn a_macos_app_keeps_its_built_ins_in_its_resources() {
        let (_keep, root) = scratch();
        let app = root.join("purlis.app/Contents");
        let binary = exe(&app.join("MacOS/purlis"));
        let extensions = dir(&app.join("Resources/extensions"));
        assert_eq!(
            BuiltIn::beside(Layout::MacApp, &binary),
            BuiltIn::at(extensions)
        );
    }

    #[test]
    fn a_deb_or_an_appimage_keeps_its_built_ins_under_usr_lib_purlis() {
        let (_keep, root) = scratch();
        // A `.deb` installs to `/usr`; an AppImage mounts the same tree anywhere.
        let usr = root.join("mount_ab12/usr");
        let binary = exe(&usr.join("bin/purlis"));
        let extensions = dir(&usr.join("lib/purlis/extensions"));
        assert_eq!(
            BuiltIn::beside(Layout::LinuxUsr, &binary),
            BuiltIn::at(extensions)
        );
    }

    #[test]
    fn a_link_to_the_binary_finds_the_bundle_the_binary_is_in() {
        let (_keep, root) = scratch();
        let app = root.join("purlis.app/Contents");
        let binary = exe(&app.join("MacOS/purlis"));
        let extensions = dir(&app.join("Resources/extensions"));
        // The link stands in a folder shaped like another bundle, which it does not speak for.
        let elsewhere = root.join("other.app/Contents");
        dir(&elsewhere.join("Resources/extensions"));
        let link = dir(&elsewhere.join("MacOS")).join("purlis");
        std::os::unix::fs::symlink(&binary, &link).expect("the link");
        assert_eq!(
            BuiltIn::beside(Layout::MacApp, &link),
            BuiltIn::at(extensions)
        );
    }

    #[test]
    fn a_binary_outside_a_bundle_has_no_built_ins() {
        let (_keep, root) = scratch();
        // A development build, with a resources-shaped folder beside it all the same.
        let binary = exe(&root.join("target/debug/purlis"));
        dir(&root.join("target/Resources/extensions"));
        dir(&root.join("target/lib/purlis/extensions"));
        assert_eq!(BuiltIn::beside(Layout::MacApp, &binary), BuiltIn::none());
        assert_eq!(BuiltIn::beside(Layout::LinuxUsr, &binary), BuiltIn::none());
    }

    #[test]
    fn a_folder_shaped_almost_like_an_app_is_not_one() {
        let (_keep, root) = scratch();
        for almost in [
            "purlis/Contents/MacOS",
            "purlis.app/Other/MacOS",
            "purlis.app/Contents/bin",
        ] {
            let binary = exe(&root.join(almost).join("purlis"));
            let contents = binary.parent().and_then(Path::parent).expect("a parent");
            dir(&contents.join("Resources/extensions"));
            assert_eq!(
                BuiltIn::beside(Layout::MacApp, &binary),
                BuiltIn::none(),
                "{almost}"
            );
        }
    }

    #[test]
    fn a_linux_bundle_is_read_only_under_purlis_own_name() {
        let (_keep, root) = scratch();
        let usr = root.join("usr");
        let binary = exe(&usr.join("bin/purlis"));
        // Another package's extensions, under its own name in the same `lib`.
        dir(&usr.join("lib/someone-else/extensions"));
        assert_eq!(BuiltIn::beside(Layout::LinuxUsr, &binary), BuiltIn::none());
    }

    #[test]
    fn a_bundle_without_built_ins_has_none() {
        let (_keep, root) = scratch();
        let binary = exe(&root.join("purlis.app/Contents/MacOS/purlis"));
        dir(&root.join("purlis.app/Contents/Resources"));
        assert_eq!(BuiltIn::beside(Layout::MacApp, &binary), BuiltIn::none());
        // A file where the directory should be is no directory of built-ins either.
        std::fs::write(root.join("purlis.app/Contents/Resources/extensions"), "").expect("a file");
        assert_eq!(BuiltIn::beside(Layout::MacApp, &binary), BuiltIn::none());
    }

    #[test]
    fn a_link_inside_the_bundle_refuses_it() {
        let (_keep, root) = scratch();
        let outside = dir(&root.join("outside/extensions"));
        // The extensions directory itself a link out of the bundle.
        let app = root.join("a.app/Contents");
        let binary = exe(&app.join("MacOS/purlis"));
        dir(&app.join("Resources"));
        std::os::unix::fs::symlink(&outside, app.join("Resources/extensions")).expect("link");
        assert_eq!(BuiltIn::beside(Layout::MacApp, &binary), BuiltIn::none());
        // `lib/purlis` a link out of a Linux bundle.
        let usr = root.join("usr");
        let binary = exe(&usr.join("bin/purlis"));
        dir(&usr.join("lib"));
        std::os::unix::fs::symlink(root.join("outside"), usr.join("lib/purlis")).expect("link");
        assert_eq!(BuiltIn::beside(Layout::LinuxUsr, &binary), BuiltIn::none());
    }

    #[test]
    fn a_binary_that_is_not_there_has_no_built_ins() {
        let (_keep, root) = scratch();
        let app = root.join("purlis.app/Contents");
        dir(&app.join("Resources/extensions"));
        assert_eq!(
            BuiltIn::beside(Layout::MacApp, &app.join("MacOS/purlis")),
            BuiltIn::none()
        );
    }

    #[test]
    fn the_linux_resources_directory_is_the_apps_product_name() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../../../../app/src-tauri/tauri.conf.json"))
                .expect("tauri.conf.json is JSON");
        assert_eq!(conf["productName"], LINUX_RESOURCES);
    }

    #[test]
    fn a_test_binary_is_in_no_bundle() {
        // This test runs from `target/`, which is no bundle on any platform.
        assert_eq!(BuiltIn::of_this_program(), BuiltIn::none());
    }
}
