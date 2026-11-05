# my-whisper — نقشه‌ی کار

شورتکات بزن → حرف بزن → متن همان جایی که کرسر هست paste شود.

## تصمیم‌های فنی

| بخش | انتخاب |
|---|---|
| فریم‌ورک | Tauri 2 + Rust، رابط کاربری SvelteKit + TypeScript (با npm و Node 24) |
| سرویس STT | OpenRouter، endpoint `POST /api/v1/audio/transcriptions` (صدا base64) |
| مدل پیش‌فرض | `openai/gpt-4o-transcribe` (قابل تغییر در تنظیمات، مثلاً `whisper-large-v3-turbo`، `elevenlabs/scribe-v2`) |
| ضبط صدا | `cpal` → 16kHz mono → WAV در حافظه (بعداً FLAC/Opus) |
| شورتکات (لینوکس) | Custom Shortcut گنوم → `my-whisper toggle` → پیام به نسخه‌ی در حال اجرا (single-instance) |
| شورتکات (مک/ویندوز) | `tauri-plugin-global-shortcut` |
| Paste (لینوکس) | ذخیره‌ی کلیپ‌بورد → `wl-copy` متن → `ydotool` کلید Ctrl+V → برگرداندن کلیپ‌بورد |
| Paste (مک/ویندوز) | crate `enigo` (Cmd+V / Ctrl+V) |
| نگهداری کلید API | Keyring سیستم‌عامل (crate `keyring`)، بقیه‌ی تنظیمات با `tauri-plugin-store` |
| Overlay | پنجره‌ی بی‌قاب، شفاف، همیشه رو، **بدون فوکوس** (`focusable: false`)، روی لینوکس با XWayland (`GDK_BACKEND=x11`) برای تعیین جای پنجره |

## معماری

```
src-tauri/src/
  main.rs              اجبار XWayland روی لینوکس
  lib.rs               راه‌اندازی، tray، single-instance، دستورات CLI
  state.rs             ماشین حالت: Idle → Recording → Transcribing → Pasting → Idle (+ Cancel / Error)
  overlay.rs           نمایش و جای پنجره‌ی کوچک
  audio/               (فاز ۲) لیست میکروفون‌ها، ضبط، سطح صدا، WAV
  stt/                 (فاز ۳) trait مشترک STT + پیاده‌سازی OpenRouter  ← ترجمه بعداً اینجا اضافه می‌شود
  inject/              (فاز ۵) کلیپ‌بورد + شبیه‌سازی paste
  config.rs            (فاز ۶) تنظیمات و keyring
src/routes/
  overlay/             پنجره‌ی کوچک
  settings/            پنجره‌ی تنظیمات
```

جای توسعه‌ی بعدی: بعد از `stt` یک مرحله‌ی «پردازش متن» (pipeline) در نظر می‌گیریم که فعلاً خالی است. ترجمه، اصلاح علائم نگارشی، یا برعکس (متن به صدا) بعداً به همین مرحله اضافه می‌شوند.

## اجرا در حالت توسعه

```bash
PATH=$HOME/.nvm/versions/node/v24.19.0/bin:$HOME/.cargo/bin:$PATH npm run tauri dev
./src-tauri/target/debug/my-whisper toggle   # شروع/پایان
./src-tauri/target/debug/my-whisper cancel   # لغو
```

## مراحل

### فاز ۰ — پیش‌نیازها ✅
- [x] git لوکال
- [x] Rust (rustup, cargo 1.99.1)
- [x] پکیج‌های سیستمی Tauri + ALSA + wl-clipboard + ydotool
- [x] Node 24 (با nvm) + npm (bun در این محیط گیر می‌کرد)

### فاز ۱ — اسکلت پروژه ✅
- [x] ساخت پروژه‌ی Tauri 2 + Svelte
- [x] آیکون tray با منوی Start/Stop، Settings و Quit
- [x] single-instance + دستورات `my-whisper toggle | cancel | settings | quit`
- [x] overlay پایین وسط مانیتوری که موس رویش است، بدون گرفتن فوکوس
- **خروجی:** اجرای `my-whisper toggle` در ترمینال به برنامه‌ی در حال اجرا پیام می‌رساند

### فاز ۲ — ضبط صدا ✅
- [x] لیست میکروفون‌ها (`my-whisper mics`) و ضبط از میکروفون انتخاب‌شده (فعلاً با `MY_WHISPER_MIC`)
- [x] resample به 16kHz mono، سطح صدا (RMS) برای انیمیشن موج
- [x] ساخت WAV در حافظه، ذخیره در `~/.cache/dev.hadi.mywhisper/last.wav`
- [x] ابزار تست: `cargo run --example record -- 5 default out.wav`
- **یادداشت:** لیست ALSA شلوغ و تکراری است. در فاز ۶ لیست میکروفون‌ها از PipeWire خوانده شود (`wpctl` یا feature `pipewire` در cpal).
- **یادداشت:** هدست بلوتوث در اولین ضبط حدود ۲ ثانیه طول می‌کشد تا به حالت میکروفون برود.
- **یادداشت:** هدست بلوتوث در حالت A2DP (فقط پخش) میکروفون ندارد و سکوت کامل می‌فرستد. برنامه این را تشخیص می‌دهد و خطای «Microphone is silent» نشان می‌دهد.
- **یادداشت:** انتخاب میکروفون از طریق ALSA ممکن است با متغیر `PIPEWIRE_NODE=<id>` انجام شود (تست شد، id میکروفون داخلی = 59). برای فاز ۶ گزینه‌ی ساده‌ای است.

### فاز ۳ — اتصال به OpenRouter
- کلاینت STT با `reqwest`، پارامتر زبان (`fa`)، timeout و مدیریت خطا
- **خروجی:** فایل تست به متن فارسی تبدیل می‌شود

### فاز ۴ — Overlay
- حالت‌های «در حال گوش دادن» (موج واقعی + تایمر)، «در حال پردازش» و «خطا»
- دکمه‌ی ■ برای پایان و ✕ برای لغو، دکمه‌ی ⚙ برای تنظیمات (نسخه‌ی اولیه در فاز ۱ ساخته شد)
- **خروجی:** شورتکات → overlay → ضبط → پایان → متن در لاگ

### فاز ۵ — Paste در جای کرسر
- راه‌اندازی `ydotoold` (دسترسی به `/dev/uinput` با یک قانون udev و یک user service)
- ذخیره و برگرداندن کلیپ‌بورد، Ctrl+V
- گزینه‌ی Ctrl+Shift+V برای ترمینال‌ها
- **خروجی:** چرخه‌ی کامل در هر برنامه‌ای کار می‌کند

### فاز ۶ — تنظیمات
- کلید API (keyring)، مدل، زبان، میکروفون، روش paste، جای overlay
- دکمه‌ی «تست اتصال»
- **خروجی:** پنجره‌ی تنظیمات با چرخدنده

### فاز ۷ — یکپارچه‌سازی با سیستم
- ثبت خودکار شورتکات گنوم با `gsettings` (پیشنهاد: `Super+Alt+Space`)
- اجرای خودکار با بالا آمدن سیستم، فایل `.desktop`
- **خروجی:** بعد از ری‌استارت هم کار می‌کند

### فاز ۸ — صیقل‌کاری
- صدای کوتاه شروع و پایان، پیام‌های خطای روشن، لاگ
- بیلد `.deb`

### بعداً
- ترجمه و حالت‌های دیگر پردازش متن
- تشخیص سکوت (VAD) برای پایان خودکار
- تاریخچه
- streaming (نمایش متن هم‌زمان با حرف زدن)
- مک و ویندوز
