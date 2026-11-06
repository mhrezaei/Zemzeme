//! The app's recording state machine:
//! Idle → Recording → Transcribing → Pasting → Idle, with Cancel from any busy state.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::audio::{Recorder, Recording};
use crate::config::Config;
use crate::inject::{Injector, Outcome};
use crate::overlay;
use crate::pipeline;
use crate::sound::{self, Chime};
use crate::stt::{OpenRouter, SpeechToText};

/// Recordings shorter than this are treated as accidental presses.
const MIN_SECONDS: f32 = 0.3;
/// Clips with less speech than this are not sent (models hallucinate on noise).
const MIN_SPEECH_SECONDS: f32 = 0.25;
/// How long an error stays on screen before the overlay hides.
const ERROR_HIDE_AFTER: Duration = Duration::from_secs(3);

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
    chime(Chime::Error);
    crate::log::warn(&message);
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

    let mic = Config::load().microphone;
    let level_app = app.clone();
    match Recorder::start(Some(mic.as_str()), move |level| {
        let _ = level_app.emit("level", level);
    }) {
        Ok(rec) => {
            chime(Chime::Start);
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
    chime(Chime::Stop);

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
        if recording.speech_seconds() < MIN_SPEECH_SECONDS {
            return fail(&app, session, "No speech detected".into());
        }
        let text = match transcribe(&app, &recording) {
            Ok(t) => t,
            Err(e) => return fail(&app, session, e),
        };
        // The user may have cancelled while the request was in flight.
        if !app.state::<AppState>().is_current(session) {
            return;
        }
        if text.is_empty() {
            return fail(&app, session, "No speech detected".into());
        }

        set_phase(&app, Phase::Pasting, None);
        let key = Config::load().paste_key;
        match app.state::<Injector>().paste(&text, key) {
            Ok(Outcome::Pasted) => {
                set_phase(&app, Phase::Idle, None);
                overlay::hide(&app);
            }
            Ok(Outcome::CopiedOnly(_)) => fail(&app, session, "Copied — press Ctrl+V".into()),
            Err(e) => fail(&app, session, e),
        }
    });
}

fn transcribe(app: &AppHandle, recording: &Recording) -> Result<String, String> {
    let wav = recording.to_wav();
    let cfg = Config::load();
    if cfg.keep_audio {
        keep_recording(app, &wav);
    } else if cfg!(debug_assertions) {
        save_debug_copy(app, &wav);
    }

    let lang = Some(cfg.language.clone()).filter(|l| !l.is_empty());
    let stt = OpenRouter::new(cfg.api_key.clone(), cfg.model.clone(), lang).with_prompt(cfg.stt_prompt());

    let started = Instant::now();
    let result = stt.transcribe(&wav, "wav")?;
    // Success: no reason to keep the audio on disk.
    if cfg!(debug_assertions) && !cfg.keep_audio {
        delete_debug_copy(app);
    }
    crate::log::info(&format!(
        "{:.1}s audio → {} in {:.1}s, cost {}, {} chars",
        recording.duration_secs(),
        cfg.model,
        started.elapsed().as_secs_f32(),
        result.cost.map(|c| format!("${c:.5}")).unwrap_or_else(|| "?".into()),
        result.text.chars().count()
    ));
    crate::log::private(&format!("transcript: {}", result.text));
    let text = pipeline::process(&result.text, &cfg);
    if cfg.cleanup {
        crate::log::private(&format!("after cleanup: {text}"));
    }
    Ok(text)
}

/// Dev builds only: the last recording stays on disk only if transcription
/// failed, so it can be inspected. Release builds never write audio to disk.
fn delete_debug_copy(app: &AppHandle) {
    if let Ok(dir) = app.path().app_cache_dir() {
        let _ = std::fs::remove_file(dir.join("last.wav"));
    }
}

/// Troubleshooting option: every recording saved as recordings/<unix-time>.wav.
fn keep_recording(app: &AppHandle, wav: &[u8]) {
    let Ok(dir) = app.path().app_cache_dir().map(|d| d.join("recordings")) else { return };
    let _ = std::fs::create_dir_all(&dir);
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let path = dir.join(format!("{secs}.wav"));
    if std::fs::write(&path, wav).is_ok() {
        crate::log::info(&format!("kept recording {}", path.display()));
    }
}

fn save_debug_copy(app: &AppHandle, wav: &[u8]) {
    if let Ok(dir) = app.path().app_cache_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("last.wav"), wav);
    }
}

fn chime(c: Chime) {
    if Config::load().sounds {
        sound::play(c);
    }
}
