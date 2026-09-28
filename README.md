# MusicZero

MusicZero is a lightweight Linux music player made of two service adapters and a shared Rust core:

- **YMZ** plays Yandex Music, including My Wave and the account's playlists.
- **YouMZ** plays YouTube Music mixes, liked tracks, and saved playlists.
- **MCZ (Music Core Zero)** provides shared playback, MPRIS controls, queue handling, configuration paths, and the optional system tray.

The player runs without a browser window. The tray lets you switch playlists; YMZ also exposes My Wave mood, diversity, and language settings.

[Русская версия](README_RU.md)

## Install from source

Install Rust with [rustup](https://rustup.rs), then install the native build dependencies for your distribution.

| Distribution | Build dependencies |
| --- | --- |
| Debian, Ubuntu, Linux Mint | `sudo apt install build-essential pkg-config libasound2-dev libsqlite3-dev` |
| Fedora | `sudo dnf install gcc pkgconf-pkg-config alsa-lib-devel sqlite-devel` |
| Arch Linux, Manjaro | `sudo pacman -S base-devel pkgconf alsa-lib sqlite` |
| openSUSE | `sudo zypper install gcc pkg-config alsa-devel sqlite3-devel` |
| Alpine Linux | `sudo apk add build-base pkgconf alsa-lib-dev sqlite-dev` |

Build the daemons and both tray programs:

```sh
cargo build --workspace --release --features ymz/tray,youmz/tray
```

Or install all four executables under `~/.local/bin`:

```sh
./install.sh
```

After publishing a new version, Cargo can build and install both adapters in one command:

```sh
cargo install --git https://github.com/BBQQYT/MusicZero.git ymz youmz --features ymz/tray,youmz/tray --locked
```

`install.sh` also accepts a custom prefix, for example `MUSICZERO_PREFIX=/usr/local ./install.sh`.

## Runtime requirements

- A Linux audio output supported by ALSA/CPAL. PipeWire and PulseAudio installations usually provide an ALSA compatibility layer.
- A user D-Bus session for MPRIS controls.
- A StatusNotifierItem-compatible panel or tray host to show the optional tray icons.
- `yt-dlp` on `PATH` for YouMZ audio downloads.
- A Yandex Music OAuth token for YMZ and an authenticated YouTube Music session for YouMZ.

No particular init system is required. The optional tray programs communicate with their player over the user D-Bus session.

## Configure YMZ

Put the Yandex Music OAuth token in the XDG configuration directory and restrict access to it:

```sh
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/ymz"
printf '%s\n' 'YOUR_YANDEX_TOKEN' > "${XDG_CONFIG_HOME:-$HOME/.config}/ymz/token"
chmod 600 "${XDG_CONFIG_HOME:-$HOME/.config}/ymz/token"
```

`YM_TOKEN` can be used instead. YMZ starts with My Wave. The tray can select another playlist from the account and change My Wave settings.

## Configure YouMZ

The first run can use the login flow:

```sh
youmz login
```

You can also put an authenticated YouTube Music cookie in `${XDG_CONFIG_HOME:-$HOME/.config}/youmz/cookie`, with file permissions `600`. An optional proxy can be set in the `proxy` file or through `YOUMZ_PROXY`, for example `socks5h://127.0.0.1:2080`.

YouMZ starts with **My Mix** (`RDMM`). Use the tray to select liked music or a saved playlist. The selected playlist is saved for the next start.

## Run

Start one player and, if desired, its tray program in the same desktop session:

```sh
ymz
ymz-tray
```

or:

```sh
youmz
youmz-tray
```

The tray is optional. Without it, use any MPRIS client, such as `playerctl`:

```sh
playerctl -p ymz play-pause
playerctl -p youmz next
```

## Configuration locations

On Linux, MusicZero follows `XDG_CONFIG_HOME` and `XDG_CACHE_HOME`. If those variables are unset, it uses `~/.config` and `~/.cache`.

## Windows

Windows support is incomplete. MCZ has portable profile paths and shutdown handling, but the player adapters still depend on Linux D-Bus and the native tray and media controls are not implemented for Windows.

## License

MusicZero is distributed under the MIT License. See the license files in the project directories.
