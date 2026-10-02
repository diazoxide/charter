//! ADR 0070 §4: `a_forge_token_appears_in_no_log_record_or_crash_report`.
//!
//! A child of this binary signs a canary token in, installs the diagnostic log (FD-8's
//! `applog`, into a directory of its own) and a `log` recorder at trace level for what `ureq`
//! and rustls write (with `RUST_LOG=trace` set too), and makes every forge seam operation
//! natively three times: against a recorded forge that answers, against one that refuses with
//! a `401`, and against a port nothing listens on. It writes every
//! answer and every error to the log, keeps its ETag store in the same run directory, and then
//! crashes: a panic on a thread holding the token, then one on the main thread.
//!
//! The parent searches everything the run wrote, every file under the run directory and the
//! child's standard output and error, for the canary. It also checks the run did what it says:
//! the log holds the run's own lines, the network log (OB-15) lists the native requests, and
//! the recorded forge was sent the token, so a quiet log or an unauthenticated run cannot pass
//! for a clean one. The canary is not shaped like a credential, so the log's own redaction
//! could not hide a leak from this.
//!
//! OB-11's crash reports (minidumps) do not exist yet; a panic's report is what it writes to
//! standard error, which this searches. When OB-11 lands, its minidump joins the search.

use std::path::Path;
use std::sync::Arc;

mod support;
use support::forge_cli::bare_repo;

use charter_core::forge::http::{ApiRoot, TokenSource};
use charter_core::forge::pr::Pr;
use charter_core::forge::route::{HostScope, Resolver, SignIn};
use charter_core::forge::{
    Account, Caller, Capability, Forge, ForgeError, Kind, Owner, Reach, RepoRecord,
};
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

const CANARY: &str = "CANARYforgeToken4c1d9b";
const CHILD: &str = "CHARTER_TEST_TOKEN_LEAK_CHILD";
const RUN: &str = "CHARTER_TEST_TOKEN_LEAK_RUN";

struct Canary;
impl TokenSource for Canary {
    fn token(&self) -> Result<SecretString, ForgeError> {
        Ok(SecretString::from(CANARY))
    }
}

fn account() -> Account {
    Account {
        kind: Kind::GitHub,
        host: "github.com".into(),
        login: "octocat".into(),
    }
}

/// Every seam operation, as a human in the window, against a forge answering `status`.
fn every_operation(rt: &tokio::runtime::Runtime, run: &Path, status: u16) {
    let server = rt.block_on(MockServer::start());
    // Status 0: nothing listens where the forge should be.
    let root = if status == 0 {
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = closed.local_addr().unwrap().port();
        drop(closed);
        ApiRoot::at(&format!("http://127.0.0.1:{port}"))
    } else {
        ApiRoot::at(&server.uri())
    };
    let body = if status == 200 {
        json!({"data": {}, "number": 1, "html_url": "x", "state": "open"}).to_string()
    } else {
        json!({"message": "Bad credentials"}).to_string()
    };
    rt.block_on(
        Mock::given(any())
            .respond_with(
                ResponseTemplate::new(status.max(200))
                    .set_body_string(body)
                    .insert_header("etag", "\"v1\""),
            )
            .mount(&server),
    );
    let resolver = Resolver::new(Kind::GitHub, "github.com")
        .at_root(root)
        .etags_in(&run.join("config"))
        .signed_in(
            &HostScope::for_a_test(),
            account(),
            SignIn {
                tokens: Arc::new(Canary),
                imported_from_cli: false,
            },
        );
    let backend = Forge::default_of(Kind::GitHub).backend_for(Arc::new(resolver));
    let me = Caller::window().as_account(account());
    let pr = Pr {
        number: 1,
        url: "x".into(),
    };
    let repo = RepoRecord {
        path_with_namespace: "o/r".into(),
        default_branch: Some("main".into()),
        ..bare_repo("r", Kind::GitHub)
    };
    let sha = "6dcb09b5b57875f334f61aebed695e2e4193db5e";
    let said = [
        format!("{:?}", backend.owned(&me, &Owner::new("o"))),
        format!("{:?}", backend.reachable(&me, &Owner::new("o"))),
        format!("{:?}", backend.top_level(&me, &repo, None)),
        format!(
            "{:?}",
            backend.open_or_update(&me, "o/r", "charter/x", "main", "t", "b")
        ),
        format!("{:?}", backend.state(&me, "o/r", &pr)),
        format!("{:?}", backend.state(&me, "o/r", &pr)),
        format!("{:?}", backend.by_head(&me, "o/r", "charter/x")),
        format!("{:?}", backend.request_auto_merge(&me, "o/r", &pr, sha)),
        format!("{:?}", backend.checks_at(&me, "o/r", sha, 1)),
        format!("{:?}", backend.open_on_branch(&me, "o/r", "charter/x")),
        format!("{:?}", backend.ci_word(&me, "o/r", "charter/x")),
        format!(
            "{:?}",
            backend.support(&me, &Reach::Instance, Capability::Boards)
        ),
    ];
    for one in said {
        tracing::warn!(target: "charter::forge", "a forge operation answered {one}");
        println!("{one}");
    }
    let sent = rt.block_on(server.received_requests()).unwrap_or_default();
    let with_token = sent.iter().any(|r| {
        r.headers.get("authorization").and_then(|v| v.to_str().ok())
            == Some(&format!("Bearer {CANARY}"))
    });
    if with_token {
        println!("CHILD-SENT-THE-TOKEN {status}");
    }
}

