"""
ipbeeldbuis — IPTV stream picker TUI.

The Rust binary is downloaded from GitHub Releases on first use and cached
next to this file. No compilation required.
"""

import os
import platform
import stat
import sys
import tarfile
import tempfile
import urllib.request

VERSION = "0.1.1"
REPO = "stanlocht/ipbeeldbuis"


def _binary_path() -> str:
    here = os.path.dirname(os.path.abspath(__file__))
    name = "ipbeeldbuis.exe" if sys.platform == "win32" else "ipbeeldbuis"
    return os.path.join(here, name)


def _detect_target() -> str:
    machine = platform.machine().lower()
    system = platform.system().lower()

    if system == "darwin":
        os_name = "macos"
    elif system == "linux":
        os_name = "linux"
    else:
        raise RuntimeError(
            f"Unsupported platform: {system}. "
            "Install manually from https://github.com/stanlocht/ipbeeldbuis/releases"
        )

    if machine in ("arm64", "aarch64"):
        arch = "arm64"
    elif machine == "x86_64":
        arch = "x86_64"
    else:
        raise RuntimeError(
            f"Unsupported architecture: {machine}. "
            "Install manually from https://github.com/stanlocht/ipbeeldbuis/releases"
        )

    return f"{os_name}-{arch}"


def _download_binary() -> None:
    target = _detect_target()
    filename = f"ipbeeldbuis-{target}.tar.gz"
    url = f"https://github.com/{REPO}/releases/download/v{VERSION}/{filename}"

    print(f"ipbeeldbuis: downloading binary for {target}...", file=sys.stderr)

    with tempfile.TemporaryDirectory() as tmp:
        archive = os.path.join(tmp, filename)
        try:
            urllib.request.urlretrieve(url, archive)
        except Exception as e:
            raise RuntimeError(
                f"Failed to download binary from {url}\n"
                "Check https://github.com/stanlocht/ipbeeldbuis/releases"
            ) from e

        with tarfile.open(archive, "r:gz") as tar:
            tar.extractall(tmp)

        src = os.path.join(tmp, "ipbeeldbuis")
        if not os.path.exists(src):
            raise RuntimeError(f"Binary not found in archive: {filename}")

        dest = _binary_path()
        import shutil
        shutil.copy2(src, dest)
        os.chmod(dest, os.stat(dest).st_mode | stat.S_IEXEC | stat.S_IXGRP | stat.S_IXOTH)

    print("ipbeeldbuis: binary ready.", file=sys.stderr)


def main() -> None:
    binary = _binary_path()
    if not os.path.isfile(binary):
        _download_binary()
    os.execv(binary, [binary] + sys.argv[1:])
