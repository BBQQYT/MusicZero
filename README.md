# MusicZero

<p align="center">
  <a href="README.md"><img src="https://img.shields.io/badge/Language-English-lightgrey?style=for-the-badge" alt="English" /></a>
  <a href="README_RU.md"><img src="https://img.shields.io/badge/Язык-Русский-red?style=for-the-badge" alt="Русская версия" /></a>
</p>

MusicZero is a lightweight modular music player written in Rust featuring a unified CLI and controller **`mz`**:

- **`mz`** — unified CLI tool. Launches either provider and controls playback. On Linux it also starts a system tray icon.
- **`YMZ`** — Yandex Music module ("My Vibe" stream, account playlists).
- **`YouMZ`** — YouTube Music module ("My Supermix", custom playlists, library).
- **`MCZ (Music Core Zero)`** — shared core: audio engine (rodio/symphonia), MPRIS v2, queue, configuration, and D-Bus StatusNotifierItem tray menu (ksni).

No browser required for playback. On Linux the tray icon starts with the player and offers playlist selection and Yandex Music "My Vibe" tuning. On Windows, `mz` uses a local named pipe for playback and playlist commands; there is no tray or native media key integration yet.

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
mz stop       # Stop playback
mz status     # Print active service, playback status, and current track
```

*The CLI automatically discovers the active running player. If both are running, specify a target: `mz next ymz` or `mz pause youmz`. Previous track is not supported yet.*

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

### Windows

Download `musiczero-windows-x64.zip` from a tagged GitHub release, extract its three `.exe` files to one directory, and run them from PowerShell. Alternatively, build with `cargo build --workspace --release --no-default-features --locked` using the MSVC Rust toolchain. Put the Yandex token in `%APPDATA%\ymz\token` (or set `YM_TOKEN`). Run `mz.exe login` for YouTube Music authentication. Keep `yt-dlp.exe` on `PATH` for YouMZ audio downloads. Start a player with `mz.exe ymz` or `mz.exe youmz`, then use another terminal for commands such as `mz.exe status` and `mz.exe next`.

The Windows package contains `mz.exe`, `ymz.exe`, and `youmz.exe`. The Linux tray and MPRIS are available only on Linux. `mz prev` is currently unsupported.

### Linux

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

### Releases

Every successful push to `main` or `master` builds Linux and Windows archives and publishes an automatic prerelease. Pushing a tag beginning with `v` (for example `v0.2.1`) publishes a versioned release after both builds succeed.

---

## Media Key Bindings (Sway / i3 / Hyprland)

`mz` makes binding media keys seamless:

```ini
# Sway / i3
bindsym XF86AudioPlay exec mz toggle
bindsym XF86AudioNext exec mz next
bindsym XF86AudioStop exec mz stop
```

---

## License

MusicZero is distributed under the MIT License.
