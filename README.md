# MusicZero

<p align="center">
  <a href="README.md"><img src="https://img.shields.io/badge/Language-English-lightgrey?style=for-the-badge" alt="English" /></a>
  <a href="README_RU.md"><img src="https://img.shields.io/badge/Язык-Русский-red?style=for-the-badge" alt="Русская версия" /></a>
</p>

MusicZero is a lightweight modular music player written in Rust featuring a unified CLI and controller **`mz`**:

- **`mz`** — unified CLI tool. Launches any provider **in one command with system tray included** and controls playback on the fly without extra tools.
- **`YMZ`** — Yandex Music module ("My Vibe" stream, account playlists).
- **`YouMZ`** — YouTube Music module ("My Supermix", custom playlists, library).
- **`MCZ (Music Core Zero)`** — shared core: audio engine (rodio/symphonia), MPRIS v2, queue, configuration, and D-Bus StatusNotifierItem tray menu (ksni).

No browser required. The tray icon spawns automatically along with the player daemon, offering playlist selection and Yandex Music "My Vibe" tuning.

---

## Quickstart

### Launching (Daemon + Tray in one command):

```sh
mz ymz        # Start Yandex Music (with tray)
mz youmz      # Start YouTube Music (with tray)
mz all        # Start both players concurrently
```

Pass `--no-tray` to run without a tray icon (e.g. on headless servers).

### Playback control with `mz`:

```sh
mz toggle     # Play / Pause (also: mz play, mz pause, mz pp)
mz next       # Next track (also: mz skip)
mz prev       # Previous track
mz stop       # Stop playback
mz status     # Print active service, playback status, and current track
```

*The CLI automatically discovers the active running player. If both are running, specify a target: `mz next ymz` or `mz pause youmz`.*

### Playlists & Tuning:

```sh
mz playlists        # List available playlists
mz playlist 2       # Switch to playlist #2
mz wave             # Show Yandex Music Wave settings (mood/diversity/language)
mz wave mood calm   # Set wave mood: calm
mz login            # Log in to YouTube Music
```

---

## Installation

Install Rust via [rustup](https://rustup.rs), then install build dependencies for your distro:

| Distribution | Dependencies |
| --- | --- |
| Debian, Ubuntu, Linux Mint | `sudo apt install build-essential pkg-config libasound2-dev libsqlite3-dev` |
| Fedora | `sudo dnf install gcc pkgconf-pkg-config alsa-lib-devel sqlite-devel` |
| Arch Linux, Manjaro | `sudo pacman -S base-devel pkgconf alsa-lib sqlite` |
| openSUSE | `sudo zypper install gcc pkg-config alsa-devel sqlite3-devel` |
| Alpine Linux | `sudo apk add build-base pkgconf alsa-lib-dev sqlite-dev` |

### Build and Install:

Install all binaries (`mz`, `ymz`, `youmz`, `ymz-tray`, `youmz-tray`) into `~/.local/bin`:

```sh
./install.sh
```

Or build manually via Cargo:

```sh
cargo build --workspace --release --features ymz/tray,youmz/tray,mz/tray
```

Or install `mz` directly with Cargo:

```sh
cargo install --git https://github.com/BBQQYT/MusicZero.git mz --features tray --locked
```

---

## Media Key Bindings (Sway / i3 / Hyprland)

`mz` makes binding media keys seamless:

```ini
# Sway / i3
bindsym XF86AudioPlay exec mz toggle
bindsym XF86AudioNext exec mz next
bindsym XF86AudioPrev exec mz prev
bindsym XF86AudioStop exec mz stop
```

---

## License

MusicZero is distributed under the MIT License.
