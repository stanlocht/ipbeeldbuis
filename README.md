# ipbeeldbuis

A terminal UI for browsing and playing IPTV streams from M3U playlists.

**Requires [mpv](https://mpv.io) to be installed for playback.**

---

## Install

### curl (macOS / Linux)

```bash
curl -sSL https://raw.githubusercontent.com/stanlocht/ipbeeldbuis/main/install.sh | bash
```

### pip / uv

```bash
pip install ipbeeldbuis
# or
uv add ipbeeldbuis
```

The binary is downloaded automatically on first run.

### Build from source

Requires [Rust](https://rustup.rs).

```bash
cargo install --git https://github.com/stanlocht/ipbeeldbuis
```

---

## Usage

Launch with a playlist URL or local file:

```bash
ipb --source "http://your-provider.com/playlist.m3u"
```

Or just run `ipb` to use saved playlists:

```bash
ipb
```

---

## Key bindings

| Key | Action |
|-----|--------|
| `↑ / ↓` or `j / k` | Navigate channels |
| `← / →` or `h / l` | Switch category tabs |
| `/` | Search |
| `t` | Toggle content filter (All / Live / Movies / Series) |
| `e` | Toggle EPG panel (Now / Next) |
| `s` | Settings — manage playlists and EPG URLs |
| `a` | Add a new playlist |
| `Enter` | Play selected channel |
| `q` | Quit |

---

## EPG

If your M3U playlist includes an EPG URL in its header (`url-tvg="..."`), the EPG is loaded automatically. You can also set or override the EPG URL per playlist in the settings screen (`s`).
