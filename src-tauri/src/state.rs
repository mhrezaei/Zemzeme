//! The app's recording state machine:
//! Idle → Recording → Transcribing → Pasting → Idle, with Cancel from any busy state.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::overlay;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Idle,
    Recording,
    Transcribing,
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
}

impl AppState {
    pub fn new() -> Self {
        Self { phase: Mutex::new(Phase::Idle) }
    }

    pub fn phase(&self) -> Phase {
        *self.phase.lock().unwrap()
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
    set_phase(app, Phase::Idle, None);
    overlay::hide(app);
}

fn start(app: &AppHandle) {
    overlay::show(app);
    set_phase(app, Phase::Recording, None);
    // Phase 2: start microphone capture here.
}

fn stop(app: &AppHandle) {
    set_phase(app, Phase::Transcribing, None);
    // Phases 2–5: stop capture → send to STT → paste result.
    // Placeholder so the full cycle can be tested now.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(800));
        if app.state::<AppState>().phase() == Phase::Transcribing {
            set_phase(&app, Phase::Idle, None);
            overlay::hide(&app);
        }
    });
}
