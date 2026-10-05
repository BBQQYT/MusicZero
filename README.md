# MusicZero

[Русский README](README_RU.md)

MusicZero is one audio player with replaceable providers: **Yandex Music**, **YouTube Music**, **a local music folder**, and **Icecast radio**. `mz` owns audio output, the queue, playback controls, next-track preloading and Linux desktop integration. Providers run as separate executables.

## Installation

### Linux x86_64

Requires `curl` and Python 3.8+:

```sh
curl -fsSL https://raw.githubusercontent.com/BBQQYT/MusicZero/main/install-linux.py | python3
```

The installer chooses the newest published release containing a Linux archive, including automatic prereleases, verifies its GitHub SHA-256 digest and installs into `~/.local/bin`. Run it again to update. Custom module folders and user settings are preserved. Older release archives may contain only YMZ and YouMZ.

The installer opens a terminal wizard automatically, including when piped from `curl`. Use Up/Down (or j/k), Enter to confirm, and Esc to cancel. It lets you choose the prefix, optionally add `mz` to Bash/Zsh/Fish PATH, install missing runtime packages using apt/dnf/pacman/zypper (sudo may prompt), configure Local and Icecast, and log into Yandex/YouTube Music. Advanced settings include FFmpeg paths, SoundFont, radio proxy/TLS/buffering and Yandex mix preferences. YouTube login needs Firefox or Chromium/Chrome and a graphical session. Package installation and shell edits are optional; configuration can be retried without reinstalling.

```sh
# Reopen setup for an existing installation
curl -fsSL https://raw.githubusercontent.com/BBQQYT/MusicZero/main/install-linux.py | python3 - --tui --setup-only
# Automation: install binaries without prompts
curl -fsSL https://raw.githubusercontent.com/BBQQYT/MusicZero/main/install-linux.py | python3 - --non-interactive
```

`--tui` requires a controlling terminal (`/dev/tty`) with `TERM` set; without a terminal the default is install-only. The wizard uses Python's standard `curses` module and requires no dialog package. Missing packages are checked for the selected source, not installed automatically. Pressing Esc leaves completed installation/settings intact. During updates all files are staged before replacement and existing files are restored if replacement fails; this does not provide a transaction across power loss or concurrent installer runs. User settings and custom modules are preserved. A running player should be restarted after an update.

If the installer reports that its directory is outside `PATH`, add it to your shell configuration:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

`MUSICZERO_PREFIX` changes the installation prefix; binaries go in `<prefix>/bin`. `MUSICZERO_TAG` selects a particular release. When piping the installer into Python, set these variables on the Python command:

```sh
curl -fsSL https://raw.githubusercontent.com/BBQQYT/MusicZero/main/install-linux.py |
  MUSICZERO_PREFIX="$HOME/.local" MUSICZERO_TAG="build-123" python3
```

Replace `build-123` with an existing release tag. After updating, restart any running player to load the new binaries.

### Manual installation and Windows

