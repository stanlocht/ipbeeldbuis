# ipbeeldbuis

A Rust CLI tool for watching IPTV streams from M3U playlists, with a smooth TUI interface.

## Project

- Language: Rust
- Working directory: `/Users/stan/Documents/CODING/ipbeeldbuis`

## Stack

- `ratatui` + `crossterm` — TUI
- `reqwest` — HTTP (fetch M3U URLs)
- `clap` — CLI argument parsing
- `mpv` — video playback (external binary, must be installed)

## Run

```bash
cargo run -- --url "http://your-m3u-url"
cargo run -- --file /path/to/playlist.m3u
```

## Roadmap

- [x] M3U parser
- [x] Ratatui TUI with search + category tabs
- [x] mpv launcher
- [x] Local cache with ETag refresh
- [ ] Chromecast casting (planned)
