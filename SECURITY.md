# Security

## Reporting a problem

Please **don't open a public issue** for security problems. Report them privately through
[GitHub's private vulnerability reporting](https://github.com/mhrezaei/Zemzeme/security/advisories/new).

You'll get a reply within a week. Fixes go into the next release, and you'll be credited unless you'd rather not be.

Only the latest release is supported.

## How Zemzeme handles your data

- **API key:** stored in the system keyring (GNOME Keyring), encrypted with your login password. It is never written to the settings file or the log. Any program running as your user while you're logged in can read the keyring; that's how desktop keyrings work on Linux.
- **Audio:** kept in memory and sent to OpenRouter over HTTPS, then discarded. It's written to disk only if you turn on *Advanced → Keep recordings*.
- **Transcripts:** pasted through the clipboard, which is restored a moment later. Clipboard managers may keep a copy. Transcripts are never written to the log.
- **Virtual keyboard:** the udev rule lets the logged-in desktop user create a virtual keyboard (`/dev/uinput`), which the app uses to press the paste keys. It applies only to the user at the active local session.

**Tip:** set a monthly credit limit on your key at [openrouter.ai/keys](https://openrouter.ai/keys), so a leaked key can't cost much.
