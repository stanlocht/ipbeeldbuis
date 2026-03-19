use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistEntry {
    pub name: String,
    pub url: String,
    pub last_fetched: u64, // UNIX timestamp
    #[serde(default)]
    pub epg_url: Option<String>,
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ipbeeldbuis")
        .join("playlists.json")
}

pub(crate) fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ipbeeldbuis")
}

pub(crate) fn url_hash(url: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    url.hash(&mut h);
    h.finish()
}

fn cache_path(url: &str) -> PathBuf {
    cache_dir().join(format!("{:016x}.m3u", url_hash(url)))
}

pub fn load_playlists() -> Vec<PlaylistEntry> {
    let path = config_path();
    if !path.exists() {
        return Vec::new();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_playlists(entries: &[PlaylistEntry]) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(entries) {
        let _ = std::fs::write(path, json);
    }
}

pub fn load_cached_m3u(url: &str) -> Option<String> {
    std::fs::read_to_string(cache_path(url)).ok()
}

pub fn save_cached_m3u(url: &str, content: &str) {
    let path = cache_path(url);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, content);
}

pub fn needs_refresh(entry: &PlaylistEntry) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let three_days: u64 = 3 * 24 * 3600;
    now.saturating_sub(entry.last_fetched) > three_days
}

pub fn prompt_add_playlist() -> Result<PlaylistEntry> {
    eprint!("Enter M3U URL: ");
    let mut url = String::new();
    std::io::stdin().read_line(&mut url)?;
    let url = url.trim().to_string();
    if url.is_empty() {
        anyhow::bail!("No URL provided");
    }
    let name = url
        .split('/')
        .last()
        .filter(|s| !s.is_empty())
        .unwrap_or(&url)
        .to_string();
    eprint!("Enter EPG URL (press Enter to skip): ");
    let mut epg = String::new();
    std::io::stdin().read_line(&mut epg)?;
    let epg_url = {
        let s = epg.trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    };
    Ok(PlaylistEntry { name, url, last_fetched: 0, epg_url })
}

pub fn pick_playlist(playlists: &[PlaylistEntry]) -> Result<usize> {
    eprintln!("Saved playlists:");
    for (i, p) in playlists.iter().enumerate() {
        eprintln!("  {}: {}", i + 1, p.name);
    }
    eprint!("Select [1-{}]: ", playlists.len());
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let n: usize = line.trim().parse().unwrap_or(1);
    Ok(n.saturating_sub(1).min(playlists.len() - 1))
}
