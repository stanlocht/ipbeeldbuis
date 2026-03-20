use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Programme {
    pub title: String,
    pub desc: Option<String>,
    pub start: u64, // UNIX timestamp UTC
    pub stop: u64,
}

pub type EpgData = HashMap<String, Vec<Programme>>;

fn epg_cache_path(url: &str) -> PathBuf {
    crate::cache::cache_dir().join(format!("epg_{:016x}.xml", crate::cache::url_hash(url)))
}

fn epg_needs_refresh(url: &str) -> bool {
    let path = epg_cache_path(url);
    match std::fs::metadata(&path).and_then(|m| m.modified()) {
        Ok(modified) => match modified.elapsed() {
            Ok(elapsed) => elapsed.as_secs() > 3600,
            Err(_) => true,
        },
        Err(_) => true,
    }
}

fn fetch_epg(url: &str) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let resp = client.get(url).send()?;
    if !resp.status().is_success() {
        anyhow::bail!("EPG fetch returned HTTP {}", resp.status());
    }
    let bytes = resp.bytes()?;

    // Detect gzip by magic bytes
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        use std::io::Read;
        let mut decoder = flate2::read::GzDecoder::new(&bytes[..]);
        let mut s = String::new();
        decoder.read_to_string(&mut s)?;
        Ok(s)
    } else {
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

/// Parse XMLTV timestamp format "YYYYMMDDHHMMSS ±HHMM" into UTC unix timestamp.
/// Uses Howard Hinnant's days_from_civil algorithm for correct leap-year handling.
pub fn parse_xmltv_timestamp(s: &str) -> u64 {
    let s = s.trim();
    if s.len() < 14 {
        return 0;
    }

    let year: i64 = s[0..4].parse().unwrap_or(0);
    let month: u64 = s[4..6].parse().unwrap_or(0);
    let day: u64 = s[6..8].parse().unwrap_or(0);
    let hour: u64 = s[8..10].parse().unwrap_or(0);
    let minute: u64 = s[10..12].parse().unwrap_or(0);
    let second: u64 = s[12..14].parse().unwrap_or(0);

    fn days_from_civil(y: i64, m: u64, d: u64) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
        let yoe = (y - era * 400) as u64;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe as i64 - 719468
    }

    let days = days_from_civil(year, month, day);
    let unix_local = days as u64 * 86400 + hour * 3600 + minute * 60 + second;

    // Parse timezone offset at position 15: "±HHMM"
    let tz_offset_secs: i64 = if s.len() >= 20 {
        let sign: i64 = if s.as_bytes()[15] == b'-' { -1 } else { 1 };
        let h: i64 = s[16..18].parse().unwrap_or(0);
        let m: i64 = s[18..20].parse().unwrap_or(0);
        sign * (h * 3600 + m * 60)
    } else {
        0
    };

    (unix_local as i64 - tz_offset_secs) as u64
}

pub(crate) fn format_time(ts: u64) -> String {
    let secs_in_day = ts % 86400;
    let h = secs_in_day / 3600;
    let m = (secs_in_day % 3600) / 60;
    format!("{h:02}:{m:02}")
}

pub fn parse_xmltv(xml: &str) -> Result<EpgData> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let window_start = now.saturating_sub(7 * 86400);
    let window_end = now + 7 * 86400;

    let mut data: EpgData = HashMap::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut current_channel: Option<String> = None;
    let mut current_start: u64 = 0;
    let mut current_stop: u64 = 0;
    let mut current_title: Option<String> = None;
    let mut current_desc: Option<String> = None;
    let mut in_title = false;
    let mut in_desc = false;
    let mut in_programme = false;

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"programme" => {
                    in_programme = true;
                    current_title = None;
                    current_desc = None;
                    current_channel = None;
                    current_start = 0;
                    current_stop = 0;

                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"start" => {
                                if let Ok(v) = attr.unescape_value() {
                                    current_start = parse_xmltv_timestamp(&v);
                                }
                            }
                            b"stop" => {
                                if let Ok(v) = attr.unescape_value() {
                                    current_stop = parse_xmltv_timestamp(&v);
                                }
                            }
                            b"channel" => {
                                if let Ok(v) = attr.unescape_value() {
                                    current_channel = Some(v.trim().to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                }
                b"title" if in_programme => in_title = true,
                b"desc" if in_programme => in_desc = true,
                _ => {}
            },
            Ok(Event::Text(e)) => {
                if let Ok(raw) = std::str::from_utf8(&e) {
                    let text = quick_xml::escape::unescape(raw)
                        .unwrap_or(std::borrow::Cow::Borrowed(raw))
                        .into_owned();
                    if in_title && current_title.is_none() {
                        current_title = Some(text);
                    } else if in_desc && current_desc.is_none() {
                        current_desc = Some(text);
                    }
                }
            }
            Ok(Event::End(ref e)) => match e.name().as_ref() {
                b"title" => in_title = false,
                b"desc" => in_desc = false,
                b"programme" => {
                    if in_programme {
                        if let (Some(ch), Some(title)) =
                            (current_channel.take(), current_title.take())
                        {
                            if current_start >= window_start && current_start <= window_end {
                                data.entry(ch).or_default().push(Programme {
                                    title,
                                    desc: current_desc.take(),
                                    start: current_start,
                                    stop: current_stop,
                                });
                            }
                        }
                        in_programme = false;
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    for progs in data.values_mut() {
        progs.sort_by_key(|p| p.start);
    }

    Ok(data)
}

pub fn now_and_next<'a>(
    data: &'a EpgData,
    tvg_id: &str,
) -> (Option<&'a Programme>, Option<&'a Programme>) {
    // Normalise a tvg-id for loose matching: lowercase and strip trailing
    // quality suffix like "@SD", "@HD", "@FHD" (iptv-org M3U convention).
    fn normalise(s: &str) -> String {
        let stripped = if let Some(at) = s.rfind('@') {
            &s[..at]
        } else {
            s
        };
        stripped.to_lowercase()
    }

    // 1. Exact match
    // 2. Case-insensitive match
    // 3. Match after stripping @suffix from both sides
    let progs = if let Some(p) = data.get(tvg_id) {
        p
    } else {
        let id_lower = tvg_id.to_lowercase();
        let id_norm = normalise(tvg_id);
        match data
            .iter()
            .find(|(k, _)| k.to_lowercase() == id_lower || normalise(k) == id_norm)
        {
            Some((_, p)) => p,
            None => return (None, None),
        }
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let now_idx = progs.iter().position(|p| p.start <= now && p.stop > now);
    let now_prog = now_idx.map(|i| &progs[i]);
    let next_prog = now_idx
        .and_then(|i| progs.get(i + 1))
        .or_else(|| progs.iter().find(|p| p.start > now));

    (now_prog, next_prog)
}

pub fn load(url: &str) -> Option<EpgData> {
    let path = epg_cache_path(url);

    let xml = if !epg_needs_refresh(url) {
        std::fs::read_to_string(&path).ok()?
    } else {
        match fetch_epg(url) {
            Ok(content) => {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&path, &content);
                content
            }
            Err(_) => std::fs::read_to_string(&path).ok()?,
        }
    };

    parse_xmltv(&xml).ok()
}
