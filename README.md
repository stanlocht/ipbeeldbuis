# ipbeeldbuis

```
    _       __              __    ____          _
   (_)___  / /_  ___  ___  / /___/ / /_  __  __(_)____
  / / __ \/ __ \/ _ \/ _ \/ / __  / __ \/ / / / / ___/
 / / /_/ / /_/ /  __/  __/ / /_/ / /_/ / /_/ / (__  )
/_/ .___/_.___/\___/\___/_/\__,_/_.___/\__,_/_/____/
 /_/
```

A terminal UI for browsing and playing IPTV streams from M3U playlists.

**Requires [mpv](https://mpv.io) to be installed for playback.**
Supports casting streams directly to a **Chromecast** device on your local network.

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

### uvx / pipx (no install required)

```bash
uvx ipbeeldbuis
# or
pipx run ipbeeldbuis
```

### Build from source

Requires [Rust](https://rustup.rs).

```bash
cargo install --git https://github.com/stanlocht/ipbeeldbuis
```

---

## Usage

Launch with a playlist URL or local file:

```bash
ipbeeldbuis --source "http://your-provider.com/playlist.m3u"
```

Or just run `ipbeeldbuis` to use saved playlists:

```bash
ipbeeldbuis
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
| `c` | Cast selected channel to Chromecast |
| `q` | Quit |

---

## EPG

If your M3U playlist includes an EPG URL in its header (`url-tvg="..."`), the EPG is loaded automatically. You can also set or override the EPG URL per playlist in the settings screen (`s`).
