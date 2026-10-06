//! The scene a contract case asks about: the owner, its two repos, the request open on the save
//! branch and the request that landed (FG-4).
//!
//! Every case is written against a [`Scene`], never against a literal name, so the same case runs
//! on a recording and on a real forge. The recorded scene is the one every recording describes
//! (`acme`, request 12). The live scene is a fixture the operator provisioned on a real forge
//! (`docs/forges.md`, "The live nightly"), read from that forge when the live run starts
//! (`live.rs`).
//!
//! What a scene cannot know before a case runs, the number a forge gives a request or an issue
//! it opens and the id it gives that issue, is `None` in the live scene: the case then holds the
//! answer to its shape, and to the number the answer itself carries, rather than to a literal.

use purlis_core::forge::{ForgeRef, Kind, RepoRecord};

/// The head every recording's request is at.
pub const SHA: &str = "6dcb09b5b57875f334f61aebed695e2e4193db5e";
/// The commit a recorded merged request landed as.
pub const MERGE: &str = "e5bd3914e2e596debea16f433f57875b5b90bcd6";

/// The branch whose open request the read cases ask about.
pub const SAVE: &str = "charter/save";

/// One scene, on one forge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scene {
    pub kind: Kind,
    /// The instance the scene's URLs are on.
    pub host: String,
    /// The org or group holding `api` and `web`.
    pub owner: String,
    /// The forge's id of `api`, and of `web`.
    pub api_id: String,
    pub web_id: String,
    /// The id the top-level case asks by. The recordings were written one per case, and theirs
    /// is 7; on a live forge it is `api`'s.
    pub tree_id: String,
    /// The request open from [`SAVE`] into `main`, and the commit it is at.
    pub open: u64,
    pub head: String,
    /// A request that landed, and the commit it landed as.
    pub merged: u64,
    pub merge: String,
    /// The branch `open_or_update` opens from, and the number its request has, when known.
    pub opened_from: String,
    pub opened: Option<u64>,
    /// The number and forge id the issue `create` opens has, when known.
    pub created: Option<(u64, String)>,
    /// Whether `main` lands through a merge queue or train.
    pub queue: bool,
}

impl Scene {
    /// The scene every recording describes.
    pub fn recorded(kind: Kind) -> Scene {
        Scene {
            kind,
            host: kind.default_host().to_string(),
            owner: "acme".into(),
            api_id: "1".into(),
            web_id: "2".into(),
            tree_id: "7".into(),
            open: 12,
            head: SHA.into(),
            merged: 12,
            merge: MERGE.into(),
            opened_from: SAVE.into(),
            opened: Some(12),
            created: Some((
                12,
                match kind {
                    Kind::GitHub => "I_kwDOAcme12",
                    Kind::GitLab => "84012",
                }
                .into(),
            )),
            queue: true,
        }
    }

    /// `owner/name`.
    pub fn path(&self, name: &str) -> String {
        format!("{}/{name}", self.owner)
    }

    /// The record repo `name` of the scene becomes, in neutral fields, with forge id `id`. GitLab
    /// names a repo by its `path` (`api`), not its display name (`Api`).
    pub fn repo(&self, id: &str, name: &str, topics: &[&str]) -> RepoRecord {
        let host = &self.host;
        let path = self.path(name);
        RepoRecord {
            id: Some(ForgeRef(id.into())),
            name: name.into(),
            path_with_namespace: path.clone(),
            default_branch: Some("main".into()),
            description: String::new(),
            web_url: format!("https://{host}/{path}"),
            ssh_url: format!("git@{host}:{path}.git"),
            topics: topics.iter().map(|t| t.to_string()).collect(),
            forge: self.kind,
        }
    }

    /// `api`, as an owner's listing answers it.
    pub fn api(&self) -> RepoRecord {
        self.repo(&self.api_id, "api", &[])
    }

    /// `web`, tagged `frontend`.
    pub fn web(&self) -> RepoRecord {
        self.repo(&self.web_id, "web", &["frontend"])
    }

    /// The page of request `number` of `api`.
    pub fn request_url(&self, number: u64) -> String {
        match self.kind {
            Kind::GitHub => format!("https://{}/{}/pull/{number}", self.host, self.path("api")),
            Kind::GitLab => format!(
                "https://{}/{}/-/merge_requests/{number}",
                self.host,
                self.path("api")
            ),
        }
    }

    /// The page of issue `number` of `api`.
    pub fn issue_url(&self, number: u64) -> String {
        let sep = match self.kind {
            Kind::GitHub => "",
            Kind::GitLab => "-/",
        };
        format!(
            "https://{}/{}/{sep}issues/{number}",
            self.host,
            self.path("api")
        )
    }

    /// The tracker key of issue `number` of `api`.
    pub fn issue_key(&self, number: u64) -> String {
        format!(
            "{}:{}/{}#{number}",
            self.kind.word(),
            self.host,
            self.path("api")
        )
    }
}
