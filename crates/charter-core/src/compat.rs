//! Whether this charter may write a project: its `schema` and its `requires` feature list
//! (FR-24; rulings V5, V37a, V37b). `docs/plane-format.md`, *Compatibility across charter
//! versions*, is the rule this module holds.

use std::path::Path;

/// The project format version this charter understands. A project that declares a higher one
/// is read-only to it (V37a), everywhere.
pub const SCHEMA: i64 = 2;

/// The version a `charter.toml` with no `schema` line is in.
pub const UNSTAMPED: i64 = 1;

/// What a `charter.toml`'s `schema` says, read in this one place for every caller.
#[derive(Debug, Clone, PartialEq)]
pub enum Schema {
    /// A version this charter understands: absent ([`UNSTAMPED`]) or at most [`SCHEMA`].
    Understood(i64),
    /// A version higher than [`SCHEMA`].
    TooNew(i64),
    /// A value that is not an integer.
    Unplaceable(toml::Value),
}

impl Schema {
    /// The refusal `path` meets when this charter does not understand its schema, in the one
    /// wording `doctor`, the forge commands and `init`/`reinit` all say, or `None`.
    pub fn refusal(&self, path: &str) -> Option<String> {
        match self {
            Schema::Understood(_) => None,
            Schema::TooNew(found) => Some(format!(
                "{path} declares schema {found}, but this charter understands {SCHEMA}. Upgrade \
                 charter: update the app."
            )),
            Schema::Unplaceable(other) => Some(format!(
                "{path} declares schema {}, which is not a project format version this \
                 charter can compare against {SCHEMA}. charter will not operate on a project \
                 whose format it cannot place. Fix the `schema` line, or upgrade charter: update \
                 the app.",
                // Quoted back as Python's `repr` writes it, the wording the refusal has always had.
                crate::pyrepr::repr_toml(other)
            )),
        }
    }
}

/// The `schema` of a parsed `charter.toml`.
pub fn schema(cfg: &toml::Table) -> Schema {
    match cfg.get("schema") {
        None => Schema::Understood(UNSTAMPED),
        Some(toml::Value::Integer(found)) if *found > SCHEMA => Schema::TooNew(*found),
        Some(toml::Value::Integer(found)) => Schema::Understood(*found),
        Some(other) => Schema::Unplaceable(other.clone()),
    }
}

/// The features this charter has. None yet: the first is added with the first change that
/// needs one, so any `requires` entry makes a project read-only to this charter.
const KNOWN: &[&str] = &[];

/// What this charter may do with a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compat {
    /// This charter understands the project's format and has every feature it requires.
    Writable,
    /// This charter reads the project and never writes it, for this reason.
    ReadOnly(Why),
}

