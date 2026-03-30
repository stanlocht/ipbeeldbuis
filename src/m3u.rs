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

pub(crate) fn infer_content_type(group: &str) -> ContentType {
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

pub(crate) fn extract_display_name(extinf: &str) -> String {
    extinf
        .rfind(',')
        .map(|i| extinf[i + 1..].trim().to_string())
        .unwrap_or_else(|| "Unknown".to_string())
}

pub(crate) fn extract_attr(line: &str, attr: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_channel(name: &str, url: &str, group: &str) -> Channel {
        Channel {
            name: name.to_string(),
            url: url.to_string(),
            group: group.to_string(),
            logo: None,
            tvg_id: None,
            content_type: ContentType::Live,
        }
    }

    // ── parse ────────────────────────────────────────────────────────────────

    #[test]
    fn parse_empty_returns_empty() {
        assert!(parse("").is_empty());
    }

    #[test]
    fn parse_header_only_returns_empty() {
        assert!(parse("#EXTM3U\n").is_empty());
    }

    #[test]
    fn parse_single_channel_all_attributes() {
        let m3u = concat!(
            "#EXTM3U\n",
            "#EXTINF:-1 tvg-id=\"ch1\" tvg-logo=\"http://logo.example.com/img.png\" group-title=\"News\",Channel 1\n",
            "http://stream.example.com/ch1\n"
        );
        let channels = parse(m3u);
        assert_eq!(channels.len(), 1);
        let ch = &channels[0];
        assert_eq!(ch.name, "Channel 1");
        assert_eq!(ch.url, "http://stream.example.com/ch1");
        assert_eq!(ch.group, "News");
        assert_eq!(ch.logo, Some("http://logo.example.com/img.png".to_string()));
        assert_eq!(ch.tvg_id, Some("ch1".to_string()));
        assert_eq!(ch.content_type, ContentType::Live);
    }

    #[test]
    fn parse_multiple_channels() {
        let m3u = concat!(
            "#EXTM3U\n",
            "#EXTINF:-1,Alpha\n",
            "http://example.com/1\n",
            "#EXTINF:-1,Beta\n",
            "http://example.com/2\n",
        );
        let channels = parse(m3u);
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].name, "Alpha");
        assert_eq!(channels[0].url, "http://example.com/1");
        assert_eq!(channels[1].name, "Beta");
        assert_eq!(channels[1].url, "http://example.com/2");
    }

    #[test]
    fn parse_skips_channel_with_no_url() {
        let m3u = "#EXTM3U\n#EXTINF:-1,Channel 1\n";
        assert!(parse(m3u).is_empty());
    }

    #[test]
    fn parse_skips_comment_between_extinf_and_url() {
        let m3u = concat!(
            "#EXTM3U\n",
            "#EXTINF:-1,Channel 1\n",
            "# this is a comment\n",
            "http://example.com/1\n",
        );
        let channels = parse(m3u);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].url, "http://example.com/1");
    }

    #[test]
    fn parse_skips_blank_lines_between_extinf_and_url() {
        let m3u = "#EXTM3U\n#EXTINF:-1,Channel 1\n\n\nhttp://example.com/1\n";
        let channels = parse(m3u);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].url, "http://example.com/1");
    }

    #[test]
    fn parse_attributes_are_case_insensitive() {
        let m3u = concat!(
            "#EXTM3U\n",
            "#EXTINF:-1 TVG-ID=\"id1\" TVG-LOGO=\"http://logo.example.com/\" GROUP-TITLE=\"Sports\",Chan\n",
            "http://example.com/1\n",
        );
        let channels = parse(m3u);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].tvg_id, Some("id1".to_string()));
        assert_eq!(
            channels[0].logo,
            Some("http://logo.example.com/".to_string())
        );
        assert_eq!(channels[0].group, "Sports");
    }

    #[test]
    fn parse_name_is_text_after_last_comma() {
        let m3u = "#EXTM3U\n#EXTINF:-1 group-title=\"A,B\",My Channel, HD\nhttp://example.com/1\n";
        let channels = parse(m3u);
        assert_eq!(channels[0].name, "HD");
    }

    #[test]
    fn parse_name_is_unknown_when_no_comma() {
        let m3u = "#EXTM3U\n#EXTINF:-1\nhttp://example.com/1\n";
        let channels = parse(m3u);
        assert_eq!(channels[0].name, "Unknown");
    }

    #[test]
    fn parse_missing_optional_attrs_are_none() {
        let m3u = "#EXTM3U\n#EXTINF:-1,Channel\nhttp://example.com/1\n";
        let channels = parse(m3u);
        assert_eq!(channels[0].logo, None);
        assert_eq!(channels[0].tvg_id, None);
    }

    // ── infer_content_type ───────────────────────────────────────────────────

    #[test]
    fn infer_series_keywords() {
        for kw in &["serie", "Series", "SEASON", "Episode", "show", "tvshow"] {
            assert_eq!(
                infer_content_type(kw),
                ContentType::Series,
                "expected Series for {kw}"
            );
        }
    }

    #[test]
    fn infer_movie_keywords() {
        for kw in &["movie", "Movies", "FILM", "VOD", "Cinema", "4K Movie"] {
            assert_eq!(
                infer_content_type(kw),
                ContentType::Movie,
                "expected Movie for {kw}"
            );
        }
    }

    #[test]
    fn infer_live_is_default() {
        for kw in &["Sports", "News", "Entertainment", "Kids", ""] {
            assert_eq!(
                infer_content_type(kw),
                ContentType::Live,
                "expected Live for {kw}"
            );
        }
    }

    // ── extract_display_name ─────────────────────────────────────────────────

    #[test]
    fn extract_display_name_after_last_comma() {
        assert_eq!(
            extract_display_name("#EXTINF:-1 group-title=\"x\",My Channel"),
            "My Channel"
        );
    }

    #[test]
    fn extract_display_name_trims_whitespace() {
        assert_eq!(extract_display_name("#EXTINF:-1,  Channel  "), "Channel");
    }

    #[test]
    fn extract_display_name_no_comma_gives_unknown() {
        assert_eq!(extract_display_name("#EXTINF:-1"), "Unknown");
    }

    // ── extract_attr ─────────────────────────────────────────────────────────

    #[test]
    fn extract_attr_finds_value() {
        assert_eq!(
            extract_attr(r#"#EXTINF:-1 group-title="News","#, "group-title"),
            Some("News".to_string())
        );
    }

    #[test]
    fn extract_attr_case_insensitive_key() {
        assert_eq!(
            extract_attr(r#"#EXTINF:-1 GROUP-TITLE="Sports","#, "group-title"),
            Some("Sports".to_string())
        );
    }

    #[test]
    fn extract_attr_returns_none_when_missing() {
        assert_eq!(extract_attr("#EXTINF:-1,Channel", "tvg-id"), None);
    }

    #[test]
    fn extract_attr_returns_none_for_empty_value() {
        assert_eq!(extract_attr(r#"#EXTINF:-1 tvg-id="","#, "tvg-id"), None);
    }

    #[test]
    fn extract_attr_handles_unicode_in_values() {
        assert_eq!(
            extract_attr(r#"#EXTINF:-1 group-title="Téléfilm","#, "group-title"),
            Some("Téléfilm".to_string())
        );
    }

    // ── Channel::display_group ───────────────────────────────────────────────

    #[test]
    fn display_group_empty_returns_uncategorized() {
        let ch = make_channel("Test", "http://x.com", "");
        assert_eq!(ch.display_group(), "Uncategorized");
    }

    #[test]
    fn display_group_nonempty_returns_group() {
        let ch = make_channel("Test", "http://x.com", "Sports");
        assert_eq!(ch.display_group(), "Sports");
    }

    // ── fetch_or_read_raw ────────────────────────────────────────────────────

    #[test]
    fn fetch_or_read_raw_reads_existing_file() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        let content = "#EXTM3U\n#EXTINF:-1,Test\nhttp://example.com\n";
        tmp.write_all(content.as_bytes()).unwrap();
        let result = fetch_or_read_raw(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(result, content);
    }

    #[test]
    fn fetch_or_read_raw_errors_on_missing_file() {
        let result = fetch_or_read_raw("/tmp/__ipbeeldbuis_no_such_file_xyz.m3u");
        assert!(result.is_err());
    }
}
