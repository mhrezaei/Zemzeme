<div align="center">

<img src="src-tauri/icons/128x128@2x.png" width="128" alt="Zemzeme icon">

# Zemzeme

**Press a shortcut, speak, and your words are typed where the cursor is.**

[![CI](https://github.com/mhrezaei/Zemzeme/actions/workflows/ci.yml/badge.svg)](https://github.com/mhrezaei/Zemzeme/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/mhrezaei/Zemzeme)](https://github.com/mhrezaei/Zemzeme/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

English · [فارسی](README.fa.md)

</div>

Zemzeme (زمزمه, Persian for *whisper*) is a small desktop dictation app. It records from your microphone, sends the audio to a speech-to-text model through [OpenRouter](https://openrouter.ai), and pastes the text into whatever app you're typing in: an editor, a browser, a chat, a terminal.

It was built for Persian, where people mix in a lot of English words, but it works with any language the model supports.

## Features

- **Works in any app.** The text is pasted at the cursor, and your clipboard is restored afterwards.
- **One shortcut.** `Super+Alt+Space` starts recording, and the same keys stop it. A small widget at the bottom of the screen shows the level, a timer, and stop / cancel buttons.
- **English terms in English letters (optional).** A fast LLM pass turns "کامیت کردم روی داکر" into "commit کردم روی Docker" without rewriting anything else.
- **Your vocabulary.** Add names and jargon so they're spelled the way you want.
- **Pick your model.** `openai/gpt-4o-transcribe` by default, or any speech-to-text model on OpenRouter.
- **Private by design.**
  - Audio stays in memory and is never written to disk.
  - The API key is kept in the system keyring.
  - The log file never contains what you said.
- **Ignores silence and noise.** Clips with no speech aren't sent, so the model doesn't invent text from background noise.
- **Small and fast.** Built with Rust and Tauri: a ~12 MB binary that idles in the tray.

## Requirements

- Linux with GNOME. Tested on Ubuntu 26.04, Wayland. Other desktops work, but you'll have to add the shortcut yourself (see [Other desktops](#other-desktops)).
- PipeWire, the default audio system on current Ubuntu.
- An [OpenRouter API key](https://openrouter.ai/keys) with a little credit. A typical message costs **$0.0005–0.001**.

macOS and Windows aren't supported yet. The code is mostly cross-platform, so contributions are welcome.

## Install

Download the `.deb` from the [latest release](https://github.com/mhrezaei/Zemzeme/releases/latest) and install it:

```bash
sudo apt install ./Zemzeme_*_amd64.deb
```

Then open **Zemzeme** from the app menu. On first start it:

- adds the `Super+Alt+Space` shortcut,
- sets itself to start at login,
- puts an icon in the tray.

Click the tray icon, paste your OpenRouter key, and you're ready.

> The package also installs a udev rule that lets the app press the paste keys through a virtual keyboard (`/dev/uinput`). If pasting doesn't work right after installing, log out and back in once.

## Usage

1. Click where you want to type.
2. Press `Super+Alt+Space` and speak.
3. Press `Super+Alt+Space` again. The text appears a couple of seconds later.

Press **✕** in the widget to cancel. If the app can't paste (for example, the virtual keyboard isn't allowed), the text stays on the clipboard and the widget says *Copied — press Ctrl+V*.

### Settings

| Section | What you can change |
|---|---|
| API key | Save, test (shows your OpenRouter usage), remove |
| Speech recognition | Model and language (empty = auto-detect) |
| Text cleanup | English terms in English letters on/off, cleanup model, your vocabulary |
| Microphone | Which input to use, with a live level test |
| Paste | `Ctrl+V`, `Ctrl+Shift+V` (some terminals), or `Shift+Insert` |
| Shortcut | Pick or record the keys; start at login |
| Advanced | Sounds, keep recordings for troubleshooting |

### Command line

The running app also takes commands, which is handy for other desktops or scripts:

```bash
zemzeme toggle     # start / stop recording
zemzeme cancel     # cancel the current recording
zemzeme settings   # open settings
zemzeme mics       # list microphones
zemzeme quit
```

### Other desktops

On KDE, Sway, Hyprland and others, bind any key to `zemzeme toggle` in your desktop's keyboard settings.

## Privacy

- Audio is recorded into memory and sent to OpenRouter over HTTPS. It is deleted once the text comes back. Under *Advanced → Keep recordings* you can choose to save it for troubleshooting.
- OpenRouter forwards the audio to the model's provider (for example OpenAI). Their data policies apply; see [OpenRouter's privacy settings](https://openrouter.ai/settings/privacy).
- The API key is stored in the system keyring (GNOME Keyring), not in a file.
- The log (`~/.local/state/zemzeme/zemzeme.log`) records timings and errors only, never transcripts.

## Files

| What | Where |
|---|---|
| Settings | `~/.config/zemzeme/settings.json` |
| API key | System keyring, service `zemzeme` |
| Log | `~/.local/state/zemzeme/zemzeme.log` |

## Troubleshooting

- **Nothing is pasted.** Log out and back in after installing. If you built from source, run `sudo ./scripts/setup-linux.sh` once.
- **"Microphone is silent".** Pick another microphone in settings and use **Test**. Bluetooth headsets often don't record on Linux: their microphone needs the HFP profile, which some adapters don't support.
- **Text in a terminal doesn't appear.** Set *Paste* to `Ctrl+Shift+V`.
- **Shortcut does nothing.** Check *Settings → Keyboard → Custom Shortcuts* in GNOME. The entry is called *Zemzeme*.

## Build from source

You need Rust (stable), Node.js 22 or newer, and Tauri's system packages:

```bash
sudo apt install build-essential libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev libasound2-dev libdbus-1-dev libssl-dev
```

```bash
git clone https://github.com/mhrezaei/Zemzeme.git && cd Zemzeme
npm install
sudo ./scripts/setup-linux.sh   # once: allow the virtual keyboard
./scripts/install-linux.sh      # build and install to ~/.local/bin (no sudo)
```

`./scripts/install-linux.sh --uninstall` removes it. To build the `.deb` instead, run `npm run tauri build`.

### Development

```bash
npm run tauri dev
```

- In dev builds, settings are seeded from `.env` on first run (see [.env.example](.env.example)).
- Run the tests with `cargo test` in `src-tauri`.
- Small tools for testing are in `src-tauri/examples`:

  ```bash
  cargo run --example record -- 5            # record 5 s, print levels
  cargo run --example transcribe -- last.wav # speech-to-text + cleanup
  cargo run --example chime
  ```

### How it works

```
shortcut → GNOME runs `zemzeme toggle` → running instance (single-instance)
  → record (cpal, 16 kHz mono, in memory) → skip if no speech
  → OpenRouter /audio/transcriptions → optional LLM cleanup
  → clipboard + virtual Ctrl+V (uinput) → restore clipboard
```

| Part | File |
|---|---|
| Recording, microphones, speech detection | `src-tauri/src/audio.rs` |
| Speech-to-text | `src-tauri/src/stt.rs` |
| English-terms cleanup | `src-tauri/src/pipeline.rs` |
| Paste | `src-tauri/src/inject.rs` |
| Shortcut, autostart | `src-tauri/src/system.rs` |
| Widget and settings UI (Svelte) | `src/routes/` |

## Roadmap

- Translation mode (speak Persian, paste English)
- Stop automatically after silence
- History of recent transcripts
- macOS and Windows

## License

[MIT](LICENSE)
