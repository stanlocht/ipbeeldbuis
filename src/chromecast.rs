use anyhow::{Result, anyhow};
use std::time::{Duration, Instant};

pub struct CastDevice {
    pub name: String,
    pub addr: String,
    pub port: u16,
}

/// Saved state from a successful cast — enough to send control commands later.
pub struct CastSession {
    pub device_name: String,
    pub addr: String,
    pub port: u16,
    pub transport_id: String,
    pub media_session_id: i32,
}

pub enum StreamFormat {
    Hls,
    MpegTs,
    Unknown,
}

/// Discover Chromecast devices on the local network via mDNS.
pub fn discover_devices(timeout_secs: u64) -> Vec<CastDevice> {
    let Ok(mdns) = mdns_sd::ServiceDaemon::new() else {
        return Vec::new();
    };
    let Ok(receiver) = mdns.browse("_googlecast._tcp.local.") else {
        return Vec::new();
    };

    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let mut devices: Vec<CastDevice> = Vec::new();
    let mut seen_addrs = std::collections::HashSet::new();

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match receiver.recv_timeout(remaining) {
            Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                let name = info
                    .get_property_val_str("fn")
                    .unwrap_or("Unknown Chromecast")
                    .to_string();
                let addr = info
                    .get_addresses_v4()
                    .into_iter()
                    .next()
                    .map(|a| a.to_string());
                if let Some(addr) = addr
                    && seen_addrs.insert(addr.clone())
                {
                    devices.push(CastDevice {
                        name,
                        addr,
                        port: info.get_port(),
                    });
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    let _ = mdns.stop_browse("_googlecast._tcp.local.");
    devices.sort_by(|a, b| a.name.cmp(&b.name));
    devices
}

/// Detect the stream format from a URL using URL heuristics then a HEAD request.
pub fn detect_format(url: &str) -> StreamFormat {
    let lower = url.to_lowercase();

    // Tier 1: URL heuristics
    if lower.contains(".m3u8") || lower.contains("/hls/") {
        return StreamFormat::Hls;
    }
    if lower.ends_with(".ts") || lower.contains("mpeg-ts") {
        return StreamFormat::MpegTs;
    }

    // Tier 2: HEAD request Content-Type
    if let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        && let Ok(resp) = client.head(url).send()
        && let Some(ct) = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
    {
        let ct = ct.to_lowercase();
        if ct.contains("mpegurl") || ct.contains("m3u8") {
            return StreamFormat::Hls;
        }
        if ct.contains("mp2t") || ct.contains("mpegts") {
            return StreamFormat::MpegTs;
        }
    }

    StreamFormat::Unknown
}

/// Cast a stream URL to a Chromecast device. Returns a session for future control.
pub fn cast(device: &CastDevice, url: &str, _title: &str) -> Result<CastSession> {
    use rust_cast::CastDevice as RustCastDevice;
    use rust_cast::channels::media::{Media, StreamType};
    use rust_cast::channels::receiver::CastDeviceApp;

    let dev = RustCastDevice::connect_without_host_verification(device.addr.as_str(), device.port)
        .map_err(|e| anyhow!("Connect failed: {e}"))?;

    dev.connection
        .connect("receiver-0")
        .map_err(|e| anyhow!("Receiver connect failed: {e}"))?;

    dev.heartbeat
        .ping()
        .map_err(|e| anyhow!("Heartbeat failed: {e}"))?;

    let app = dev
        .receiver
        .launch_app(&CastDeviceApp::DefaultMediaReceiver)
        .map_err(|e| anyhow!("App launch failed: {e}"))?;

    dev.connection
        .connect(app.transport_id.as_str())
        .map_err(|e| anyhow!("App transport connect failed: {e}"))?;

    let status = dev
        .media
        .load(
            app.transport_id.as_str(),
            app.session_id.as_str(),
            &Media {
                content_id: url.to_string(),
                stream_type: StreamType::Live,
                content_type: "application/x-mpegURL".to_string(),
                metadata: None,
                duration: None,
            },
        )
        .map_err(|e| anyhow!("Media load failed: {e}"))?;

    let media_session_id = status
        .entries
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("No media session in load response"))?
        .media_session_id;

    Ok(CastSession {
        device_name: device.name.clone(),
        addr: device.addr.clone(),
        port: device.port,
        transport_id: app.transport_id,
        media_session_id,
    })
}

/// Pause the active cast session.
pub fn pause_session(session: &CastSession) -> Result<()> {
    let dev = connect_session(session)?;
    dev.media
        .pause(session.transport_id.clone(), session.media_session_id)
        .map_err(|e| anyhow!("Pause failed: {e}"))?;
    Ok(())
}

/// Resume a paused cast session.
pub fn resume_session(session: &CastSession) -> Result<()> {
    let dev = connect_session(session)?;
    dev.media
        .play(session.transport_id.clone(), session.media_session_id)
        .map_err(|e| anyhow!("Resume failed: {e}"))?;
    Ok(())
}

/// Stop the active cast session entirely.
pub fn stop_session(session: &CastSession) -> Result<()> {
    let dev = connect_session(session)?;
    dev.media
        .stop(session.transport_id.clone(), session.media_session_id)
        .map_err(|e| anyhow!("Stop failed: {e}"))?;
    Ok(())
}

// Pass addr/transport_id as owned Strings so the returned CastDevice<'static> is unconstrained
// by the session reference lifetime.
fn connect_session(session: &CastSession) -> Result<rust_cast::CastDevice<'static>> {
    let dev = rust_cast::CastDevice::connect_without_host_verification(
        session.addr.clone(),
        session.port,
    )
    .map_err(|e| anyhow!("Connect failed: {e}"))?;

    dev.connection
        .connect("receiver-0")
        .map_err(|e| anyhow!("Receiver connect failed: {e}"))?;

    dev.connection
        .connect(session.transport_id.clone())
        .map_err(|e| anyhow!("Transport connect failed: {e}"))?;

    Ok(dev)
}
