//! Log to stderr and to `~/.local/state/zemzeme/zemzeme.log`.
//!
//! The file never contains what the user said: transcripts are passed to
//! `private`, which only reaches stderr in development builds.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// The file is cut to empty past this size; it only needs recent history.
const MAX_BYTES: u64 = 1024 * 1024;

static LOCK: Mutex<()> = Mutex::new(());

pub fn path() -> PathBuf {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("zemzeme")
        .join("zemzeme.log")
}

fn write(level: &str, msg: &str) {
    eprintln!("zemzeme: {msg}");
    let _guard = LOCK.lock().unwrap();
    let p = path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let truncate = std::fs::metadata(&p).map(|m| m.len() > MAX_BYTES).unwrap_or(false);
    let file = OpenOptions::new().create(true).append(!truncate).write(true).truncate(truncate).open(&p);
    if let Ok(mut f) = file {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "{secs} {level} {msg}");
    }
}

pub fn info(msg: &str) {
    write("INFO", msg);
}

pub fn warn(msg: &str) {
    write("WARN", msg);
}

/// User content (transcripts): stderr in dev builds only, never the file.
pub fn private(msg: &str) {
    if cfg!(debug_assertions) {
        eprintln!("zemzeme: {msg}");
    }
}
