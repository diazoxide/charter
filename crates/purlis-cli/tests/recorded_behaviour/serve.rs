//! What a scenario needs RUNNING while its command runs, which no file can carry.
//!
//! Two kinds, each a stand-in for the desktop app's half of a conversation on its hook socket.
//! Both bind the socket, answer a ticket ask with a ticket, answer the ask that spends it, and
//! are gone. They check nothing, because what is under test is the COMMAND's side; the app's
//! half is `app/src-tauri/src/handoff.rs` and its own tests.
//!
//! - `an-app-that-opens` (charter-app#204) answers the spending ask with "opened as chat 9":
//!   the differential's `_AN_APP_THAT_OPENS`, answer for answer.
//! - `an-app-that-answers` (#1436) answers it with the scenario's own `answer`, one JSON value
//!   written as one line: what a dispatch is told, whichever of the app's answers that is.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

pub struct Served {
    socket: PathBuf,
    thread: JoinHandle<()>,
}

pub fn start(serve: &Map<String, Value>, side: &Path) -> Served {
    let kind = serve["kind"].as_str().expect("a kind");
    let answers = match kind {
        "an-app-that-opens" => "{\"opened\": {\"chat\": 9}}\n".to_owned(),
        "an-app-that-answers" => {
            let answer = serve.get("answer").expect("an answer");
            format!("{}\n", serde_json::to_string(answer).expect("one line"))
        }
        _ => panic!("no stand-in named {kind:?}"),
    };
    let socket = side.join(serve["socket"].as_str().expect("a socket"));
    std::fs::create_dir_all(socket.parent().expect("a parent")).expect("the socket's directory");
    let listener = UnixListener::bind(&socket).expect("the stand-in binds");
    listener.set_nonblocking(true).expect("non-blocking");
    let thread = std::thread::spawn(move || {
        // Thirty seconds, as the stand-in it replaces: a command that never connects does not
        // leave it waiting for ever.
        let deadline = Instant::now() + Duration::from_secs(30);
        let connection = loop {
            match listener.accept() {
                Ok((c, _)) => break Some(c),
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break None,
            }
        };
        let Some(connection) = connection else { return };
        connection.set_nonblocking(false).expect("blocking");
        connection
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("a timeout");
        let mut writer = connection.try_clone().expect("a second handle");
        for line in BufReader::new(connection).lines() {
            let Ok(line) = line else { break };
            let Ok(ask) = serde_json::from_str::<Value>(&line) else {
                break;
            };
            let answer = if ask.get("ticket").is_some() {
                format!("{{\"ticket\": {{\"ticket\": \"{}\"}}}}\n", "0".repeat(64))
            } else {
                answers.clone()
            };
            if writer
                .write_all(answer.as_bytes())
                .and_then(|()| writer.flush())
                .is_err()
            {
                break;
            }
        }
    });
    Served { socket, thread }
}

impl Served {
    /// Wait for the stand-in to be done with its one connection, and take its socket away, as
    /// the stand-in it replaces did on its way out.
    pub fn finish(self) {
        let _ = self.thread.join();
        let _ = std::fs::remove_file(&self.socket);
    }
}
