//! No harness found (FR-29, W10): each harness's official installer, and a local model the
//! machine already serves.
//!
//! W10 names three things for a machine with no harness: **official installers in a shell
//! tab, the harness's own login, and a local model fallback.** This module holds the data the
//! first two need and the detection the third needs. Nothing here runs an installer or signs
//! anybody in: the window types the installer into a shell tab the operator asked for, and a
//! harness's login is the harness's own first screen.
//!
//! # The installers are the vendors' own, word for word
//!
//! Each command is the one its vendor's install page gives for macOS and Linux, copied rather
//! than composed, and the page it came from is kept beside it so the screen can link to it.
//! charter never ships, patches or wraps a harness binary (ADR 0073, ADR 0080): the installer
//! is the vendor's, run in the operator's shell with the operator's authority, after a press
//! that showed the exact command.
//!
//! **Each one installs where charter already looks** ([`crate::programs::USER_BIN`]), so the
//! harness is found by the next look, with no restart and no `PATH` edit. A test holds every
//! installer to that.
//!
//! # A local model is detected, never assumed
//!
//! ADR 0087 §8 (MS-17): an endpoint is detected, never assumed. [`local_models`] asks each
//! well-known loopback endpoint one HTTP question and counts it only when it answers `200`.
//! Nothing leaves the machine, and nothing is sent but a `GET` of the endpoint's model list.
//! Pointing a harness at the endpoint is MS-17's adapter; here the screen only says that one
//! is there and which harness can use it.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use crate::harness::Harness;

/// A harness's official installer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Installer {
    /// The command, as the vendor's install page gives it for macOS and Linux.
    pub command: &'static str,
    /// The vendor's install page, where the command is from.
    pub page: &'static str,
    /// Where the installer puts the program, under the home directory.
    pub lands_in: &'static str,
}

/// `harness`'s official installer.
pub fn installer(harness: Harness) -> Installer {
    match harness {
        // https://code.claude.com/docs/en/setup — "Native Install (Recommended)", whose
        // launcher is `~/.local/bin/claude`.
        Harness::ClaudeCode => Installer {
            command: "curl -fsSL https://claude.ai/install.sh | bash",
            page: "https://code.claude.com/docs/en/setup",
            lands_in: ".local/bin",
        },
        // https://github.com/openai/codex — "Mac/Linux"; the script's `BIN_DIR` defaults to
        // `$HOME/.local/bin`.
        Harness::Codex => Installer {
            command: "curl -fsSL https://chatgpt.com/codex/install.sh | sh",
            page: "https://github.com/openai/codex",
            lands_in: ".local/bin",
        },
        // https://opencode.ai/docs/ — "the install script"; its `INSTALL_DIR` is
        // `$HOME/.opencode/bin`.
        Harness::Opencode => Installer {
            command: "curl -fsSL https://opencode.ai/install | bash",
            page: "https://opencode.ai/docs/",
            lands_in: ".opencode/bin",
        },
    }
}

/// Whether no harness charter starts is installed: what turns the first chat into the
/// harness setup tab (FR-29) rather than the picker.
pub fn none_installed(found: &[crate::firstrun::HarnessFound]) -> bool {
    found.iter().all(|one| one.program.is_none())
}

/// A local model server charter knows to look for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalServer {
    /// What the screen calls it.
    pub title: &'static str,
    /// Its loopback address, on the port it listens on by default.
    pub addr: &'static str,
    /// The path whose `200` says it is there: its own list of models.
    pub path: &'static str,
    /// The base URL a harness is pointed at.
    pub base_url: &'static str,
}

/// The local model servers charter looks for, in the order the screen lists them: Ollama and
/// LM Studio, each on its default loopback port. vLLM's default port (8000) is every other
/// development server's too, so a `200` there would say nothing; it waits for MS-17, where
/// the operator names an endpoint.
pub const LOCAL_SERVERS: [LocalServer; 2] = [
    LocalServer {
        title: "Ollama",
        addr: "127.0.0.1:11434",
        path: "/api/tags",
        base_url: "http://127.0.0.1:11434/v1",
    },
    LocalServer {
        title: "LM Studio",
        addr: "127.0.0.1:1234",
        path: "/v1/models",
        base_url: "http://127.0.0.1:1234/v1",
    },
];

/// The harness that can run on a local model with no account: opencode, whose config takes an
/// OpenAI-compatible provider (its providers page, "Ollama" and "LM Studio").
pub const LOCAL_MODEL_HARNESS: Harness = Harness::Opencode;

/// The servers in [`LOCAL_SERVERS`] that `answers` says are there.
pub fn local_models(answers: &dyn Fn(&LocalServer) -> bool) -> Vec<LocalServer> {
    LOCAL_SERVERS
        .iter()
        .filter(|server| answers(server))
        .copied()
        .collect()
}

/// [`local_models`] for this machine: each server asked once, with a short timeout.
pub fn local_models_here() -> Vec<LocalServer> {
    local_models(&|server| {
        server
            .addr
            .parse()
            .is_ok_and(|addr| answers_200(addr, server.path, PROBE_TIMEOUT))
    })
}

/// How long one question to a local server may take, **in all**: connecting, asking and the
/// whole status line together. A server on loopback answers in milliseconds; a port nobody
/// listens on is refused at once.
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(400);

