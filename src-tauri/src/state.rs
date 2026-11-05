//! The app's recording state machine:
//! Idle → Recording → Transcribing → Pasting → Idle, with Cancel from any busy state.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::audio::{Recorder, Recording};
use crate::overlay;

/// Recordings shorter than this are treated as accidental presses.
const MIN_SECONDS: f32 = 0.3;
/// How long an error stays on screen before the overlay hides.
const ERROR_HIDE_AFTER: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Idle,
    Recording,
    Transcribing,
    #[allow(dead_code)] // used from phase 5 (paste)
    Pasting,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatePayload {
    pub phase: Phase,
    pub message: Option<String>,
}

pub struct AppState {
    phase: Mutex<Phase>,
    recorder: Mutex<Option<Recorder>>,
    /// Bumped on every start/cancel so a stale background job can tell it was superseded.
    session: Mutex<u64>,
}

impl AppState {
    pub fn new() -> Self {
        Self { phase: Mutex::new(Phase::Idle), recorder: Mutex::new(None), session: Mutex::new(0) }
    }

    pub fn phase(&self) -> Phase {
        *self.phase.lock().unwrap()
    }

    fn next_session(&self) -> u64 {
        let mut s = self.session.lock().unwrap();
        *s += 1;
        *s
    }

    fn is_current(&self, session: u64) -> bool {
        *self.session.lock().unwrap() == session
    }
}

fn set_phase(app: &AppHandle, phase: Phase, message: Option<String>) {
    *app.state::<AppState>().phase.lock().unwrap() = phase;
    let _ = app.emit("state", StatePayload { phase, message });
}

/// Start recording when idle, stop when recording. Ignored while busy.
pub fn toggle(app: &AppHandle) {
    match app.state::<AppState>().phase() {
        Phase::Idle | Phase::Error => start(app),
        Phase::Recording => stop(app),
        Phase::Transcribing | Phase::Pasting => {}
    }
}

pub fn cancel(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.next_session();
    drop(state.recorder.lock().unwrap().take());
    set_phase(app, Phase::Idle, None);
    overlay::hide(app);
}

fn fail(app: &AppHandle, session: u64, message: String) {
    eprintln!("my-whisper: {message}");
    set_phase(app, Phase::Error, Some(message));
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(ERROR_HIDE_AFTER);
        let state = app.state::<AppState>();
        if state.is_current(session) && state.phase() == Phase::Error {
            set_phase(&app, Phase::Idle, None);
            overlay::hide(&app);
        }
    });
}

fn start(app: &AppHandle) {
    let state = app.state::<AppState>();
    let session = state.next_session();
    overlay::show(app);

    // Phase 6 moves this into settings.
    let mic = std::env::var("MY_WHISPER_MIC").ok();
    let level_app = app.clone();
    match Recorder::start(mic.as_deref(), move |level| {
        let _ = level_app.emit("level", level);
    }) {
        Ok(rec) => {
            *state.recorder.lock().unwrap() = Some(rec);
            set_phase(app, Phase::Recording, None);
        }
        Err(e) => fail(app, session, e),
    }
}

fn stop(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(recorder) = state.recorder.lock().unwrap().take() else { return };
    let session = *state.session.lock().unwrap();
    set_phase(app, Phase::Transcribing, None);

    let app = app.clone();
    std::thread::spawn(move || {
        let recording = match recorder.stop() {
            Ok(r) => r,
            Err(e) => return fail(&app, session, e),
        };
        if recording.duration_secs() < MIN_SECONDS {
            return fail(&app, session, "Too short".into());
        }
        // Exact digital silence means no signal at all (muted mic, or a
        // Bluetooth headset in playback-only mode), not just a quiet room.
        if recording.is_silent() {
            return fail(&app, session, "Microphone is silent".into());
        }
        if let Err(e) = process(&app, &recording) {
            return fail(&app, session, e);
        }
        if app.state::<AppState>().is_current(session) {
            set_phase(&app, Phase::Idle, None);
            overlay::hide(&app);
        }
    });
}

/// Phases 3–5 replace this: send to STT, then paste the text.
fn process(app: &AppHandle, recording: &Recording) -> Result<(), String> {
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("last.wav");
    let wav = recording.to_wav();
    std::fs::write(&path, &wav).map_err(|e| e.to_string())?;
    eprintln!(
        "my-whisper: recorded {:.1}s ({} KB) → {}",
        recording.duration_secs(),
        wav.len() / 1024,
        path.display()
    );
    Ok(())
}
