# Changelog

## 0.1.1

Maintenance release; the app works the same as 0.1.0.

- Every release now runs the full test suite before the `.deb` is built.
- 28 tests (was 19), including OpenRouter requests and error messages, tested against a local fake server.
- Updated SvelteKit to 3 and other dependencies.
- Added [SECURITY.md](https://github.com/mhrezaei/Zemzeme/blob/main/SECURITY.md) with how your key, audio and text are handled, and private security reporting.
- Tip in the README: set a credit limit on your OpenRouter key.

## 0.1.0

First public release.

- Shortcut (`Super+Alt+Space`) to start and stop recording, with a small on-screen widget.
- Speech-to-text through OpenRouter (default `openai/gpt-4o-transcribe`).
- Optional cleanup that writes English terms in English letters.
- Pastes at the cursor in any app and restores the clipboard.
- Settings: API key (in the system keyring), model, language, microphone, vocabulary, paste keys, shortcut, start at login.
