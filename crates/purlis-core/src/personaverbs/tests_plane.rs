//! The planes the persona verbs' own tests run against: the committed `daily` and `minimal`
//! fixture planes (`tests/fixtures/planes/`), copied into a temp dir, with the files a
//! recorded scenario (`tests/fixtures/recorded/behaviour.jsonl`) adds on top of them.
//!
//! The recorded scenarios are replayed by purlis-cli, which the nightly mutation run does not
//! build; these put the same starting planes in front of purlis-core's own tests, so an
//! expectation copied from a recorded row is the Python charter's answer for the same bytes.
//! Nothing here reaches a keychain, 1Password, a clipboard or a live plane: every vault is a
//! plain-file vault, and every path is under the temp dir.

use std::path::{Path, PathBuf};

use crate::repocmd::Say;

/// A sub-agent file as `persona sync-agents` wrote one for `name`, before it was retired
/// (#1451): the frontmatter, the marker line, and the sentence that names the persona.
pub fn generated_agent(name: &str) -> String {
    let marker = crate::names::SYNC_AGENTS_MARKER
        .spellings()
        .last()
        .expect("a spelling");
    format!(
        "---\nname: {name}\ndescription: \"The {name} persona.\"\n---\n<!-- {marker} from \
         personas/{name}/persona.md — edit the persona, not this file. -->\n\nThis sub-agent \
         acts as the **{name}** persona — Role — in an\nisolated context. Adopt the charter \
         below as your role.\n\n# {name}\n"
    )
}

/// `personas/ops/persona.md` of the `persona-use-…` and `persona-stats-…` scenarios.
pub const OPS: &str = "---\nname: ops\nrole: Operations Engineer\nvault: ops\ntools: kubectl, gh\n\
agent-tools: Read, Bash\nuses: devops, steward\nskills: superpowers:tdd, deploy\n\
dispatch-isolation: worktree\ndisallowed-tools: WebFetch\nmodel: sonnet\ncolor: blue\n\
memory: project\ndelegate-when: cluster operations and \"quoted\" rollouts\n---\n\n\
# Operations\n\nKeeps the clusters up.\n";

/// `personas/ops-lite/persona.md` of the same scenarios: extends `ops`, holds no vault.
pub const OPS_LITE: &str = "---\nname: ops-lite\nextends: ops\nrole: Lite Operator\nvault: none\n\
borrows: none\nagent-tools: Grep\ndelegate-when: read-only cluster questions\n---\n\n# Lite\n\n\
Only looks, never touches.\n";

/// `personas/solo/persona.md`: no role, no vault key — its vault comes from the registry.
pub const SOLO: &str = "---\nname: solo\ndelegate-when: solo work\n---\n\n\
Solo has no role and no vault key of its own.\n";

/// `personas/ops/mcp.json`: a plain server, two credentialed ones and a refused name.
pub const OPS_MCP: &str = r#"{
  "mcpServers": {
    "status": {"type": "http", "url": "https://status.example.com/mcp"},
    "grafana": {
      "type": "stdio",
      "command": "npx",
      "args": ["-y", "grafana-mcp@1.2.0"],
      "env": {"GRAFANA_URL": "https://grafana.example.com"},
      "secrets": {"GRAFANA_TOKEN": "grafana-token"}
    },
    "gsc": {
      "type": "stdio",
      "command": "uvx",
      "args": ["gsc-mcp==0.3.0"],
      "secret_files": {"GOOGLE_APPLICATION_CREDENTIALS": "GOOGLE_SA"}
    },
    "bad name": {"type": "http", "url": "https://x.example.com"}
  }
}
"#;

/// `.charter/vaults.json` of the same scenarios: two plain-file vaults, tagged with a persona.
pub const VAULTS: &str = r#"{
  "vaults": {
    "fixture": {"provider": "plain-file", "config": {"file": ".charter/vaults/fixture.json"}, "persona": "steward"},
    "solo-vault": {"provider": "plain-file", "config": {"file": ".charter/vaults/solo.json"}, "persona": "solo"}
  }
}
"#;

/// The two fingerprints the Python charter recorded for `ops/grafana` and `ops/gsc` when it
/// approved them, in the order it wrote them.
pub const OPS_APPROVED: [&str; 2] = [
    "6c0e43fb8762aaf6beb63444e07c956bebf55d8d2a7e40b1f9a9f967c8a1658a",
    "872898db1e4a8c92491cfa50d4ac4a71b375a75630adf0bd995b06de5a362e76",
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/planes")
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// A fixture plane's `charter.toml`, `personas/` and vault registry, in a temp dir.
pub struct Plane {
    pub dir: tempfile::TempDir,
}

impl Plane {
    /// `tests/fixtures/planes/<name>`: `daily` (a draft `devops` with one memory, `steward`
    /// with none, one shared memory and one dispatch to `devops`) or `minimal` (`steward`
    /// alone).
    pub fn fixture(name: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let from = fixtures().join(name);
        std::fs::copy(from.join("charter.toml"), dir.path().join("charter.toml")).unwrap();
        copy_tree(&from.join("personas"), &dir.path().join("personas"));
        let registry = from.join(".charter/vaults.json");
        if registry.exists() {
            std::fs::create_dir_all(dir.path().join(".charter")).unwrap();
            std::fs::copy(registry, dir.path().join(".charter/vaults.json")).unwrap();
        }
        Self { dir }
    }

    /// The `daily` plane with the `ops`, `ops-lite` and `solo` personas the richer recorded
    /// scenarios add, `ops`'s MCP servers and executables, and their vault registry.
    pub fn daily_with_ops() -> Self {
        let plane = Self::fixture("daily");
        plane.write("personas/ops/persona.md", OPS);
        plane.write("personas/ops-lite/persona.md", OPS_LITE);
        plane.write("personas/solo/persona.md", SOLO);
        plane.write("personas/ops/mcp.json", OPS_MCP);
        plane.write("personas/ops/bin/check.sh", "#!/bin/sh\necho ok\n");
        plane.write("personas/ops/bin/notes.txt", "not a script\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                plane.path("personas/ops/bin/check.sh"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        plane.write(".charter/vaults.json", VAULTS);
        plane
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// The plane's state directory, as [`super::state_dir`] resolves it under `cfg(test)`.
    pub fn state(&self) -> PathBuf {
        self.dir.path().join(".charter")
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    pub fn write(&self, rel: &str, text: &str) {
        let path = self.path(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    pub fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.path(rel)).unwrap()
    }
}

/// What a command said, split the way the binary splits it: [`Say::Out`] is stdout, every
/// other voice is stderr — each line as the terminal shows it, `\n`-terminated, which is how
/// the recorded rows hold them.
#[derive(Default)]
pub struct Heard {
    pub out: String,
    pub err: String,
}

impl Heard {
    pub fn sink(&mut self) -> impl FnMut(Say) + '_ {
        |line| {
            let text = format!("{line}\n");
            match line {
                Say::Out(_) => self.out.push_str(&text),
                _ => self.err.push_str(&text),
            }
        }
    }
}
