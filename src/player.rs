use anyhow::{Result, bail};
use std::process::Command;

pub fn check_installed() -> Result<()> {
    match Command::new("mpv").arg("--version").output() {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!("mpv is not installed. Install it with: brew install mpv")
        }
        Err(e) => bail!("Failed to check mpv: {e}"),
    }
}

pub fn play(url: &str, channel_name: &str) -> Result<()> {
    let status = Command::new("mpv")
        .args([
            "--no-resume-playback",
            &format!("--title={channel_name}"),
            "--really-quiet",
            url,
        ])
        .status();

    match status {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!("mpv is not installed. Install it with: brew install mpv")
        }
        Err(e) => bail!("Failed to launch mpv: {e}"),
    }
}