Download the appropriate archive from [Releases](https://github.com/BBQQYT/MusicZero/releases) and extract the **entire `musiczero` folder**, preserving the modules beside the host:

```text
musiczero/
├── mz                         # mz.exe on Windows
├── README.md
├── README_RU.md
└── modules/
    ├── ymz/                   # module.json + ymz-module
    ├── youmz/                 # module.json + youmz-module
    ├── local/                 # module.json + local-module
    └── icecast/               # module.json + icecast-module
```

Provider executables also end in `.exe` on Windows. In PowerShell, use `./mz.exe` instead of `mz`, for example:

```powershell
./mz.exe set local path "C:\Users\yourname\Music"
./mz.exe local
```

### Runtime requirements

| Component | Requirements |
| --- | --- |
| Host | Working audio output. Linux needs ALSA or a configured ALSA interface to PipeWire/PulseAudio |
| Linux tray/MPRIS | User D-Bus session; a desktop panel supporting StatusNotifierItem for the tray |
| YMZ | Yandex Music account and a valid OAuth token |
| YouMZ | YouTube Music browser login; Firefox, LibreWolf, Chromium or Chrome; `yt-dlp` on PATH for audio |
| Local | `ffmpeg` and `ffprobe` on PATH |
| Legacy tracker files | FFmpeg built with `libopenmpt` |
| MIDI | `fluidsynth` on PATH and a configured SF2/SF3 SoundFont, in addition to Local's FFmpeg requirements |
| Icecast | `ffmpeg` on PATH and an HTTP(S) station mountpoint |

FFmpeg, yt-dlp, browsers and SoundFonts are not bundled with MusicZero. Local and Icecast can use configured executable paths instead of PATH. Linux desktop integration can be unavailable while CLI playback continues; MPRIS failures are logged. Windows currently uses CLI control, without a native tray or system media keys.

## Quick start and controls

Start the player in one terminal and keep it open; send commands from a second terminal. Starting another provider while the host is running switches the existing player instead of opening a second audio output.

```sh
mz modules
mz login ymz
mz ymz
```

Then, from the second terminal:

```sh
mz status
mz toggle
mz next
mz playlists
mz playlist 2
mz switch youmz
mz quit
```

| Command | Behavior |
| --- | --- |
| `mz` | Show current status if the host is running, otherwise help |
| `mz modules` | List installed providers |
| `mz start <id>` or `mz <id>` | Start a provider, or switch the running host to it |
| `mz switch <id>` | Switch the running host; clear the current track and queue |
| `mz login <id>` | Run that provider's interactive login |
| `mz status` | Show provider, playlist, current track and position/duration |
| `mz seek <time>` | Seek to seconds or MM:SS/HH:MM:SS; a leading +/− means relative offset |
| `mz play`, `mz pause`, `mz toggle` | Playback controls |
| `mz next` | Skip the current track; for radio, reconnect to the selected station |
| `mz stop` | Stop playback while keeping the host running |
| `mz quit` | Exit the host |
| `mz playlists` | List active provider's playlists/stations |
| `mz playlist <id\|number>` | Select an ID or a **1-based** number; `mz playlist` shows the selection |
| `mz config [id]` / `mz tui [id]` | Complete settings TUI with Russian/English selection; works while the player is stopped |
| `mz settings` | Read the active provider's settings |
| `mz settings <id>` | Read a provider's settings even before startup |
| `mz set <key> <value>` | Change a setting of the active provider |
| `mz set <id> <key> <value>` | Configure a named provider before or during playback |
| `mz help`, `mz version` | Show help/version |

Provider names are case-insensitive in CLI lookup. Quote paths, URLs and JSON values as needed. Local/Icecast settings changes reset the active provider's queue and loading state; an invalid setting returns an error. Other control and playlist commands require a running host.

Seeking works on downloaded finite audio from YMZ, YouMZ and Local, including while paused. It keeps the current track and preloaded next track. `status` shows the current position and decoded duration when available. The decoder's duration takes precedence over provider metadata; unknown duration does not force seeks back to zero. Targets are limited to the start/end when duration is known. Live Icecast radio cannot seek. Linux MPRIS Seek/SetPosition use the same handler and emit Seeked; SetPosition ignores stale track IDs.

```sh
mz seek 90       # absolute: 90 seconds
mz seek 1:30.5   # absolute: 90.5 seconds
mz seek +15     # forward 15 seconds
mz seek -10     # backward 10 seconds
```

## Settings menu

```sh
mz config          # All settings
mz config icecast  # Open a provider directly
```

The built-in menu does not require Python. Use arrows or j/k to select, Enter to open/save, Esc to go back or cancel input, Ctrl+U to clear a field, and Ctrl+C to exit. Paste, Unicode, scrolling and terminal resizing are supported. “Language / Язык” switches every menu and hint between Russian and English; the choice is saved in `mz/settings.json`.

The menu includes every `settings` field returned by installed modules, including new third-party fields. It covers all Local options, YMZ wave mood/variety/song language and token/login, YouMZ proxy/browser/session, playlists and all Icecast settings. Add, edit and remove stations through forms, including IDs, server URLs, mountpoints and credentials. Passwords and tokens are masked; editing another field preserves the existing password. Login opens the provider’s normal flow and returns to the menu.

Changes save on confirmation and are validated by the provider; errors appear inside the menu. Playlists can be selected before starting playback. Active Local/Icecast changes use IPC to discard obsolete downloads. Wave preferences require login and network access; menus and login remain available when the service is unavailable.

Player settings include the modules folder, log filter and temporary audio folder. Empty values restore defaults. `MZ_MODULES_DIR` and `RUST_LOG` override saved values; restart the player for these changes. The temporary audio folder applies to new downloads and provider processes. Config/cache paths and active environment overrides are also displayed.

## Yandex Music (YMZ)

```sh
mz login ymz
```

The module opens [ym-token.marshal.dev](https://ym-token.marshal.dev/), asks you to paste the OAuth token into the terminal, validates it against Yandex Music and saves it. If browser launch fails, open the displayed URL yourself; the token prompt still works.

```sh
mz ymz
mz playlists
mz playlist wave
mz settings ymz
mz set ymz mood calm
```

`wave` is the default playlist. Account playlists use IDs such as `playlist:12345`; choose an ID returned by `mz playlists`, or its list number. Wave settings expose `mood`, `diversity` and `language`; their values are accepted or rejected by the service.

The token lives in `~/.config/ymz/token` on Linux or `%APPDATA%\ymz\token` on Windows. A saved token takes precedence over `YM_TOKEN`; the variable is a fallback when no usable token file exists.

## YouTube Music (YouMZ)

```sh
mz login youmz
```

1. A supported browser opens with a separate temporary profile, initially without debugging flags.
2. Sign in and wait until your YouTube Music library appears.
3. **Close that browser window.** Keep the login command running.
4. The module reopens the temporary profile, reads the live session through the browser debugging interface, validates it and reports successful login.

It does not read your normal browser profile's cookie files. This flow does not require creating a Google OAuth application or configuring a TV/device OAuth client. Press Ctrl+C to cancel.

```sh
mz youmz
mz playlists
mz playlist RDMM
```

`RDMM` is the default personal mix. Playback requires `yt-dlp`; authentication alone does not install it. Audio cookies are passed through a temporary Netscape cookie file scoped to YouTube, rather than a Cookie header sent to every download URL. The file is removed when the request finishes normally.

| Configuration | Behavior |
| --- | --- |
| `YOUMZ_BROWSER` | Explicit browser executable path when auto-detection fails |
| `YOUMZ_SESSION` | Optional session string override; takes precedence over the saved session |
| `~/.config/youmz/session` | Saved session; `%APPDATA%\youmz\session` on Windows |
| `~/.config/youmz/proxy` / `YOUMZ_PROXY` | Optional proxy; a nonempty proxy file takes precedence over the environment variable |

YouMZ exposes `proxy`, `browser` and a masked `session` through `mz config youmz` and `mz settings youmz`. Use `mz set youmz <key> <value>` for scripts. A saved browser path is used unless `YOUMZ_BROWSER` overrides it. If Google rejects sign-in, check whether the account can sign in through a browser opened normally, then retry MusicZero's temporary-profile flow. Old OAuth-client error messages indicate outdated provider binaries; update the host **and** its modules.

## Local music folder

```sh
mz set local path "$HOME/Music"
mz settings local
mz local
```

Set `path` to an existing directory. `mz switch local` switches an already running host. The Local playlist is `all`. Scanning detects audio by content instead of using an extension whitelist, reads title/artist tags and falls back to filenames. It checks up to eight files concurrently and reuses metadata for files whose size and modification time are unchanged. On Unix, non-UTF-8 filenames keep distinct track IDs and are stored losslessly in the index; display titles replace invalid characters.

### Local settings

| Key | Default | Meaning |
| --- | --- | --- |
| `path` | empty | Existing music directory; quoted paths may contain spaces; `~/` is expanded by the module when HOME exists |
| `recursive` | `true` | Include subfolders |
| `hidden` | `false` | Include names starting with `.` |
| `shuffle` | `false` | Shuffle the library whenever the queue is refilled |
| `scan_limit` | `10000` | Maximum examined files/directories, 1–100000 |
| `probe_timeout` | `5` | Per-file probing limit, 1–30 seconds |
| `ffmpeg` | `ffmpeg` | FFmpeg executable name or path |
| `ffprobe` | `ffprobe` | ffprobe executable name or path |
| `fluidsynth` | `fluidsynth` | MIDI renderer executable name or path |
| `soundfont` | empty | Existing SF2/SF3 bank for MIDI |

```sh
mz set local recursive true
mz set local shuffle true
mz set local soundfont "/path/to/GeneralUser.sf2"
```

### Formats and limits

Format coverage depends on the installed FFmpeg build: FLAC, WAV, MP3/MP2, AAC/M4A/ALAC, Ogg/Vorbis/Opus, WMA, AIFF, AU, VOC, WavPack, APE, TTA, CAF, AC3, ADPCM and other supported formats. MOD/XM/S3M/IT and other tracker formats require `libopenmpt`. MIDI is detected from its header and rendered through FluidSynth with the configured SoundFont. Inspect your FFmpeg capabilities with `ffmpeg -demuxers` and `ffmpeg -decoders`.

**Support for literally every historical format is not guaranteed.** SID and specialized formats absent from FFmpeg/FluidSynth need additional decoders that are not integrated here. Corrupt, unsupported and DRM-protected files cannot be made playable merely by changing their extension.

Local audio is converted to FLAC for host decoding. It keeps one upcoming track preloaded. The host's finite-track limits still apply: 512 MiB per prepared audio file and 180 seconds for audio preparation. The metadata index is limited to 4 MiB. Very large first scans can exceed the 90-second metadata-request timeout; an intermediate index is saved, so a later scan can reuse completed entries. Symlinks are not traversed, and playback resolves files inside the chosen root. More details: [Local module](modules/local/README.md).

## Icecast radio

The module listens to direct HTTP(S) audio mountpoints on Icecast/Shoutcast servers; it does not broadcast to a server.

```sh
mz set icecast url "https://radio.example.org:8443/live.ogg"
mz set icecast name "My station"
mz settings icecast
mz icecast
```

Playback starts after buffering and continues before the stream reaches EOF. Radio uses bounded in-memory PCM buffering, without an endless temporary audio file or a second preloaded connection. Seeking is disabled. After a long pause, use `mz next` to reconnect to the live broadcast. Status and MPRIS show the station name; changing ICY song titles are **not yet displayed**.

### Icecast settings

| Key | Default | Meaning |
| --- | --- | --- |
| `station` | `default` | Station ID edited by the station-specific keys below |
| `stations` | one empty station | JSON array of `{id,name,url,username,password}`, 1–200 stations with unique IDs |
| `url` | empty | Full HTTP(S) URL of the selected station; credentials are separate fields |
| `server` | — | Set the selected station's server URL while retaining its mountpoint |
| `mountpoint` | — | Path such as `/live.mp3` on an already configured server |
| `name` | `Icecast` | Selected station's display name |
| `username`, `password` | empty | HTTP Basic authentication; settings output masks the password as `***` |
| `proxy` | empty | HTTP proxy URL; an empty string disables it |
| `user_agent` | `MusicZero/0.3` | HTTP User-Agent |
| `reconnect` | `true` | FFmpeg reconnect on EOF, network errors, HTTP 429 and 5xx |
| `reconnect_delay_max` | `5` | FFmpeg maximum reconnect delay, 1–60 seconds |
| `timeout` | `15` | Network read timeout, 1–120 seconds |
| `buffer_ms` | `1000` | Startup buffer/PCM queue sizing, 250–10000 milliseconds |
| `tls_verify` | `true` | Verify HTTPS certificates |
| `ca_file` | empty | CA file for a private HTTPS certificate authority |
| `ffmpeg` | `ffmpeg` | FFmpeg executable name or path |

Use HTTPS when authenticating. The displayed `***` is a mask, not the stored password. `reconnect=false` disables FFmpeg's internal reconnect options; the host still retries ended/failed playback. Changes to an active Icecast provider close the old stream and reconnect using the new settings.

### Several stations

Replace the example URLs with actual mountpoints:

```sh
mz set icecast stations '[{"id":"one","name":"First","url":"https://radio.example.org/one.mp3"},{"id":"two","name":"Second","url":"https://radio.example.org/two.ogg"}]'
mz icecast
mz playlists
mz playlist two
mz set icecast station two
mz set icecast buffer_ms 1500
```

`mz playlist` chooses playback; `station` chooses which entry `url`, `name`, `username`, etc. edit. The special playlist `default` follows the configured `station`. Use `mz playlist default` to return to that selection. In PowerShell, single quotes preserve the JSON string. More details: [Icecast module](modules/icecast/README.md).

## Settings, cache and environment

Configuration bases follow `XDG_CONFIG_HOME`/`~/.config` on Linux and `APPDATA` on Windows. Cache bases follow `XDG_CACHE_HOME`/`~/.cache` on Linux and `LOCALAPPDATA` on Windows.

| Data | Path relative to the appropriate base |
| --- | --- |
| Language and player preferences | Config: `mz/settings.json` |
| YMZ token | Config: `ymz/token` |
| YouMZ session/proxy | Config: `youmz/session`, `youmz/proxy`, `youmz/browser` |
| Local settings | Config: `mz-local/settings.json` |
| Icecast stations/settings | Config: `mz-icecast/settings.json` |
| Selected playlist for each provider | Config: `mz/<id>.playlist` |
| Local metadata index | Cache: `mz-local/index.json` |

Credentials and Local/Icecast settings are saved atomically; newly written files have mode `0600` on Unix. Custom providers manage their own configuration.

Finite audio lives in the system temporary directory, normally `/tmp` on Linux. Only the current track and one upcoming track, prepared or being downloaded, are retained. Files are removed on normal cleanup; an abort or force-kill can leave temporary files behind. There is no permanent downloaded-music cache. To change the temporary directory on Linux, create it and set `TMPDIR` before launching:

```sh
mkdir -p "$HOME/.cache/musiczero-tmp"
TMPDIR="$HOME/.cache/musiczero-tmp" mz local
```

`MZ_MODULES_DIR` overrides provider discovery; otherwise modules are read beside the actual executable, not from the current working directory. `RUST_LOG` controls logging; the host defaults to `warn,mz=info`. Linux control uses `$XDG_RUNTIME_DIR/musiczero.sock`, falling back to the host cache directory. Windows uses a named pipe. Player/client commands must use the same runtime environment to reach the same host.

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| `mz` not found | Installer's printed path, PATH, or `./mz.exe` in PowerShell |
| Provider absent from `mz modules` | Complete module folder beside the host; executable name, permissions and `MZ_MODULES_DIR`; source builds do not assemble that layout automatically |
| Old OAuth-client or token-file-only login messages | Update both host and provider binaries and restart; use the login flows above |
| Google refuses login | Normal browser sign-in, then the two-phase temporary-profile login; browser detection and `YOUMZ_BROWSER` |
| YouMZ cannot download | `yt-dlp` on PATH, its error output, saved session and proxy |
| Local folder has no tracks | Existing `path`, FFmpeg/ffprobe, supported decoders, recursion, hidden files and scan limits |
| MIDI does not play | FluidSynth, a readable configured SoundFont and valid MIDI data |
| Radio fails or disconnects | Direct mountpoint URL, auth, network/proxy, FFmpeg output, timeout and reconnect settings; CA file for private HTTPS |
| No sound | System audio device/ALSA setup; runtime audio output is still needed with `--no-default-features` |
| No tray/MPRIS | User D-Bus session and panel support; unavailable on Windows |
| Control says host is not running | Start a provider first and use the same runtime directory/environment |

## Build and verification

Use stable Rust. Linux compilation needs `pkg-config` and ALSA development headers (for example `libasound2-dev`); CI also installs `libsqlite3-dev`.

```sh
cargo build --workspace --release --locked
./install.sh
```

`install.sh` builds and installs the four official providers beside `mz`; `MUSICZERO_PREFIX` defaults to `~/.local`, and `CARGO_TARGET_DIR` is respected. `bootstrap.sh` clones a temporary checkout and invokes this installer; `MUSICZERO_REPO` and `MUSICZERO_REF` override its repository/ref.

The Windows CI build disables the default tray feature; the same flag is useful for a Linux build without a tray:

```sh
cargo build --workspace --release --no-default-features --locked
```

| Directory/package | Role |
| --- | --- |
| `mz` / `mz` | CLI, IPC, player, preloading, live PCM and Linux tray |
| `MCZ` / `mcz` | Shared MPRIS, platform paths and shutdown handling |
| `YMZ` / `ymz` | `ymz-module` provider |
| `YouMZ` / `youmz` | `youmz-module` provider and browser login |
| `Local` / `local` | `local-module` scanning/metadata/audio provider |
| `Icecast` / `icecast` | `icecast-module` settings/stations/live PCM provider |
| `ModuleSupport` / `mz-module-support` | Shared provider config, paths and FFmpeg execution |

```sh
cargo test --workspace --locked
cargo test -p mz --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

Linux integration smoke tests require FFmpeg/ffprobe with `libopenmpt` and OpenSSL. They generate synthetic audio, use isolated configurations, ALSA null output, a fake authenticated HTTP station and a temporary HTTPS CA; they do not use your accounts or music:

```sh
cargo build --workspace --locked
python3 -m unittest discover -s tests -p 'test_*.py'
python3 tests/smoke_modules.py
python3 tests/smoke_https.py
python3 tests/smoke_seek.py
python3 tests/smoke_wave.py
python3 tests/smoke_config.py
```

To test already built release binaries, set `MZ_TEST_PROFILE=release` on those Python commands. Tests cover synthetic FLAC and legacy formats, content detection, tags, authenticated live playback and provider switching; a successful build does not prove real account login or every decoder format.

Account integration checks are opt-in and require saved MusicZero credentials (or `YM_TOKEN`/`YOUMZ_SESSION`), network access and `yt-dlp`. They use temporary copies of app credentials, remove temporary files on exit, and never read browser profiles. `--host` additionally checks actual playback and paused/resumed seeking with ALSA null output; it does not test physical speakers. These checks are not run in CI:

```sh
python3 tests/check_services.py --host
# Check one service only
python3 tests/check_services.py --module youmz --host
# Check wave continuation beyond the first batch
python3 tests/check_services.py --module ymz --host --wave-tracks 15
```

CI builds Linux x64 and Windows x64. Pushes to `main`/`master` publish `build-<run_number>` prereleases after both builds pass; `v*` tags publish versioned releases. See [.github/workflows/ci.yml](.github/workflows/ci.yml) for exact packaging and release conditions. License: [LICENSE](LICENSE).

## Write a provider

Providers can be written in any language. Build executables separately for each platform. Create `modules/<id>/module.json` and place the binary beside it:

```json
{
  "protocol": 1,
  "id": "demo",
  "name": "My Service",
  "binary": "demo-module",
  "default_playlist": "main"
}
```

The folder name must equal `id`; `id` and `binary` use ASCII letters, digits, `_` and `-`. Write `binary` without `.exe`; Windows appends the extension. Discovery skips malformed manifests and missing executables. `info` must match the manifest's protocol, ID, name and default playlist exactly.

A fresh process handles each request. Write **only protocol data to stdout**, diagnostics to stderr, and exit nonzero on failure. Finite audio is returned in an encoded format the host can decode, such as MP3, M4A, WAV or FLAC.

| Command | stdout |
| --- | --- |
| `info` | `{"protocol":1,"id":"demo","name":"My Service","default_playlist":"main"}` |
| `playlists` | `{"playlists":[{"id":"main","name":"Main"}]}` |
| `tracks main` | `{"tracks":[{"id":"track-1","title":"Song","artist":"Artist","duration_ms":120000,"art_url":""}]}` |
| `audio track-1` | Encoded audio bytes only |
| `settings` | `{"settings":{}}` or populated settings inside that wrapper |
| `set-setting key value` | Updated `{"settings":{...}}`; optional if settings are not editable |
| `login` | Optional interactive command with inherited terminal I/O |

`info`, `playlists`, `tracks`, `audio` and `settings` are required. Return tracks in playback order; IDs pass back unchanged. JSON responses are limited to 4 MiB/90 seconds. Finite audio is limited to 512 MiB/180 seconds, including process completion.

For continuous recommendations, the `tracks` response adds `"continuous":true`. Subsequent requests use `tracks <playlist> <after>`, where `after` is the last finished or skipped track ID. The host removes batch overlaps and remembers the last 256 IDs; ordinary playlists keep their existing behavior. An optional track field `"feedback":"<batch-context>"` enables `feedback <event> <id> <context> <played_seconds>`, returning `{"ok":true}`. Events `trackStarted`, `trackFinished` and `skip` are sent in order; seconds measure actual listening excluding pauses. The host drains pending events before requesting continuation. YMZ uses this extension for My Wave.

For live audio, a track adds `"stream":true,"buffer_ms":1000`. Its `audio` command must output **raw signed 16-bit little-endian PCM, 48000 Hz, two interleaved channels**. It can remain running indefinitely. The host bounds buffering, starts before EOF, and cancels its reader/provider on switch or exit; startup is limited to 60 seconds and `buffer_ms` is clamped to 250–10000. Seeking and second-stream preloading are disabled. This extension requires the current host.

The standalone [examples/module-template](examples/module-template) crate generates a WAV tone without dependencies:

```sh
cargo build --release --manifest-path examples/module-template/Cargo.toml
```

Copy its `module.json` and resulting `demo-module`/`demo-module.exe` into `modules/demo`. For development, stage complete provider folders and set `MZ_MODULES_DIR`; pointing it at manifests without their executables will not work.