/// Every `log` record, at every level, appended to one file.
struct Recorder(std::sync::Mutex<std::fs::File>);

impl log::Log for Recorder {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        use std::io::Write;
        if let Ok(mut file) = self.0.lock() {
            let _ = writeln!(
                file,
                "{} {} {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }
    fn flush(&self) {}
}

/// Every file under `dir`, recursively.
fn files_under(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

#[test]
fn a_forge_token_appears_in_no_log_record_or_crash_report() {
    charter_core::unsteered!();
    if let Some(run) = std::env::var_os(RUN).filter(|_| std::env::var_os(CHILD).is_some()) {
        let run = Path::new(&run);
        charter_core::applog::install();
        let trace = std::fs::File::create(run.join("trace.log")).unwrap();
        log::set_boxed_logger(Box::new(Recorder(std::sync::Mutex::new(trace)))).unwrap();
        log::set_max_level(log::LevelFilter::Trace);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        every_operation(&rt, run, 200);
        every_operation(&rt, run, 401);
        every_operation(&rt, run, 0);
        // The crash: a thread holding the token panics, then the main thread does.
        let held = Canary.token().unwrap();
        let _ = std::thread::spawn(move || {
            let _keep = held.expose_secret().len();
            panic!("a forced crash on a thread holding the forge token");
        })
        .join();
        panic!("a forced crash after every forge operation");
    }
    let run = tempfile::tempdir().unwrap();
    let logs = run.path().join("logs");
    let out = charter_core::forklock::output(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "a_forge_token_appears_in_no_log_record_or_crash_report",
                "--nocapture",
            ])
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", run.path())
            // The run's own tree is the fence, so the network log under `HOME` is written: an
            // emptied environment has no `TMPDIR`, and the default fence would miss it.
            .env(charter_core::fence::VAR, run.path())
            .env("RUST_BACKTRACE", "1")
            .env("RUST_LOG", "trace")
            .env("CHARTER_LOG_DIR", &logs)
            .env(CHILD, "1")
            .env(RUN, run.path()),
    )
    .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "the child was meant to crash");
    assert!(
        stderr.contains("a forced crash after every forge operation"),
        "the child did not get as far as its crash:\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("CHILD-SENT-THE-TOKEN 200") && stdout.contains("CHILD-SENT-THE-TOKEN 401"),
        "the recorded forge was never sent the token, so the run proves nothing:\n{stdout}"
    );
    let written = files_under(run.path());
    let log_text: String = written
        .iter()
        .filter(|p| p.starts_with(&logs))
        .map(|p| std::fs::read_to_string(p).unwrap_or_default())
        .collect();
    assert!(
        log_text.contains("a forge operation answered"),
        "the log holds no record of the run:\n{log_text}"
    );
    let network_log = charter_core::netlog::dir(&run.path().join(".config"));
    assert!(
        written.iter().any(|p| p.starts_with(&network_log)),
        "the network log (OB-15) listed none of the native requests, so it was not searched"
    );
    assert!(
        written
            .iter()
            .any(|p| p.starts_with(run.path().join("config"))),
        "the ETag store wrote nothing, so it was not searched"
    );
    for file in &written {
        let bytes = std::fs::read(file).unwrap();
        assert!(
            !String::from_utf8_lossy(&bytes).contains(CANARY),
            "{} holds the forge token",
            file.display()
        );
    }
    assert!(
        !stdout.contains(CANARY),
        "standard output holds the forge token"
    );
    assert!(
        !stderr.contains(CANARY),
        "the crash report holds the forge token"
    );
}