/// Why a project is read-only to this charter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// The manifest (`file`, `charter.toml` or `purlis.toml`) could not be read or is not TOML.
    ManifestUnreadable { file: &'static str, detail: String },
    /// `schema` is there and is not an integer.
    SchemaUnplaceable { file: &'static str, found: String },
    /// `schema` is higher than [`SCHEMA`].
    SchemaTooNew { file: &'static str, found: i64 },
    /// `requires` has a shape this charter cannot read.
    RequiresUnreadable { detail: &'static str },
    /// The project requires a feature this charter does not have.
    Missing {
        feature: String,
        since: Option<String>,
    },
}

impl Why {
    /// What the operator does about it, as one sentence.
    pub fn remedy(&self) -> &'static str {
        match self {
            Why::ManifestUnreadable { file, .. } if *file == crate::names::PLANE_MANIFEST.write => {
                "Fix purlis.toml."
            }
            Why::ManifestUnreadable { .. } => "Fix charter.toml.",
            Why::RequiresUnreadable { .. } => {
                "Upgrade charter: update the app. Or fix `requires` in charter.toml."
            }
            Why::SchemaUnplaceable { .. } => {
                "Fix the `schema` line, or upgrade charter: update the app."
            }
            Why::SchemaTooNew { .. } | Why::Missing { .. } => "Upgrade charter: update the app.",
        }
    }
}

impl std::fmt::Display for Why {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let me = env!("CARGO_PKG_VERSION");
        match self {
            Why::ManifestUnreadable { file, detail } => write!(
                f,
                "{file} cannot be read ({detail}), so charter {me} cannot tell which \
                 format this project is in"
            )?,
            Why::SchemaUnplaceable { file, found } => write!(
                f,
                "{file} declares schema {found}, which is not a project format version \
                 charter {me} can compare against {SCHEMA}"
            )?,
            Why::SchemaTooNew { file, found } => write!(
                f,
                "{file} declares schema {found}, but this charter understands {SCHEMA} \
                 (it is charter {me})"
            )?,
            Why::RequiresUnreadable { detail } => write!(
                f,
                "{detail}, so charter {me} cannot tell whether it has every feature this \
                 project requires"
            )?,
            Why::Missing { feature, since } => {
                let needs = match since {
                    Some(since) => {
                        format!("charter {} or later has it", crate::shown::short(since))
                    }
                    None => "a newer charter has it".to_owned(),
                };
                write!(
                    f,
                    "this project requires the feature {}, which charter {me} does not have; \
                     {needs}",
                    crate::shown::short(feature)
                )?
            }
        }
        write!(
            f,
            ". charter opens the project read-only and writes nothing to it. {}",
            self.remedy()
        )
    }
}

/// Read the manifest at `root`: `purlis.toml` when it is there, else `charter.toml`.
///
/// **Fails closed.** A `charter.toml` that cannot be read, a `schema` this charter does not
/// understand, and a `requires` entry it cannot read each make the project read-only, because
/// a charter that guessed would write a project it does not understand. Only a directory with
/// no `charter.toml` at all, which is not a project, is answered `Writable`.
///
/// `requires` is honoured at any `schema`. A project that lists it also declares `schema = 2`
/// (V37a), which is what makes a charter older than FR-24 refuse it; a charter that knows
/// `requires` does not need the `schema` to read it.
pub fn read(root: &Path) -> Compat {
    let file = crate::names::manifest_name(root);
    let unreadable = |detail: String| Compat::ReadOnly(Why::ManifestUnreadable { file, detail });
    let raw = match std::fs::read(crate::names::manifest(root)) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Compat::Writable,
        Err(e) => return unreadable(e.to_string()),
    };
    let Ok(text) = String::from_utf8(raw) else {
        return unreadable("it is not UTF-8".to_owned());
    };
    let cfg: toml::Table = match text.parse() {
        Ok(cfg) => cfg,
        Err(e) => return unreadable(format!("it is not valid TOML: {}", e.message())),
    };
    match schema(&cfg) {
        Schema::Understood(_) => {}
        Schema::TooNew(found) => return Compat::ReadOnly(Why::SchemaTooNew { file, found }),
        Schema::Unplaceable(other) => {
            return Compat::ReadOnly(Why::SchemaUnplaceable {
                file,
                found: crate::shown::short(&other.to_string()),
            });
        }
    }
    let Some(requires) = cfg.get("requires") else {
        return Compat::Writable;
    };
    let Some(entries) = requires.as_array() else {
        return Compat::ReadOnly(Why::RequiresUnreadable {
            detail: "`requires` is not a list",
        });
    };
    for entry in entries {
        let Some(feature) = entry.get("feature").and_then(toml::Value::as_str) else {
            return Compat::ReadOnly(Why::RequiresUnreadable {
                detail: "a `requires` entry has no `feature` this charter can read",
            });
        };
        if KNOWN.contains(&feature) {
            continue;
        }
        return Compat::ReadOnly(Why::Missing {
            feature: feature.to_owned(),
            since: entry
                .get("since")
                .and_then(toml::Value::as_str)
                .map(str::to_owned),
        });
    }
    Compat::Writable
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(toml: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(crate::plane::MANIFEST), toml).unwrap();
        dir
    }

    fn why(toml: &str) -> Why {
        match read(project(toml).path()) {
            Compat::ReadOnly(why) => why,
            Compat::Writable => panic!("{toml:?} must make the project read-only"),
        }
    }

    #[test]
    fn a_project_requiring_a_feature_this_charter_lacks_names_it_and_its_version() {
        let found =
            why("schema = 2\nrequires = [{ feature = \"memory-proposals\", since = \"0.9.0\" }]\n");
        assert_eq!(
            found,
            Why::Missing {
                feature: "memory-proposals".into(),
                since: Some("0.9.0".into())
            }
        );
        let said = found.to_string();
        assert!(said.contains("memory-proposals"), "{said}");
        assert!(said.contains("0.9.0"), "{said}");
        assert!(said.contains("read-only"), "{said}");
    }

    fn purlis_project(toml: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("purlis.toml"), toml).unwrap();
        dir
    }

    #[test]
    fn a_schema_too_new_in_purlis_toml_is_refused_as_in_charter_toml() {
        assert_eq!(
            read(purlis_project("schema = 99\n").path()),
            Compat::ReadOnly(Why::SchemaTooNew {
                file: "purlis.toml",
                found: 99
            })
        );
        let Compat::ReadOnly(why) = read(purlis_project("schema = 99\n").path()) else {
            unreachable!()
        };
        assert!(
            why.to_string()
                .starts_with("purlis.toml declares schema 99"),
            "{why}"
        );
    }

    #[test]
    fn a_project_marked_by_purlis_toml_is_writable_now_every_reader_knows_it() {
        // D-RN1-12 lifted (RN-2a): every reader of the manifest asks `names::manifest`.
        let dir = purlis_project("schema = 2\n");
        assert_eq!(read(dir.path()), Compat::Writable);
        // Beside a charter.toml that does not even parse: purlis.toml is the manifest.
        std::fs::write(dir.path().join("charter.toml"), "not toml [").unwrap();
        assert_eq!(read(dir.path()), Compat::Writable);
    }

    #[test]
    fn a_project_this_charter_understands_is_writable() {
        for toml in [
            "",
            "schema = 1\n",
            "schema = 2\n",
            "schema = 2\nrequires = []\n",
        ] {
            assert_eq!(read(project(toml).path()), Compat::Writable, "{toml:?}");
        }
        assert_eq!(
            read(tempfile::tempdir().unwrap().path()),
            Compat::Writable,
            "a directory with no charter.toml is not a project, and says nothing"
        );
    }

    #[test]
    fn a_schema_this_charter_does_not_understand_is_read_only() {
        assert_eq!(
            why("schema = 3\n"),
            Why::SchemaTooNew {
                file: "charter.toml",
                found: 3
            }
        );
        assert!(matches!(
            why("schema = \"two\"\n"),
            Why::SchemaUnplaceable { .. }
        ));
        assert!(
            why("schema = 3\n")
                .to_string()
                .contains("declares schema 3, but this charter understands 2"),
            "the words `init` and `reinit` have always refused with"
        );
    }

    #[test]
    fn a_charter_toml_that_cannot_be_read_fails_closed() {
        assert!(matches!(why("[harness\n"), Why::ManifestUnreadable { .. }));
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(crate::plane::MANIFEST), [0xff, 0xfe]).unwrap();
        assert!(matches!(
            read(dir.path()),
            Compat::ReadOnly(Why::ManifestUnreadable { .. })
        ));
    }

    #[test]
    fn an_entry_charter_cannot_read_makes_the_project_read_only() {
        for toml in [
            "requires = \"memory-proposals\"\n",
            "requires = [\"memory-proposals\"]\n",
            "requires = [{ since = \"0.9.0\" }]\n",
            "requires = [{ feature = 3 }]\n",
        ] {
            assert!(
                matches!(why(toml), Why::RequiresUnreadable { .. }),
                "{toml:?}"
            );
        }
    }

    #[test]
    fn a_feature_with_no_since_still_says_a_newer_charter_has_it() {
        let found = why("requires = [{ feature = \"memory-proposals\" }]\n");
        assert!(found.to_string().contains("a newer charter"), "{found}");
    }

    #[test]
    fn a_schema_this_charter_does_not_understand_is_refused_in_one_wording() {
        let table = |t: &str| t.parse::<toml::Table>().unwrap();
        assert_eq!(schema(&table("schema = 2\n")), Schema::Understood(2));
        assert_eq!(schema(&table("")), Schema::Understood(UNSTAMPED));
        assert_eq!(
            schema(&table("schema = 2\n")).refusal("p/charter.toml"),
            None
        );
        assert_eq!(
            schema(&table("schema = 3\n"))
                .refusal("p/charter.toml")
                .as_deref(),
            Some(
                "p/charter.toml declares schema 3, but this charter understands 2. Upgrade \
                 charter: update the app."
            )
        );
        let unplaceable = schema(&table("schema = \"one\"\n"))
            .refusal("p/charter.toml")
            .unwrap();
        assert!(
            unplaceable.contains("which is not a project format version this charter can compare"),
            "{unplaceable}"
        );
    }

    #[test]
    fn the_remedy_for_an_old_charter_says_how_to_upgrade_it() {
        assert_eq!(
            Why::SchemaTooNew {
                file: "charter.toml",
                found: 3
            }
            .remedy(),
            "Upgrade charter: update the app."
        );
    }
}