/// Whether an HTTP server at `addr` answers `GET path` with `200`, within `timeout` in all.
///
/// One `GET`, HTTP/1.0 so the server closes the connection after it, and only the status line
/// of the answer is read. Anything that is not `HTTP/1.x 200` — a refused connection, a
/// timeout, another status, something that is not HTTP — is `false`.
///
/// **`timeout` is one deadline for the whole probe**, not a timeout per call: every socket
/// operation is given only the time left before it, so a server that drips its answer a byte
/// at a time is cut off at the deadline like one that says nothing.
pub fn answers_200(addr: SocketAddr, path: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    // The time left, or `None` once it is up: a zero timeout is an error to the socket calls.
    let left = || {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
    };
    let Some(first) = left() else {
        return false;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, first) else {
        return false;
    };
    let asked = format!("GET {path} HTTP/1.0\r\nHost: {addr}\r\nAccept: */*\r\n\r\n");
    let Some(now) = left() else {
        return false;
    };
    if stream.set_write_timeout(Some(now)).is_err() || stream.write_all(asked.as_bytes()).is_err() {
        return false;
    }
    // The status line is the first dozen bytes: `HTTP/1.1 200`.
    let mut head = [0u8; 12];
    let mut got = 0;
    while got < head.len() {
        let Some(now) = left() else {
            return false;
        };
        if stream.set_read_timeout(Some(now)).is_err() {
            return false;
        }
        match stream.read(&mut head[got..]) {
            Ok(0) | Err(_) => return false,
            Ok(n) => got += n,
        }
    }
    head.starts_with(b"HTTP/1.") && &head[8..] == b" 200"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::path::Path;

    #[test]
    fn each_harness_installs_with_its_vendors_own_command() {
        // The vendors' install pages, word for word (fetched 2026-10-02).
        assert_eq!(
            installer(Harness::ClaudeCode).command,
            "curl -fsSL https://claude.ai/install.sh | bash"
        );
        assert_eq!(
            installer(Harness::Codex).command,
            "curl -fsSL https://chatgpt.com/codex/install.sh | sh"
        );
        assert_eq!(
            installer(Harness::Opencode).command,
            "curl -fsSL https://opencode.ai/install | bash"
        );
    }

    #[test]
    fn every_installer_puts_its_harness_where_charter_looks() {
        let home = Path::new("/home/someone");
        let looked = crate::programs::search_dirs_from(None, Some(home));
        for harness in Harness::ALL {
            let lands = home.join(installer(harness).lands_in);
            assert!(
                looked.contains(&lands),
                "{} installs into {}, which charter does not search",
                harness.name(),
                lands.display()
            );
        }
    }

    fn found(harness: Harness, installed: bool) -> crate::firstrun::HarnessFound {
        crate::firstrun::HarnessFound {
            harness,
            program: installed.then(|| std::path::PathBuf::from("/bin/x")),
            signed_in: false,
        }
    }

    #[test]
    fn a_machine_with_one_harness_installed_has_a_harness_even_signed_out() {
        assert!(none_installed(&[
            found(Harness::ClaudeCode, false),
            found(Harness::Codex, false),
            found(Harness::Opencode, false),
        ]));
        assert!(!none_installed(&[
            found(Harness::ClaudeCode, false),
            found(Harness::Codex, true),
        ]));
    }

    #[test]
    fn a_local_server_counts_only_when_it_answers() {
        let found = local_models(&|server| server.title == "LM Studio");

        assert_eq!(
            found.iter().map(|one| one.title).collect::<Vec<_>>(),
            ["LM Studio"]
        );
        assert!(local_models(&|_| false).is_empty());
    }

    /// A one-shot HTTP server on loopback that answers its first request with `status`.
    fn serving(status: &'static str) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let addr = listener.local_addr().expect("its address");
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut asked = [0u8; 1024];
                let _ = stream.read(&mut asked);
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
                );
            }
        });
        addr
    }

    #[test]
    fn a_server_that_answers_200_is_there() {
        let addr = serving("200 OK");

        assert!(answers_200(addr, "/api/tags", Duration::from_secs(2)));
    }

    #[test]
    fn a_server_that_answers_anything_else_is_not() {
        let addr = serving("404 Not Found");

        assert!(!answers_200(addr, "/api/tags", Duration::from_secs(2)));
    }

    #[test]
    fn a_server_that_drips_its_answer_is_cut_off_at_the_timeout() {
        // One byte every 100 ms: each read is well inside any per-read timeout, and the whole
        // status line takes 1.2 s. The probe's timeout bounds all of it, not each read.
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let addr = listener.local_addr().expect("its address");
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut asked = [0u8; 1024];
                let _ = stream.read(&mut asked);
                for byte in b"HTTP/1.1 200 OK\r\n\r\n" {
                    if stream.write_all(&[*byte]).is_err() {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        });

        let started = std::time::Instant::now();
        let there = answers_200(addr, "/api/tags", Duration::from_millis(300));

        assert!(
            !there,
            "a status line slower than the timeout is not an answer"
        );
        assert!(
            started.elapsed() < Duration::from_millis(700),
            "the probe took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_port_nobody_listens_on_is_not_a_server() {
        // Bound, then let go: nothing listens there by the time it is asked.
        let addr = TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("a free port");

        assert!(!answers_200(addr, "/api/tags", Duration::from_secs(2)));
    }
}
