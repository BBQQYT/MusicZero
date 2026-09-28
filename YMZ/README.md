# YMZ (Yandex Music Zero)

<p align="center">
  <a href="README.md"><img src="https://img.shields.io/badge/Language-English-blue?style=for-the-badge" alt="English" /></a>
  <a href="README_RU.md"><img src="https://img.shields.io/badge/Язык-Русский-lightgrey?style=for-the-badge" alt="Русский" /></a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Language-Rust-dea584?style=for-the-badge&logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/Platform-Linux-1793d1?style=for-the-badge&logo=linux" alt="Linux" />
  <img src="https://img.shields.io/badge/Memory-~15--20MB_RSS-brightgreen?style=for-the-badge" alt="RAM" />
  <img src="https://img.shields.io/badge/License-MIT-blue?style=for-the-badge" alt="License" />
</p>

> **Shared core:** this project now uses the sibling `../MCZ` crate (Music Core Zero). Keep `MCZ`, `YMZ`, and `YouMZ` in one parent directory. Build the daemon and tray with `cargo build --release --features tray`. Linux tray menus require a session D-Bus and a StatusNotifierItem host.

---

An ultra-lightweight headless client for **Yandex Music**, written in Rust. It plays **"My Wave" ("Моя волна")** and library playlists, avoids heavy Electron/Chromium dependencies, integrates natively via **MPRIS v2**, and is controlled using standard Linux system tools (`playerctl`, Waybar, desktop widgets, media keys).

Yes, written by AI, not hiding it!

There is also an equivalent for YouTube Music — [YouMZ!](https://github.com/BBQQYT/YouMZ)

---

### Features

* **Zero-bloat:** Memory consumption stays within **15–20 MB RSS** (compared to ~700 MB for the official desktop client).
* **Full MPRIS v2 Support:**
  * Real-time track title, artist name, and album artwork.
  * Timeline synchronization: duration (`mpris:length`) and playback progress (`Position`).
  * Full seeking and scrubbing support (`Seek`, `SetPosition`).
* **Security:** Isolated OAuth token storage in `~/.config/ymz/token` with `600` permissions (the token is never exposed in process lists or logs).
* **Network Resilience:** Automatic retries with exponential backoff on network errors or brief disconnections.
* **Accurate Rotor Feedback:** Sends proper playback telemetry (`trackStarted`, `trackFinished`, `skip`) to Yandex recommendation algorithms.
* **Universal Audio Backend:** Seamlessly works with PipeWire, PulseAudio, and pure ALSA across all Linux distributions.

---

### System Dependencies

Building requires ALSA development headers and `pkg-config`:

* **Arch Linux / Manjaro / CachyOS:**
  ```bash
  sudo pacman -S alsa-lib pkgconf base-devel
  ```

* **Ubuntu / Debian / Linux Mint / Pop!_OS:**
  ```bash
  sudo apt install libasound2-dev pkg-config build-essential
  ```

* **Fedora / RHEL / AlmaLinux:**
  ```bash
  sudo dnf install alsa-lib-devel pkgconf-pkg-config gcc
  ```

* **openSUSE (Tumbleweed / Leap):**
  ```bash
  sudo zypper install alsa-devel pkg-config gcc
  ```

* **Void Linux:**
  ```bash
  sudo xbps-install -S alsa-lib-devel base-devel
  ```

---

### Building and Installation

1. **Clone the repository:**
   ```bash
   mkdir music-zero && cd music-zero
   git clone https://github.com/BBQQYT/ymz.git YMZ
   git clone https://github.com/BBQQYT/youmz.git YouMZ
   # Place the MCZ crate beside YMZ and YouMZ
   cd YMZ
   ```

2. **Build the release binary:**
   ```bash
   cargo build --release --features tray
   ```

3. **(Optional) Install system-wide:**
   ```bash
   sudo install -Dm755 target/release/ymz /usr/local/bin/ymz
   ```

---

### Configuration

Save your Yandex Music OAuth token to the configuration file and restrict access permissions:

```bash
mkdir -p ~/.config/ymz
echo "YOUR_TOKEN" > ~/.config/ymz/token
chmod 600 ~/.config/ymz/token
```

> Passing the token via the `YM_TOKEN` environment variable is also supported.

---

### Autostart

#### Option 1: systemd user service (recommended)

Create `~/.config/systemd/user/ymz.service`:

```ini
[Unit]
Description=Yandex Music Zero Daemon
After=pipewire.service wireplumber.service sound.target

[Service]
Type=simple
ExecStart=/usr/local/bin/ymz
Restart=always
RestartSec=3

[Install]
WantedBy=default.target
```

Enable and start the service:

```bash
systemctl --user daemon-reload
systemctl --user enable --now ymz.service
```

#### Option 2: Without systemd (Hyprland / Sway / AwesomeWM / i3)

Add the binary call to your window manager or compositor configuration:

* **Hyprland** (`hyprland.conf`):
  ```ini
  exec-once = ymz
  ```

* **Sway** (`config`):
  ```ini
  exec ymz
  ```

* **AwesomeWM** (`rc.lua`):
  ```lua
  awful.spawn.with_shell("ymz")
  ```

* **i3** (`config`):
  ```ini
  exec --no-startup-id ymz
  ```

---

### Controls & Usage

```bash
# Play / Pause toggle
playerctl -p ymz play-pause

# Next track (sends skip feedback)
playerctl -p ymz next

# Seek forward / backward by 10 seconds
playerctl -p ymz position 10+
playerctl -p ymz position 10-

# Jump to a specific second (e.g. 1:15)
playerctl -p ymz position 75

# Display current metadata and playback status
playerctl -p ymz metadata
```

#### Waybar Integration (`config.jsonc`):

```jsonc
"mpris": {
    "player": "ymz",
    "format": "{player_icon} {artist} — {title} [{position}/{length}]",
    "player-icons": {
        "default": "▶",
        "playing": "▶",
        "paused": "⏸"
    }
}
```

---

### License

This project is licensed under the [MIT License](LICENSE).
