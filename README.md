# Rusty Tube 🎧

A terminal-based YouTube Music player written in Rust. Features search, playlist loading, queue management, volume controls, seek controls, and macOS Now Playing media keys support.

> [!NOTE]
> This project was vibe coded with ❤️ using **Claude** and **Antigravity**.
>
> It was inspired by **[youtube-music-cli](https://github.com/involvex/youtube-music-cli)**, a similar CLI application written in Node.js.

---

## Features

- 🔍 **Search & Discover**: Find songs and playlists directly on YouTube Music.
- 🎛️ **Now Playing Integration**: Supports macOS Media Keys, AirPods, and Control Center via `souvlaki`.
- 🔑 **Automatic Authentication**: Extract browser cookies to authenticate automatically with YouTube Music.
- 📻 **Recommendations**: Automatic queue expansion with radio recommendations and repeat modes.
- 🎚️ **Playback Control**: Pause, resume, seek, and adjust volume using hotkeys.
- 🖥️ **Terminal UI**: Responsive, beautiful interface powered by `ratatui`.

---

## Prerequisites

Before running Rusty Tube, ensure you have the following system dependencies installed:

- **ffmpeg**: Required for audio streaming and decoding.
- **yt-dlp**: Required for extracting media URLs.

### Installation on macOS:
```bash
brew install ffmpeg yt-dlp
```

---

## Getting Started

To run the application, clone this repository and run cargo:

```bash
cargo run
```

---

## Controls

- `q` - Quit application (when search or login input is not focused)
- `Space` / `p` - Play / Pause
- `s` - Seek forward / search focus depending on layout
- `v` / `Volume` - Adjust volume
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
