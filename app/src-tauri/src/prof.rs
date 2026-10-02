//! THROWAWAY PROFILING (#891). Not for merge.
use std::io::Write;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn epoch_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

pub fn path() -> std::path::PathBuf {
    std::env::temp_dir().join("charter-ipc-prof.jsonl")
}

pub fn line(text: String) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path())
    {
        let _ = writeln!(f, "{text}");
    }
}

pub fn command(name: &str, started: f64, took: Duration) {
    let main = std::thread::current().name() == Some("main");
    line(format!(
        "{{\"k\":\"cmd\",\"c\":\"{name}\",\"main\":{main},\"at\":{started:.1},\"ms\":{:.2}}}",
        took.as_secs_f64() * 1000.0
    ));
}

pub fn now() -> f64 {
    epoch_ms()
}

/// Every 4 ms, how long the main thread took to run an empty closure; stalls over 8 ms are
/// written down with when they started.
pub fn probe<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    let _ = std::fs::remove_file(path());
    std::thread::spawn(move || loop {
        let sent = Instant::now();
        let at = epoch_ms();
        let (tx, rx) = mpsc::channel();
        if app
            .run_on_main_thread(move || {
                let _ = tx.send(sent.elapsed());
            })
            .is_err()
        {
            return;
        }
        if let Ok(lat) = rx.recv() {
            if lat > Duration::from_millis(8) {
                line(format!(
                    "{{\"k\":\"stall\",\"at\":{at:.1},\"ms\":{:.1}}}",
                    lat.as_secs_f64() * 1000.0
                ));
            }
        }
        std::thread::sleep(Duration::from_millis(4));
    });
}
