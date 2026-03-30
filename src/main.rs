mod cache;
mod chromecast;
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
        ui::splash(&mut terminal)?;
        let result = (|| -> Result<()> {
            let mut cast_status: Option<String> = None;
            let mut cast_session: Option<chromecast::CastSession> = None;
            loop {
                match ui::run(
                    &mut terminal,
                    &channels,
                    epg_data.as_ref(),
                    cast_status.clone(),
                )? {
                    ui::Action::Play(ch) => {
                        cast_status = None;
                        cast_session = None;
                        ui::restore_terminal(&mut terminal);
                        player::play(&ch.url, &ch.name)?;
                        terminal = ui::setup_terminal()?;
                    }
                    ui::Action::Cast(ch) => {
                        handle_cast(&mut terminal, &ch, &mut cast_status, &mut cast_session)?;
                    }
                    ui::Action::CastControl => {
                        handle_cast_control(&mut terminal, &mut cast_status, &mut cast_session)?;
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
    ui::splash(&mut terminal)?;
    let result = (|| -> Result<()> {
        let mut cast_status: Option<String> = None;
        let mut cast_session: Option<chromecast::CastSession> = None;
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
                match ui::run(
                    &mut terminal,
                    &channels,
                    epg_data.as_ref(),
                    cast_status.clone(),
                )? {
                    ui::Action::Play(ch) => {
                        cast_status = None;
                        cast_session = None;
                        ui::restore_terminal(&mut terminal);
                        player::play(&ch.url, &ch.name)?;
                        terminal = ui::setup_terminal()?;
                    }
                    ui::Action::Cast(ch) => {
                        handle_cast(&mut terminal, &ch, &mut cast_status, &mut cast_session)?;
                    }
                    ui::Action::CastControl => {
                        handle_cast_control(&mut terminal, &mut cast_status, &mut cast_session)?;
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

fn handle_cast(
    terminal: &mut ui::Term,
    ch: &m3u::Channel,
    cast_status: &mut Option<String>,
    cast_session: &mut Option<chromecast::CastSession>,
) -> Result<()> {
    ui::draw_cast_searching(terminal)?;
    let devices = chromecast::discover_devices(4);

    if devices.is_empty() {
        ui::run_error_popup(terminal, "No Chromecast devices found on network.")?;
        return Ok(());
    }

    let format = chromecast::detect_format(&ch.url);
    if matches!(format, chromecast::StreamFormat::MpegTs) {
        ui::run_error_popup(
            terminal,
            "Stream is MPEG-TS — not supported by Chromecast. Use Enter to play locally.",
        )?;
        return Ok(());
    }

    if let Some(idx) = ui::run_device_picker(terminal, &devices, &ch.name)? {
        match chromecast::cast(&devices[idx], &ch.url, &ch.name) {
            Ok(session) => {
                *cast_status = Some(format!("Casting: {}", ch.name));
                *cast_session = Some(session);
            }
            Err(e) => {
                ui::run_error_popup(terminal, &format!("Cast failed: {e}"))?;
            }
        }
    }

    Ok(())
}

fn handle_cast_control(
    terminal: &mut ui::Term,
    cast_status: &mut Option<String>,
    cast_session: &mut Option<chromecast::CastSession>,
) -> Result<()> {
    let device_name = match cast_session.as_ref() {
        Some(s) => s.device_name.clone(),
        None => return Ok(()),
    };

    match ui::run_cast_control_popup(terminal, &device_name)? {
        ui::CastControlAction::Pause => {
            if let Some(s) = cast_session.as_ref()
                && let Err(e) = chromecast::pause_session(s)
            {
                ui::run_error_popup(terminal, &format!("Pause failed: {e}"))?;
            }
        }
        ui::CastControlAction::Resume => {
            if let Some(s) = cast_session.as_ref()
                && let Err(e) = chromecast::resume_session(s)
            {
                ui::run_error_popup(terminal, &format!("Resume failed: {e}"))?;
            }
        }
        ui::CastControlAction::Stop => {
            let result = cast_session
                .as_ref()
                .map(chromecast::stop_session)
                .unwrap_or(Ok(()));
            if let Err(e) = result {
                ui::run_error_popup(terminal, &format!("Stop failed: {e}"))?;
            }
            // Clear session regardless — it's no longer usable after stop
            *cast_status = None;
            *cast_session = None;
        }
        ui::CastControlAction::Cancel => {}
    }
    Ok(())
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
