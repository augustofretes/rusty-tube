# Rusty Tube 🎧

A terminal-based YouTube Music player written in Rust. Features search, playlist loading, queue management, volume controls, seek controls, and macOS Now Playing media keys support.

> [!NOTE]
> This project was vibe coded with ❤️ using **Claude** and **Antigravity**.
>
> It was inspired by **[youtube-music-cli](https://github.com/involvex/youtube-music-cli)**, a similar CLI application built with Bun.

---

## Features

- 🔍 **Search & Discover**: Find songs and playlists directly on YouTube Music.
- 🎛️ **Now Playing Integration**: Supports macOS Media Keys, AirPods, and Control Center via `souvlaki`.
- 🔑 **Automatic Authentication**: Extract browser cookies to authenticate automatically with YouTube Music.
- 📻 **Recommendations**: Automatic queue expansion with radio recommendations and repeat modes.
- 🎚️ **Playback Control**: Pause, resume, seek, and adjust volume using hotkeys.
- 🖥️ **Terminal UI**: Responsive, beautiful interface powered by `ratatui`.

> [!NOTE]
> When you log in, macOS will prompt you to grant access to the system keychain. Rusty Tube needs this access to read your browser's stored cookie in order to authenticate with YouTube Music. The prompt appears only at login.

---

## Installation

### Homebrew (macOS)

```bash
brew install augustofretes/tap/rusty-tube
```

This pulls in the required `ffmpeg` and `yt-dlp` dependencies automatically. Once installed, launch it with:

```bash
rusty-tube
```

### Build from source

Rusty Tube depends on two system tools at runtime:

- **ffmpeg**: Required for audio streaming and decoding.
- **yt-dlp**: Required for extracting media URLs.

With the [Rust toolchain](https://rustup.rs/) installed, build and run from source:

```bash
brew install ffmpeg yt-dlp
git clone https://github.com/augustofretes/rusty-tube.git
cd rusty-tube
cargo run --release
```

---

## Controls

- `q` - Quit application (when search or login input is not focused)
- `Space` - Play / Pause
- `n` / `p` - Next / previous track
- `e` - Add selected song to the queue
- `N` - Play selected song next
- `x` - Remove selected song from the Queue tab
- `a` / `d` - Seek backward / forward
- `w` / `s` - Volume up / down
- Arrow keys / Enter - Navigate listings and select songs

---

## References & Sources Used

We relied on the following projects, libraries, and tools as references and key foundations during development:

1. **[ytmapi-rs](https://github.com/nickbabcock/ytmapi-rs)** - Unofficial YouTube Music API client in Rust.
2. **[rookie](https://github.com/thewh1teagle/rookie)** - Cross-browser cookie extraction for authentication.
3. **[souvlaki](https://github.com/Hirschybar/souvlaki)** - Cross-platform OS media controls integration (AirPods, Control Center, and media keys).
4. **[ratatui](https://github.com/ratatui-org/ratatui)** - TUI toolkit used to build the terminal user interface.
5. **[rodio](https://github.com/RustAudio/rodio)** - Audio playback library for Rust.
6. **[yt-dlp](https://github.com/yt-dlp/yt-dlp)** - CLI media extraction tool.
7. **[ffmpeg](https://ffmpeg.org/)** - Fast input seeking and PCM audio decoding.

---

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
