<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";

  type PasteKey = "ctrl+v" | "ctrl+shift+v" | "shift+insert";
  interface Settings {
    model: string;
    language: string;
    microphone: string;
    paste_key: PasteKey;
    cleanup: boolean;
    cleanup_model: string;
    vocabulary: string;
    keep_audio: boolean;
    sounds: boolean;
  }
  interface View extends Settings {
    has_api_key: boolean;
    api_key_hint: string;
    settings_path: string;
    log_path: string;
  }
  interface Mic { id: string; name: string; is_default: boolean }
  interface KeyInfo { usage: number; usage_daily: number; limit_remaining: number | null }
  interface MicTest { peak: number; speech_seconds: number; silent: boolean }

  const STT_MODELS = [
    ["openai/gpt-4o-transcribe", "GPT-4o Transcribe — accurate, ~2s (recommended)"],
    ["elevenlabs/scribe-v2", "ElevenLabs Scribe v2 — writes English terms itself"],
    ["mistralai/voxtral-small-transcribe-3-0", "Voxtral Small — fast"],
    ["openai/gpt-4o-mini-transcribe", "GPT-4o mini Transcribe — cheaper"],
    ["openai/whisper-large-v3-turbo", "Whisper large-v3 turbo — cheapest"],
  ];
  const CLEANUP_MODELS = [
    ["google/gemini-3.1-flash-lite", "Gemini 3.1 Flash Lite — fast, faithful (recommended)"],
    ["anthropic/claude-haiku-5.5", "Claude Haiku 5.5"],
    ["deepseek/deepseek-v4.1-flash", "DeepSeek V4.1 Flash"],
  ];
  const LANGUAGES = [
    ["fa", "Persian (فارسی)"],
    ["en", "English"],
    ["", "Auto-detect"],
    ["ar", "Arabic"],
    ["tr", "Turkish"],
    ["de", "German"],
    ["fr", "French"],
  ];
  const CUSTOM = "__custom__";
  // GNOME accelerators.
  const SHORTCUTS = [
    ["<Super><Alt>space", "Super + Alt + Space"],
    ["<Ctrl><Alt>space", "Ctrl + Alt + Space"],
    ["<Ctrl><Shift>space", "Ctrl + Shift + Space"],
    ["<Super>grave", "Super + `"],
    ["<Alt>grave", "Alt + `"],
    ["F9", "F9"],
  ];

  interface SystemView { shortcut: string | null; autostart: boolean; dev_build: boolean; exe: string }
  let sys = $state<SystemView | null>(null);
  let capturing = $state(false);
  let shortcutStatus = $state<{ ok: boolean; text: string } | null>(null);

  // Physical key (e.code) → GNOME key name. e.code ignores keyboard layout,
  // so this works while the Persian layout is active too.
  const CODE_NAMES: Record<string, string> = {
    Space: "space", Backquote: "grave", Minus: "minus", Equal: "equal", Comma: "comma",
    Period: "period", Slash: "slash", Backslash: "backslash", Semicolon: "semicolon",
    Quote: "apostrophe", BracketLeft: "bracketleft", BracketRight: "bracketright",
    Enter: "Return", Tab: "Tab", Insert: "Insert", Delete: "Delete", Home: "Home", End: "End",
    PageUp: "Page_Up", PageDown: "Page_Down", ArrowUp: "Up", ArrowDown: "Down",
    ArrowLeft: "Left", ArrowRight: "Right", Pause: "Pause", ScrollLock: "Scroll_Lock",
  };

  function accelFromEvent(e: KeyboardEvent): string | null {
    let key: string | undefined;
    if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3).toLowerCase();
    else if (/^Digit\d$/.test(e.code)) key = e.code.slice(5);
    else if (/^F\d{1,2}$/.test(e.code)) key = e.code;
    else key = CODE_NAMES[e.code];
    if (!key) return null; // modifier alone or unsupported key
    const mods = (e.metaKey ? "<Super>" : "") + (e.ctrlKey ? "<Ctrl>" : "") + (e.altKey ? "<Alt>" : "") + (e.shiftKey ? "<Shift>" : "");
    // Plain letters/space without a modifier would fire while typing.
    if (!mods && !/^F\d/.test(key) && !["Pause", "Scroll_Lock", "Insert"].includes(key)) return null;
    return mods + key;
  }

  function prettyKeys(accel: string): string[] {
    const mods = [...accel.matchAll(/<(\w+)>/g)].map((m) => m[1]);
    const key = accel.replace(/<\w+>/g, "");
    const names: Record<string, string> = { space: "Space", grave: "`", Return: "Enter" };
    return [...mods, names[key] ?? (key.length === 1 ? key.toUpperCase() : key)];
  }

  async function applyShortcut(accel: string) {
    shortcutStatus = null;
    try {
      await invoke("set_shortcut", { binding: accel });
      sys = await invoke<SystemView>("get_system");
      shortcutStatus = { ok: true, text: "Shortcut saved." };
    } catch (e) {
      shortcutStatus = { ok: false, text: String(e) };
    }
  }

  async function toggleAutostart(on: boolean) {
    try {
      await invoke("set_autostart", { on });
    } catch (e) {
      shortcutStatus = { ok: false, text: String(e) };
    }
    sys = await invoke<SystemView>("get_system");
  }

  function onCaptureKey(e: KeyboardEvent) {
    if (!capturing) return;
    e.preventDefault();
    if (e.key === "Escape") {
      capturing = false;
      return;
    }
    const accel = accelFromEvent(e);
    if (accel) {
      capturing = false;
      applyShortcut(accel);
    }
  }

  let s = $state<Settings | null>(null);
  let hasKey = $state(false);
  let keyHint = $state("");
  let settingsPath = $state("");
  let logPath = $state("");
  let mics = $state<Mic[]>([]);

  let keyInput = $state("");
  let keyStatus = $state<{ ok: boolean; text: string } | null>(null);
  let keyBusy = $state(false);

  let micLevel = $state(0);
  let micTesting = $state(false);
  let micResult = $state<string | null>(null);

  let saved = $state(false);
  let saveError = $state<string | null>(null);
  let loaded = false;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;

  // Which select shows "Custom…" (value not in the preset list).
  let sttCustom = $state(false);
  let cleanupCustom = $state(false);

  const isPreset = (list: string[][], v: string) => list.some(([id]) => id === v);

  async function load() {
    const v = await invoke<View>("get_settings");
    hasKey = v.has_api_key;
    keyHint = v.api_key_hint;
    settingsPath = v.settings_path;
    logPath = v.log_path;
    const { has_api_key, api_key_hint, settings_path, log_path, ...rest } = v;
    s = rest;
    sttCustom = !isPreset(STT_MODELS, rest.model);
    cleanupCustom = !isPreset(CLEANUP_MODELS, rest.cleanup_model);
    mics = await invoke<Mic[]>("list_microphones");
    sys = await invoke<SystemView>("get_system");
    loaded = true;
  }

  // Auto-save shortly after any change.
  $effect(() => {
    if (!s) return;
    const snapshot = JSON.stringify(s);
    if (!loaded) return;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      try {
        await invoke("save_settings", { settings: JSON.parse(snapshot) });
        saveError = null;
        saved = true;
        setTimeout(() => (saved = false), 1200);
      } catch (e) {
        saveError = String(e);
      }
    }, 400);
  });

  async function saveKey() {
    keyBusy = true;
    keyStatus = null;
    try {
      const info = await invoke<KeyInfo>("test_api_key", { key: keyInput });
      await invoke("set_api_key", { key: keyInput });
      keyInput = "";
      await refreshKey();
      keyStatus = { ok: true, text: `Saved. ${usageText(info)}` };
    } catch (e) {
      keyStatus = { ok: false, text: String(e) };
    } finally {
      keyBusy = false;
    }
  }

  async function testKey() {
    keyBusy = true;
    keyStatus = null;
    try {
      const info = await invoke<KeyInfo>("test_api_key", { key: null });
      keyStatus = { ok: true, text: `Connected. ${usageText(info)}` };
    } catch (e) {
      keyStatus = { ok: false, text: String(e) };
    } finally {
      keyBusy = false;
    }
  }

  async function removeKey() {
    await invoke("set_api_key", { key: "" });
    await refreshKey();
    keyStatus = { ok: true, text: "Key removed." };
  }

  async function refreshKey() {
    const v = await invoke<View>("get_settings");
    hasKey = v.has_api_key;
    keyHint = v.api_key_hint;
  }

  function usageText(i: KeyInfo) {
    const left = i.limit_remaining != null ? ` · $${i.limit_remaining.toFixed(2)} left` : "";
    return `Used $${i.usage.toFixed(3)} total, $${i.usage_daily.toFixed(3)} today${left}.`;
  }

  async function testMic() {
    if (!s) return;
    micTesting = true;
    micResult = null;
    try {
      const r = await invoke<MicTest>("test_microphone", { id: s.microphone, seconds: 3 });
      micResult = r.silent
        ? "No signal at all — this microphone is muted or not delivering audio."
        : r.speech_seconds > 0.25
          ? `Works — heard ${r.speech_seconds.toFixed(1)}s of speech.`
          : "Signal OK but no speech heard. Say something during the test.";
    } catch (e) {
      micResult = String(e);
    } finally {
      micTesting = false;
      micLevel = 0;
    }
  }

  onMount(() => {
    load();
    const un = listen<number>("mic-test-level", (e) => (micLevel = e.payload));
    // Refresh microphones when the window is shown again.
    const onFocus = () => invoke<Mic[]>("list_microphones").then((m) => (mics = m));
    window.addEventListener("focus", onFocus);
    window.addEventListener("keydown", onCaptureKey, true);
    return () => {
      un.then((f) => f());
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("keydown", onCaptureKey, true);
    };
  });
