//! Reading `.github/workflows/*.yml` the way the tests beside this need it, and no further.
//!
//! A line reader rather than a YAML parser: a workflow's jobs are the two-space keys under
//! `jobs:`, their settings the four-space keys below them, and their steps the six-space
//! `- ` items under `steps:`, which is all these tests need, and no YAML crate is in the
//! workspace for them.

#![allow(dead_code)] // Each test crate that includes this module uses a different part of it.

use std::path::{Path, PathBuf};

/// The repository's `.github/workflows` directory.
pub fn workflows_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows")
}

pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Every workflow file, sorted, so a failure names the same file on every run.
pub fn workflow_files() -> Vec<PathBuf> {
    let dir = workflows_dir();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("yml" | "yaml")))
        .collect();
    files.sort();
    files
}

/// One job under `jobs:`: its key, and the lines of its body with comments dropped.
pub struct Job {
    pub name: String,
    pub body: Vec<String>,
}

impl Job {
    pub fn reads_a_secret(&self) -> bool {
        self.body.iter().any(|l| l.contains("secrets."))
    }

    pub fn publishes(&self) -> bool {
        self.body.iter().any(|l| {
            l.contains("gh release create")
                || l.contains("gh release edit")
                || l.contains("replace-release-assets.sh")
        })
    }

    /// The job's own `environment:` value, if it has one.
    pub fn environment(&self) -> Option<&str> {
        self.body
            .iter()
            .find_map(|l| l.strip_prefix("    environment:"))
            .map(str::trim)
    }

    /// The job's own `if:` condition, if it has one written on that line.
    pub fn condition(&self) -> Option<&str> {
        self.body
            .iter()
            .find_map(|l| l.strip_prefix("    if:"))
            .map(str::trim)
    }

