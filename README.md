# FeatherNotes

Lightweight notepad from the FeatherWorks suite, written in Rust with [pelicano](https://github.com/feather-works/pelicano).

- Tabs that come back as you left them, unsaved changes included
- Find and replace, go to line, Ctrl+wheel zoom
- Markdown with hidden markup, formatting bar and clickable tasks
- 30+ encodings, automatic detection and repair of broken accents
- Show invisible characters
- Sticky notes on the desktop: colors, pin to the background, list with trash
- System shortcut for a new sticky note (Super+Alt+N on GNOME) and start with the system

## Install

Download the [latest release](https://github.com/feather-works/feathernotes/releases/latest) or install it with the FeatherWorks Launcher:

- Linux: `feathernotes-linux-amd64.deb`, or the `.tar.gz` and run `instalar.sh`
- Windows: `FeatherNotes-windows-x64.exe`

## Build

```bash
cargo run --release
cargo test
pacote/gerar.sh   # installers, in pacote/saida/
```

License: GPL-3.0.
