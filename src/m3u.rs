use anyhow::{Context, Result};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum ContentType {
    Live,
    Movie,
    Series,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Channel {
    pub name: String,
    pub url: String,
    pub group: String,
    pub logo: Option<String>,
    pub tvg_id: Option<String>,
    pub content_type: ContentType,
}

impl Channel {
    pub fn display_group(&self) -> &str {
        if self.group.is_empty() {
            "Uncategorized"
        } else {
            &self.group
        }
    }
}

fn infer_content_type(group: &str) -> ContentType {
    let g = group.to_lowercase();
    if g.contains("serie")
        || g.contains("season")
        || g.contains("episode")
        || g.contains("show")
        || g.contains("tvshow")
    {
        ContentType::Series
    } else if g.contains("movie")
        || g.contains("film")
        || g.contains("vod")
        || g.contains("cinema")
        || g.contains("4k movie")
    {
        ContentType::Movie
    } else {
        ContentType::Live
    }
}

pub fn parse(content: &str) -> Vec<Channel> {
    let mut channels = Vec::new();
    let mut lines = content.lines().peekable();

    while let Some(line) = lines.next() {
        let line = line.trim();
        if !line.starts_with("#EXTINF:") {
            continue;
        }

        let name = extract_display_name(line);
        let group = extract_attr(line, "group-title").unwrap_or_default();
        let logo = extract_attr(line, "tvg-logo");
        let tvg_id = extract_attr(line, "tvg-id");
        let content_type = infer_content_type(&group);

        // Next non-empty, non-comment line is the URL
        let url = loop {
            match lines.next() {
                Some(l) => {
                    let l = l.trim();
                    if !l.is_empty() && !l.starts_with('#') {
                        break l.to_string();
                    }
                }
                None => break String::new(),
            }
        };

        if url.is_empty() {
            continue;
        }

        channels.push(Channel {
            name,
            url,
            group,
            logo,
            tvg_id,
            content_type,
        });
    }

    channels
}

pub fn fetch_or_read_raw(source: &str) -> Result<String> {
    if source.starts_with("http://") || source.starts_with("https://") {
        fetch_url(source)
    } else {
        std::fs::read_to_string(Path::new(source))
            .with_context(|| format!("Failed to read file: {source}"))
    }
}

#[allow(dead_code)]
pub fn fetch_or_read(source: &str) -> Result<Vec<Channel>> {
    Ok(parse(&fetch_or_read_raw(source)?))
}

pub fn parse_epg_url(content: &str) -> Option<String> {
    let header = content.lines().next()?;
    if !header.starts_with("#EXTM3U") {
        return None;
    }
    extract_attr(header, "url-tvg").or_else(|| extract_attr(header, "x-tvg-url"))
}

pub fn fetch_url(url: &str) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .context("Failed to build HTTP client")?;
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("Failed to fetch: {url}"))?;
    let status = response.status();
    if !status.is_success() {
        // Warn but don't bail — some providers use non-standard codes (e.g. 512)
        // alongside valid content.
        eprintln!("Warning: server returned HTTP {status} — trying to parse anyway");
    }
    let bytes = response.bytes().context("Failed to read response body")?;
    // Detect gzip by magic bytes — some servers omit Content-Encoding: gzip
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        use std::io::Read;
        let mut out = String::new();
        flate2::read::GzDecoder::new(&bytes[..])
            .read_to_string(&mut out)
            .context("Failed to decompress gzip response")?;
        Ok(out)
    } else {
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

fn extract_display_name(extinf: &str) -> String {
    extinf
        .rfind(',')
        .map(|i| extinf[i + 1..].trim().to_string())
        .unwrap_or_else(|| "Unknown".to_string())
}

fn extract_attr(line: &str, attr: &str) -> Option<String> {
    // Case-insensitive key search (TVG-ID, tvg-id, Tvg-Id all work).
    // We compare bytes with eq_ignore_ascii_case — safe because attribute
    // keys are always ASCII. We advance a full UTF-8 char at a time so we
    // never land on a non-char-boundary even with non-ASCII group/name values.
    let key = format!("{attr}=\"");
    let key_bytes = key.as_bytes();
    let line_bytes = line.as_bytes();

    let mut i = 0;
    while i + key_bytes.len() <= line_bytes.len() {
        if line_bytes[i..i + key_bytes.len()].eq_ignore_ascii_case(key_bytes) {
            let start = i + key_bytes.len();
            let end = line[start..].find('"')? + start;
            let value = line[start..end].trim().to_string();
            return if value.is_empty() { None } else { Some(value) };
        }
        i += line[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    None
}
