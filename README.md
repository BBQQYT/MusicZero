# MusicZero

`mz` is one audio player with replaceable service modules. Playback, the queue, CLI control, and Linux MPRIS live in the host. Providers are executable files in folders beside `mz`:

```text
musiczero/
├── mz                    # mz.exe on Windows
└── modules/
    ├── ymz/module.json
    ├── ymz/ymz-module
    ├── youmz/module.json
    └── youmz/youmz-module
```

Only one player runs. The host starts a provider process for a request, receives track metadata as JSON or encoded audio on stdout, then lets the provider exit. Audio is stored in a temporary file and decoded by the host, keeping track size out of the host's RAM usage. Switching providers clears the queue and keeps the same audio output and control endpoint.

## Use

**Linux x86_64 — one command** (requires `curl` and Python 3.8+):

```sh
curl -fsSL https://raw.githubusercontent.com/BBQQYT/MusicZero/main/install-linux.py | python3
```

The installer finds the newest published release, verifies its SHA-256 digest, and places `mz`, YMZ, and YouMZ under `~/.local/bin`. Running it again updates the official files while preserving custom module folders. If `~/.local/bin` is outside `PATH`, it prints the full command to run.

For manual installation, download and extract the complete `musiczero` folder from [Releases](https://github.com/BBQQYT/MusicZero/releases). YouMZ requires `yt-dlp` on `PATH`. Linux needs an audio device and a user D-Bus session for MPRIS and the tray.

```sh
mz modules
mz start ymz          # also: mz ymz
mz switch youmz       # change the running player's provider
mz status
mz toggle
mz next
mz playlists
mz playlist 2         # number or playlist ID
mz settings
mz set mood calm      # example YMZ setting
mz quit
```

Keep the player in the first terminal and issue control commands in another. On Windows, run `./mz.exe` from the extracted directory in PowerShell. The Linux build has one tray icon when the desktop supports StatusNotifierItem. Windows control uses the CLI; a native tray and media keys are not available yet.

YMZ reads its OAuth token from `%APPDATA%\ymz\token` on Windows or `~/.config/ymz/token` on Linux; `YM_TOKEN` also works. Run `mz login youmz` for YouTube Music and follow the console link. YouMZ also accepts `YOUMZ_COOKIE`.

## Write a module

A module is an ordinary command line program. Write it in any language and compile it separately for Linux and Windows; the protocol is the same on both systems. It does not need Rust or MusicZero source code.

Create `modules/<id>/module.json` and place the executable beside it:

```json
{
  "protocol": 1,
  "id": "demo",
  "name": "My Service",
  "binary": "demo-module",
  "default_playlist": "main"
}
```

The folder name must match `id`. Set `binary` without `.exe`; the host adds that extension on Windows. IDs and binary names may contain ASCII letters, digits, `_`, and `-`. The host looks for `modules` next to its executable. `MZ_MODULES_DIR` overrides this for development.

The host starts the program afresh for each command. Write only protocol data to stdout. Write errors to stderr and exit with a nonzero code.

| Command | stdout |
| --- | --- |
| `info` | `{"protocol":1,"id":"demo","name":"My Service","default_playlist":"main"}` |
| `playlists` | `{"playlists":[{"id":"main","name":"Main"}]}` |
| `tracks main` | `{"tracks":[{"id":"track-1","title":"Song","artist":"Artist","duration_ms":120000,"art_url":""}]}` |
| `audio track-1` | Encoded MP3, M4A, or WAV bytes only |
| `settings` | `{"settings":{}}` or a settings object |
| `set-setting key value` | Updated `settings` object; optional without settings |
| `login` | Optional interactive login; inherits terminal input and output |

`info`, `playlists`, `tracks`, `audio`, and `settings` are required. Return tracks in playback order. The host passes the track ID back unchanged. The module handles network requests, credentials, and audio retrieval; `mz` handles decoding, the queue, playback controls, and provider switching.

A complete dependency free Rust example is in [examples/module-template](examples/module-template). It generates a WAV tone. Build it with `cargo build --release --manifest-path examples/module-template/Cargo.toml`, then copy the manifest and resulting executable into `modules/demo`.

## Build

```sh
cargo build --workspace --release --locked
```

Building on Linux needs `pkg-config` and ALSA development headers. `./install.sh` builds from source and installs `mz` and its `modules` folder under `~/.local/bin`. Successful `main` builds publish prereleases; `v*` tags publish versioned Linux and Windows archives.