    /// The jobs this one `needs:`, from a one-line `needs: x` or `needs: [x, y]`.
    pub fn needs(&self) -> Vec<String> {
        self.body
            .iter()
            .find_map(|l| l.strip_prefix("    needs:"))
            .map(|v| {
                v.trim()
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .map(|n| n.trim().to_owned())
                    .filter(|n| !n.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The job's own `permissions:` block as `(scope, access)` pairs, sorted, or `None` when the
    /// job sets none and so takes the workflow's.
    pub fn permissions(&self) -> Option<Vec<(String, String)>> {
        let start = self
            .body
            .iter()
            .position(|l| l.trim_end() == "    permissions:")?;
        let mut pairs: Vec<(String, String)> = self.body[start + 1..]
            .iter()
            .filter(|l| !l.trim().is_empty())
            .take_while(|l| l.starts_with("      ") && !l.starts_with("       "))
            .filter_map(|l| l.trim().split_once(':'))
            .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
            .collect();
        pairs.sort();
        Some(pairs)
    }

    /// Whether the job may mint an OIDC token (`id-token: write`), which is what Sigstore signs
    /// build provenance with.
    pub fn can_mint_an_oidc_token(&self) -> bool {
        self.permissions()
            .is_some_and(|p| p.iter().any(|(k, v)| k == "id-token" && v == "write"))
    }

    /// The job's steps, each as its lines: the `- ` line and every more-indented one after it.
    pub fn steps(&self) -> Vec<Vec<String>> {
        let mut steps: Vec<Vec<String>> = Vec::new();
        let mut in_steps = false;
        for line in &self.body {
            if line.starts_with("    ") && !line.starts_with("     ") {
                in_steps = line.trim_end() == "    steps:";
                continue;
            }
            if !in_steps || line.trim().is_empty() {
                continue;
            }
            if line.starts_with("      - ") {
                steps.push(vec![line.clone()]);
            } else if let Some(step) = steps.last_mut() {
                step.push(line.clone());
            }
        }
        steps
    }
}

/// The action a step runs, as written after `uses:`, or `None` for a `run:` step.
pub fn step_uses(step: &[String]) -> Option<&str> {
    step.iter().find_map(|l| uses_value(l))
}

/// The value of `key` in the step's `with:` or `env:` map (`parent`), exactly as written after
/// the colon. Only the map's own keys count, so a comment or a line of a script never does.
pub fn step_value<'a>(step: &'a [String], parent: &str, key: &str) -> Option<&'a str> {
    let header = format!("        {parent}:");
    let start = step.iter().position(|l| l.trim_end() == header)?;
    step[start + 1..]
        .iter()
        .filter(|l| !l.trim().is_empty())
        .take_while(|l| l.starts_with("          "))
        .filter(|l| !l.starts_with("           ") && !l.trim_start().starts_with('#'))
        .find_map(|l| {
            let (k, v) = l.trim().split_once(':')?;
            (k == key).then(|| v.trim())
        })
}

/// The commands of the step's `run:`, one per line, trimmed, with blank lines and shell
/// comments left out: what the shell executes, and nothing a comment could add.
pub fn run_lines(step: &[String]) -> Vec<String> {
    let Some(start) = step.iter().position(|l| {
        let t = l.trim_start();
        let t = t.strip_prefix("- ").unwrap_or(t);
        t.starts_with("run:")
    }) else {
        return Vec::new();
    };
    let first = step[start].trim_start();
    let first = first.strip_prefix("- ").unwrap_or(first);
    let value = first["run:".len()..].trim();
    if value != "|" {
        return vec![value.to_owned()];
    }
    step[start + 1..]
        .iter()
        .take_while(|l| l.trim().is_empty() || l.starts_with("          "))
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// The value of a `uses:` key on this line, whether it opens a step (`- uses: x`) or not, with
/// any trailing comment cut off and quotes removed.
pub fn uses_value(line: &str) -> Option<&str> {
    let rest = line.trim_start();
    let rest = rest.strip_prefix("- ").unwrap_or(rest).trim_start();
    let value = rest.strip_prefix("uses:")?;
    let value = value.split(" #").next().unwrap_or(value).trim();
    Some(value.trim_matches(|c| c == '"' || c == '\''))
}

pub fn jobs(text: &str) -> Vec<Job> {
    let mut jobs: Vec<Job> = Vec::new();
    let mut in_jobs = false;
    for line in text.lines() {
        if line == "jobs:" {
            in_jobs = true;
            continue;
        }
        if !in_jobs || line.trim_start().starts_with('#') {
            continue;
        }
        let is_job =
            line.starts_with("  ") && !line.starts_with("   ") && line.trim_end().ends_with(':');
        if is_job {
            jobs.push(Job {
                name: line.trim().trim_end_matches(':').to_owned(),
                body: Vec::new(),
            });
        } else if let Some(job) = jobs.last_mut() {
            job.body.push(line.to_owned());
        }
    }
    jobs
}

/// The workflow-level `permissions:` block as `(scope, access)` pairs, sorted: what every job
/// that sets none of its own gets.
pub fn workflow_permissions(text: &str) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = text
        .lines()
        .skip_while(|l| *l != "permissions:")
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .filter_map(|l| l.trim().split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect();
    pairs.sort();
    pairs
}

/// The job called `name`, or a panic naming the one that is missing.
pub fn job<'a>(jobs: &'a [Job], name: &str) -> &'a Job {
    jobs.iter()
        .find(|j| j.name == name)
        .unwrap_or_else(|| panic!("release.yml has no `{name}` job"))
}

/// The `name:` of each `download-artifact` step in the job, sorted, after checking that none
/// downloads by pattern: a pattern takes whatever any step of the run uploaded under a
/// matching name, and that is not only the build jobs.
pub fn downloads_by_name(job: &Job) -> Vec<String> {
    let mut names: Vec<String> = job
        .steps()
        .iter()
        .filter(|s| step_uses(s).is_some_and(|u| u.starts_with("actions/download-artifact@")))
        .map(|s| {
            assert_eq!(
                step_value(s, "with", "pattern"),
                None,
                "job `{}` downloads by pattern: {s:#?}",
                job.name
            );
            step_value(s, "with", "name")
                .unwrap_or_else(|| panic!("job `{}` downloads without a name: {s:#?}", job.name))
                .to_owned()
        })
        .collect();
    names.sort();
    names
}

pub fn release_jobs() -> Vec<Job> {
    jobs(&read(&workflows_dir().join("release.yml")))
}
