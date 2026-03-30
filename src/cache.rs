use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistEntry {
    pub name: String,
    pub url: String,
    pub last_fetched: u64, // UNIX timestamp
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
        .next_back()
        .filter(|s| !s.is_empty())
        .unwrap_or(&url)
        .to_string();
    Ok(PlaylistEntry {
        name,
        url,
        last_fetched: 0,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── url_hash ─────────────────────────────────────────────────────────────

    #[test]
    fn url_hash_is_deterministic() {
        let url = "http://example.com/playlist.m3u";
        assert_eq!(url_hash(url), url_hash(url));
    }

    #[test]
    fn url_hash_differs_for_different_urls() {
        let a = url_hash("http://example.com/a.m3u");
        let b = url_hash("http://example.com/b.m3u");
        assert_ne!(a, b);
    }

    #[test]
    fn url_hash_differs_for_http_vs_https() {
        let a = url_hash("http://example.com/playlist.m3u");
        let b = url_hash("https://example.com/playlist.m3u");
        assert_ne!(a, b);
    }

    // ── needs_refresh ────────────────────────────────────────────────────────

    fn entry_with_last_fetched(last_fetched: u64) -> PlaylistEntry {
        PlaylistEntry {
            name: "test".to_string(),
            url: "http://example.com/test.m3u".to_string(),
            last_fetched,
        }
    }

    #[test]
    fn needs_refresh_returns_true_for_never_fetched() {
        let entry = entry_with_last_fetched(0);
        assert!(needs_refresh(&entry));
    }

    #[test]
    fn needs_refresh_returns_true_for_old_timestamp() {
        // 10 days ago
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ten_days_ago = now.saturating_sub(10 * 24 * 3600);
        let entry = entry_with_last_fetched(ten_days_ago);
        assert!(needs_refresh(&entry));
    }

    #[test]
    fn needs_refresh_returns_false_for_recent_timestamp() {
        // 1 hour ago
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let one_hour_ago = now.saturating_sub(3600);
        let entry = entry_with_last_fetched(one_hour_ago);
        assert!(!needs_refresh(&entry));
    }

    #[test]
    fn needs_refresh_returns_false_for_two_days_ago() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let two_days_ago = now.saturating_sub(2 * 24 * 3600);
        let entry = entry_with_last_fetched(two_days_ago);
        assert!(!needs_refresh(&entry));
    }

    #[test]
    fn needs_refresh_returns_true_for_just_over_three_days() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // 3 days + 1 second
        let just_over = now.saturating_sub(3 * 24 * 3600 + 1);
        let entry = entry_with_last_fetched(just_over);
        assert!(needs_refresh(&entry));
    }

    // ── save/load round-trip ─────────────────────────────────────────────────

    #[test]
    fn save_and_load_playlists_roundtrip() {
        // Write to a temp file and parse directly to avoid touching real config.
        let entries = vec![
            PlaylistEntry {
                name: "My Playlist".to_string(),
                url: "http://example.com/p.m3u".to_string(),
                last_fetched: 12345,
            },
            PlaylistEntry {
                name: "Another".to_string(),
                url: "http://other.example.com/x.m3u".to_string(),
                last_fetched: 99999,
            },
        ];
        let json = serde_json::to_string_pretty(&entries).unwrap();
        let parsed: Vec<PlaylistEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "My Playlist");
        assert_eq!(parsed[0].url, "http://example.com/p.m3u");
        assert_eq!(parsed[0].last_fetched, 12345);
        assert_eq!(parsed[1].name, "Another");
    }

    #[test]
    fn load_playlists_returns_empty_for_invalid_json() {
        // Simulate what would happen if the JSON is corrupt
        let result: Option<Vec<PlaylistEntry>> = serde_json::from_str("not json").ok();
        assert!(result.is_none());
    }
}
