# my-whisper

شورتکات بزن، حرف بزن، و متن همان جایی که کرسر هست تایپ می‌شود.

- تبدیل صدا به متن با مدل‌های OpenRouter (پیش‌فرض: `openai/gpt-4o-transcribe`)
- اصطلاحات انگلیسی با حروف انگلیسی (کامیت → commit) با یک مرحله‌ی پاک‌سازی اختیاری
- کار در هر برنامه‌ای: متن paste می‌شود و کلیپ‌بورد قبلی برمی‌گردد
- صدا فقط در حافظه می‌ماند؛ لاگ هیچ متنی از حرف‌های شما ندارد

ساخته‌شده با Tauri 2 (Rust) و SvelteKit. فعلاً برای Ubuntu/GNOME (Wayland).

## نصب

پیش‌نیازهای ساخت: Rust، Node 24، و پکیج‌های سیستمی Tauri:

```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev
```

یک بار، برای اجازه‌ی کیبورد مجازی (paste):

```bash
sudo ./scripts/setup-linux.sh
```

نصب برای کاربر فعلی (بدون sudo):

```bash
./scripts/install-linux.sh
```

این دستور برنامه را در `~/.local/bin` نصب می‌کند، به منوی برنامه‌ها اضافه می‌کند، با ورود به سیستم اجرایش می‌کند و شورتکات `Super+Alt+Space` را ثبت می‌کند. حذف: `./scripts/install-linux.sh --uninstall`.

یا بسته‌ی `.deb` (قانون udev را هم نصب می‌کند):

```bash
npm run tauri build
sudo apt install ./src-tauri/target/release/bundle/deb/my-whisper_*.deb
```

## استفاده

1. روی آیکون tray کلیک کن و در تنظیمات کلید OpenRouter را وارد کن.
2. جایی که می‌خواهی تایپ کنی کلیک کن، `Super+Alt+Space` بزن و حرف بزن.
3. دوباره `Super+Alt+Space` بزن. حدود ۲ تا ۳ ثانیه بعد متن نوشته می‌شود.

دکمه‌ی ✕ در پنجره‌ی کوچک ضبط را لغو می‌کند.

## فایل‌ها

| چه | کجا |
|---|---|
| تنظیمات | `~/.config/dev.hadi.mywhisper/settings.json` |
| کلید API | Keyring سیستم (GNOME Keyring) |
| لاگ (بدون متن گفتار) | `~/.local/state/my-whisper/my-whisper.log` |

## توسعه

```bash
npm install
npm run tauri dev
```

- در حالت توسعه، `.env` خوانده می‌شود (نمونه: `.env.example`).
- ابزارهای تست: `cargo run --example record`، `--example transcribe`، `--example chime` (در `src-tauri`).
- `cargo test` در `src-tauri`.
- نقشه و تصمیم‌های فنی: [docs/PLAN.md](docs/PLAN.md)
