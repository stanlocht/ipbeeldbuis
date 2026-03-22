mod cache;
mod epg;
mod m3u;
mod player;
mod ui;

use anyhow::{Result, bail};
use clap::Parser;

#[derive(Parser)]
#[command(name = "ipbeeldbuis", about = "IPTV stream picker", version)]
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
            let n = data.len();
            eprintln!(" done ({n} channels in EPG).");
            if n > 0 {
                let mut sample: Vec<&str> = data.keys().map(String::as_str).collect();
                sample.sort();
                let show = sample.len().min(8);
                eprintln!("  EPG IDs (sample): {:?}", &sample[..show]);
            }
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
        {
            let mut ids: Vec<&str> = channels
                .iter()
                .filter_map(|ch| ch.tvg_id.as_deref())
                .collect();
            ids.sort();
            ids.dedup();
            let show = ids.len().min(8);
            if show > 0 {
                eprintln!("  M3U tvg-ids (sample): {:?}", &ids[..show]);
            }
        }
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
        let mut terminal = ui::setup_terminal()?;
        let result = (|| -> Result<()> {
            loop {
                match ui::run(&mut terminal, &channels, epg_data.as_ref())? {
                    ui::Action::Play(ch) => {
                        ui::restore_terminal(&mut terminal);
                        player::play(&ch.url, &ch.name)?;
                        terminal = ui::setup_terminal()?;
                    }
                    ui::Action::Quit => break,
                    ui::Action::AddPlaylist | ui::Action::OpenSettings => {}
                }
            }
            Ok(())
        })();
        ui::restore_terminal(&mut terminal);
        return result;
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

    let mut terminal = ui::setup_terminal()?;
    let result = (|| -> Result<()> {
        'main: loop {
            let content = load_playlist_content(&mut playlists, active_idx)?;
            cache::save_playlists(&playlists);

            let channels = m3u::parse(&content);
            if channels.is_empty() {
                bail!("No channels found in playlist.");
            }
            {
                let mut ids: Vec<&str> = channels
                    .iter()
                    .filter_map(|ch| ch.tvg_id.as_deref())
                    .collect();
                ids.sort();
                ids.dedup();
                let show = ids.len().min(8);
                if show > 0 {
                    eprintln!("  M3U tvg-ids (sample): {:?}", &ids[..show]);
                }
            }

            // EPG: prefer manually set URL, fall back to M3U header
            let header_epg = m3u::parse_epg_url(&content);
            let epg_url = playlists[active_idx]
                .epg_url
                .as_deref()
                .or(header_epg.as_deref());
            let epg_data = load_epg(epg_url);

            loop {
                match ui::run(&mut terminal, &channels, epg_data.as_ref())? {
                    ui::Action::Play(ch) => {
                        ui::restore_terminal(&mut terminal);
                        player::play(&ch.url, &ch.name)?;
                        terminal = ui::setup_terminal()?;
                    }
                    ui::Action::Quit => break 'main,
                    ui::Action::AddPlaylist => {
                        ui::restore_terminal(&mut terminal);
                        playlists.push(cache::prompt_add_playlist()?);
                        cache::save_playlists(&playlists);
                        active_idx = playlists.len() - 1;
                        terminal = ui::setup_terminal()?;
                        continue 'main;
                    }
                    ui::Action::OpenSettings => {
                        ui::run_settings(&mut terminal, &mut playlists)?;
                        cache::save_playlists(&playlists);
                        if playlists.is_empty() {
                            ui::restore_terminal(&mut terminal);
                            playlists.push(cache::prompt_add_playlist()?);
                            cache::save_playlists(&playlists);
                            terminal = ui::setup_terminal()?;
                        }
                        active_idx = active_idx.min(playlists.len().saturating_sub(1));
                        continue 'main;
                    }
                }
            }
        }
        Ok(())
    })();
    ui::restore_terminal(&mut terminal);
    result
}

fn load_playlist_content(playlists: &mut [cache::PlaylistEntry], idx: usize) -> Result<String> {
    let url = playlists[idx].url.clone();

    if !cache::needs_refresh(&playlists[idx])
        && let Some(cached) = cache::load_cached_m3u(&url)
    {
        return Ok(cached);
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
