mod cache;
mod epg;
mod m3u;
mod player;
mod ui;

use anyhow::{bail, Result};
use clap::Parser;

#[derive(Parser)]
#[command(name = "ipb", about = "IPTV stream picker", version)]
struct Cli {
    /// M3U playlist URL or local file path (bypasses persistence)
    #[arg(short, long, value_name = "URL_OR_PATH")]
    source: Option<String>,
}

fn load_epg(url: Option<&str>) -> Option<epg::EpgData> {
    let url = url?;
    eprint!("Loading EPG...");
    match epg::load(url) {
        Some(data) => {
            eprintln!(" done.");
            Some(data)
        }
        None => {
            eprintln!(" failed (EPG unavailable).");
            None
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // 1. mpv preflight — fail fast before loading anything
    player::check_installed()?;

    // 2. --source: one-shot mode, bypass all persistence
    if let Some(source) = cli.source {
        eprint!("Loading playlist...");
        let content = m3u::fetch_or_read_raw(&source)?;
        let channels = m3u::parse(&content);
        eprintln!(" {} channels found.", channels.len());
        if channels.is_empty() {
            eprintln!("Response body: {} bytes", content.len());
            match content.lines().next() {
                Some(line) => eprintln!("First line: {:?}", &line[..line.len().min(120)]),
                None => eprintln!("Response body is empty"),
            }
            bail!("No channels found in playlist.");
        }
        let epg_url = m3u::parse_epg_url(&content);
        let epg_data = load_epg(epg_url.as_deref());
        loop {
            match ui::run(&channels, epg_data.as_ref())? {
                ui::Action::Play(ch) => player::play(&ch.url, &ch.name)?,
                ui::Action::Quit => break,
                ui::Action::AddPlaylist | ui::Action::OpenSettings => {} // no-op in one-shot mode
            }
        }
        return Ok(());
    }

    // 3. Persistence flow
    let mut playlists = cache::load_playlists();

    if playlists.is_empty() {
        playlists.push(cache::prompt_add_playlist()?);
        cache::save_playlists(&playlists);
    }

    let mut active_idx = if playlists.len() == 1 {
        0
    } else {
        cache::pick_playlist(&playlists)?
    };

    'main: loop {
        let content = load_playlist_content(&mut playlists, active_idx)?;
        cache::save_playlists(&playlists);

        let channels = m3u::parse(&content);
        if channels.is_empty() {
            bail!("No channels found in playlist.");
        }

        // EPG: prefer manually set URL, fall back to M3U header
        let header_epg = m3u::parse_epg_url(&content);
        let epg_url = playlists[active_idx]
            .epg_url
            .as_deref()
            .or(header_epg.as_deref());
        let epg_data = load_epg(epg_url);

        loop {
            match ui::run(&channels, epg_data.as_ref())? {
                ui::Action::Play(ch) => player::play(&ch.url, &ch.name)?,
                ui::Action::Quit => break 'main,
                ui::Action::AddPlaylist => {
                    playlists.push(cache::prompt_add_playlist()?);
                    cache::save_playlists(&playlists);
                    active_idx = playlists.len() - 1;
                    continue 'main;
                }
                ui::Action::OpenSettings => {
                    ui::run_settings(&mut playlists)?;
                    cache::save_playlists(&playlists);
                    if playlists.is_empty() {
                        playlists.push(cache::prompt_add_playlist()?);
                        cache::save_playlists(&playlists);
                    }
                    active_idx = active_idx.min(playlists.len().saturating_sub(1));
                    continue 'main;
                }
            }
        }
    }

    Ok(())
}

fn load_playlist_content(
    playlists: &mut Vec<cache::PlaylistEntry>,
    idx: usize,
) -> Result<String> {
    let url = playlists[idx].url.clone();

    if !cache::needs_refresh(&playlists[idx]) {
        if let Some(cached) = cache::load_cached_m3u(&url) {
            return Ok(cached);
        }
    }

    eprint!("Fetching playlist...");
    let content = m3u::fetch_url(&url)?;
    eprintln!(" done.");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    playlists[idx].last_fetched = now;
    cache::save_cached_m3u(&url, &content);

    Ok(content)
}