</script>

<main>
  <header>
    <h1>Zemzeme</h1>
    <span class="saved" class:show={saved}>Saved ✓</span>
  </header>
  {#if saveError}<p class="err">{saveError}</p>{/if}

  {#if s}
    <section>
      <h2>OpenRouter API key</h2>
      <p class="hint">
        Stored in the system keyring, not in a file.
        {#if hasKey}Current key: <code>{keyHint}</code>{:else}<b>No key set.</b>{/if}
      </p>
      <div class="row">
        <input
          type="password"
          placeholder={hasKey ? "Paste a new key to replace it" : "sk-or-v1-…"}
          bind:value={keyInput}
          autocomplete="off"
          spellcheck="false"
        />
        <button class="primary" disabled={!keyInput.trim() || keyBusy} onclick={saveKey}>Save</button>
      </div>
      <div class="row">
        <button disabled={!hasKey || keyBusy} onclick={testKey}>Test connection</button>
        {#if hasKey}<button class="ghost" onclick={removeKey}>Remove key</button>{/if}
      </div>
      {#if keyStatus}<p class={keyStatus.ok ? "ok" : "err"}>{keyStatus.text}</p>{/if}
    </section>

    <section>
      <h2>Speech recognition</h2>
      <label>
        Model
        <select
          value={sttCustom ? CUSTOM : s.model}
          onchange={(e) => {
            const v = e.currentTarget.value;
            sttCustom = v === CUSTOM;
            if (!sttCustom && s) s.model = v;
          }}
        >
          {#each STT_MODELS as [id, label]}<option value={id}>{label}</option>{/each}
          <option value={CUSTOM}>Custom…</option>
        </select>
      </label>
      {#if sttCustom}
        <input placeholder="provider/model" bind:value={s.model} spellcheck="false" />
      {/if}
      <label>
        Language
        <select bind:value={s.language}>
          {#each LANGUAGES as [id, label]}<option value={id}>{label}</option>{/each}
        </select>
      </label>
    </section>

    <section>
      <h2>Text cleanup</h2>
      <label class="switch">
        <input type="checkbox" bind:checked={s.cleanup} />
        <span>Write English terms in English letters</span>
      </label>
      <p class="hint">کامیت → commit, اسپیس → Space. Adds about 1.5s.</p>
      {#if s.cleanup}
        <label>
          Cleanup model
          <select
            value={cleanupCustom ? CUSTOM : s.cleanup_model}
            onchange={(e) => {
              const v = e.currentTarget.value;
              cleanupCustom = v === CUSTOM;
              if (!cleanupCustom && s) s.cleanup_model = v;
            }}
          >
            {#each CLEANUP_MODELS as [id, label]}<option value={id}>{label}</option>{/each}
            <option value={CUSTOM}>Custom…</option>
          </select>
        </label>
        {#if cleanupCustom}
          <input placeholder="provider/model" bind:value={s.cleanup_model} spellcheck="false" />
        {/if}
      {/if}
      <label>
        Vocabulary
        <textarea rows="3" placeholder="Names and terms to spell exactly, comma separated: Tauri, Docker, …" bind:value={s.vocabulary} spellcheck="false"></textarea>
      </label>
    </section>

    <section>
      <h2>Microphone</h2>
      <div class="row">
        <select bind:value={s.microphone}>
          <option value="">System default</option>
          {#each mics as m}
            <option value={m.id}>{m.name}{m.is_default ? " (default)" : ""}</option>
          {/each}
        </select>
        <button disabled={micTesting} onclick={testMic}>{micTesting ? "Listening…" : "Test"}</button>
      </div>
      {#if micTesting}
        <div class="meter"><span style="width:{Math.round(micLevel * 100)}%"></span></div>
        <p class="hint">Say something…</p>
      {/if}
      {#if micResult}<p class="hint">{micResult}</p>{/if}
    </section>

    <section>
      <h2>Typing the result</h2>
      <label>
        Paste shortcut
        <select bind:value={s.paste_key}>
          <option value="ctrl+v">Ctrl+V — most apps</option>
          <option value="ctrl+shift+v">Ctrl+Shift+V — terminals</option>
          <option value="shift+insert">Shift+Insert — most apps and terminals</option>
        </select>
      </label>
      <p class="hint">Your clipboard is restored after pasting.</p>
    </section>

    <section>
      <h2>Shortcut</h2>
      {#if sys}
        <p>
          Start / stop recording:
          {#if capturing}
            <span class="capture">Press the new keys… (Esc to cancel)</span>
          {:else if sys.shortcut}
            {#each prettyKeys(sys.shortcut) as k, i}{#if i > 0} + {/if}<kbd>{k}</kbd>{/each}
          {:else}
            <b>not set</b>
          {/if}
        </p>
        <div class="row">
          <select
            value={SHORTCUTS.some(([a]) => a === sys?.shortcut) ? sys.shortcut : ""}
            onchange={(e) => e.currentTarget.value && applyShortcut(e.currentTarget.value)}
          >
            <option value="" disabled>Choose a shortcut…</option>
            {#each SHORTCUTS as [accel, label]}<option value={accel}>{label}</option>{/each}
          </select>
          <button onclick={() => (capturing = true)} disabled={capturing}>Record keys</button>
        </div>
        {#if shortcutStatus}<p class={shortcutStatus.ok ? "ok" : "err"}>{shortcutStatus.text}</p>{/if}
        <label class="switch">
          <input type="checkbox" checked={sys.autostart} onchange={(e) => toggleAutostart(e.currentTarget.checked)} />
          <span>Start Zemzeme when I log in</span>
        </label>
        {#if sys.dev_build}
          <p class="hint">Development build: these point to <code>{sys.exe}</code>. The installed app sets them to itself on start.</p>
        {/if}
      {/if}
    </section>

    <section>
      <h2>Advanced</h2>
      <label class="switch">
        <input type="checkbox" bind:checked={s.sounds} />
        <span>Play a sound when recording starts and stops</span>
      </label>
      <label class="switch">
        <input type="checkbox" bind:checked={s.keep_audio} />
        <span>Keep recordings on disk (for troubleshooting)</span>
      </label>
      <p class="hint">Settings file: <code>{settingsPath}</code></p>
      <p class="hint">Log (no transcripts): <code>{logPath}</code></p>
    </section>
  {:else}
    <p class="hint">Loading…</p>
  {/if}
</main>

<style>
  :root {
    --bg: #f6f6f8;
    --card: #ffffff;
    --text: #1b1b1f;
    --muted: #6b6b76;
    --border: #e2e2e8;
    --accent: #5b5bd6;
    --ok: #1f8a4c;
    --err: #c9303e;
    --input: #ffffff;
  }
  :root { color-scheme: light dark; }
  @media (prefers-color-scheme: dark) {
    :root {
      --bg: #141417;
      --card: #1d1d22;
      --text: #ececf1;
      --muted: #9a9aa6;
      --border: #2e2e36;
      --accent: #8f8fff;
      --ok: #4cc27d;
      --err: #ff6b78;
      --input: #25252b;
    }
  }
  :global(body) {
    margin: 0;
    font-family: Inter, "Vazirmatn", system-ui, sans-serif;
    font-size: 14px;
    background: var(--bg);
    color: var(--text);
  }
  main { padding: 20px 20px 32px; max-width: 560px; margin: 0 auto; }
  header { display: flex; align-items: baseline; justify-content: space-between; margin-bottom: 12px; }
  h1 { font-size: 20px; margin: 0; }
  .saved { color: var(--ok); font-size: 13px; opacity: 0; transition: opacity 0.2s; }
  .saved.show { opacity: 1; }
  section {
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 14px 16px;
    margin-bottom: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h2 { font-size: 14px; margin: 0; }
  p { margin: 0; }
  .hint { color: var(--muted); font-size: 12.5px; }
  .ok { color: var(--ok); font-size: 13px; }
  .err { color: var(--err); font-size: 13px; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; color: var(--muted); }
  label.switch { flex-direction: row; align-items: center; gap: 8px; color: var(--text); cursor: pointer; }
  .row { display: flex; gap: 8px; align-items: center; }
  .row > input, .row > select { flex: 1; min-width: 0; }
  input:not([type="checkbox"]), select, textarea {
    font: inherit;
    color: var(--text);
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: 7px;
    padding: 7px 9px;
  }
  textarea { resize: vertical; }
  /* WebKitGTK draws native selects with the GTK theme (white in dark mode),
     so draw them ourselves. */
  select {
    appearance: none;
    -webkit-appearance: none;
    padding-right: 28px;
    background-image: linear-gradient(45deg, transparent 50%, var(--muted) 50%),
      linear-gradient(135deg, var(--muted) 50%, transparent 50%);
    background-position: calc(100% - 15px) 50%, calc(100% - 10px) 50%;
    background-size: 5px 5px;
    background-repeat: no-repeat;
  }
  option { background: var(--input); color: var(--text); }
  input[type="checkbox"] { width: 16px; height: 16px; accent-color: var(--accent); }
  button {
    font: inherit;
    padding: 7px 12px;
    border-radius: 7px;
    border: 1px solid var(--border);
    background: var(--input);
    color: var(--text);
    cursor: pointer;
    white-space: nowrap;
  }
  button:disabled { opacity: 0.5; cursor: default; }
  button.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  button.ghost { border-color: transparent; background: transparent; color: var(--muted); }
  code { font-size: 12px; word-break: break-all; }
  kbd {
    font: inherit; font-size: 12px; padding: 1px 6px;
    border: 1px solid var(--border); border-bottom-width: 2px; border-radius: 5px;
  }
  .capture { color: var(--accent); font-weight: 600; }
  .meter { height: 6px; background: var(--border); border-radius: 3px; overflow: hidden; }
  .meter span { display: block; height: 100%; background: var(--accent); transition: width 0.08s linear; }
</style>
