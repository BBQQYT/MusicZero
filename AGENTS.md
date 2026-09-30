# AGENTS.md

## Structure
- Cargo workspace (`resolver = "2"`): `MCZ`, `YMZ`, `YouMZ`, `mz`, `Local`, `Icecast`, `ModuleSupport`. Root `Cargo.toml` only defines workspace + `profile.release` (`opt-level="s"`, `lto=true`, `strip=true`).
- `MCZ` (`mcz` crate) — shared lib: `src/paths.rs` (XDG/APPDATA resolution), `src/mpris.rs`, `src/shutdown.rs`. No binary.
- `mz` — host player binary `src/main.rs` + `player.rs` (queue/playback/MPRIS), `ipc.rs`, `plugin.rs` (module discovery/execution), `tray.rs` (Linux-only, `ksni` feature). `rodio` with `symphonia-all`, `zbus`, `tokio`.
- `YMZ` (`ymz` crate, `autobins=false`) — binary `ymz-module` at `YMZ/src/bin/ymz-module.rs`. `YouMZ` (`youmz` crate, `autobins=false`) — binary `youmz-module` at `YouMZ/src/bin/youmz-module.rs`.
- Repo module manifests: `modules/ymz/module.json`, `modules/youmz/module.json`. Installed layout is `musiczero/mz` (+ `.exe` on Windows) beside `musiczero/modules/<id>/`.
- `examples/module-template` — standalone crate (own `Cargo.lock`/`target/`, `[workspace]` isolated). Copy its `module.json` + built binary to `modules/demo/` to test protocol.

- `Local`/`Icecast` expose `local-module`/`icecast-module`; `ModuleSupport` shares atomic settings, paths and FFmpeg execution without depending on the audio host. Module setup and runtime dependencies are documented in `modules/local/README.md` and `modules/icecast/README.md`.
- Live tracks set `stream=true` and write s16le 48 kHz stereo PCM; `mz/src/live.rs` owns a bounded reader and cancels its provider. Do not apply the finite-file EOF/download limit or next-stream preloading to live radio.
- `mz settings <module>` / `mz set <module> <key> <value>` configure providers before startup; active Local/Icecast changes reset the playback generation to discard obsolete downloads.

## Build & Run
- Canonical build: `cargo build --workspace --release --locked`
- Windows / headless (no tray): `cargo build --workspace --release --no-default-features --locked` — CI does this on `windows-2022`.
- Linux build deps: `pkg-config`, `libasound2-dev` (CI also installs `libsqlite3-dev`). Without them `rodio`/ALSA fails to link.
- Single crate: `cargo build -p mcz --release` / `cargo build -p ymz --release` / `cargo build -p youmz --release`.
- Example module: `cargo build --release --manifest-path examples/module-template/Cargo.toml`
- Install from source: `./install.sh` (respects `MUSICZERO_PREFIX` default `~/.local`; installs `mz` + `modules/*/module.json`+binaries). `bootstrap.sh` clones `MUSICZERO_REPO`/`MUSICZERO_REF` to temp and runs `install.sh`.
- Integration smoke tests (Linux, FFmpeg/ffprobe with libopenmpt, and OpenSSL): after `cargo build --workspace --locked`, run `python3 tests/smoke_modules.py` and `python3 tests/smoke_https.py`. These use synthetic audio and isolated configs, a fake authenticated station, ALSA null output and a temporary CA.
- Tests: `cargo test --workspace --locked`; focused shared-library tests: `cargo test -p mcz --locked`; host tests: `cargo test -p mz --locked`. No clippy/rustfmt config in repo.
- Preloading: `Preload` in `mz/src/player.rs` holds one upcoming audio file; dropping its pending future cancels the provider. Successful provider/playlist changes discard it; `next` during playback keeps it.
- Audio decoding: use `decode_audio` in `mz/src/player.rs`; seekable M4A requires the file byte length in `Decoder::builder()`. The regression fixture is synthetic (`mz/tests/fixtures/tone.m4a`).
- Env logging: `mz` uses `env_logger` default `warn,mz=info` (`RUST_LOG` overrides).

## Module Protocol (protocol = 1)
- `modules/<id>/module.json` fields: `protocol`, `id`, `name`, `binary` (no `.exe`), `default_playlist`. `id` must equal folder name; `id`/`binary` charset `^[A-Za-z0-9_-]+$`. Windows host appends `.exe`.
- Discovery: `mz/src/plugin.rs:module_dir()` — `MZ_MODULES_DIR` if set, else `current_exe_parent()/modules`. Sorted by `id`, skipped if manifest invalid, protocol !=1, or binary missing.
- Host validates per module: `module info` JSON must exactly match `module.json` (`protocol`, `id`, `name`, `default_playlist`).
- Commands (fresh process per call, stdout = protocol data, stderr + non-zero exit = error):
  `info` → manifest identity fields; `playlists` → `{"playlists":[...]}`, `tracks <playlist>` → `{"tracks":[...]}`, `audio <id>` → raw MP3/M4A/WAV bytes, `settings` → `{"settings":{}}`, `set-setting <key> <value>` → updated settings, `login` (inherits stdio, optional).
- Limits: JSON stdout capped at 4 MiB (`take(4MiB+1)`), audio stdout capped at 512 MiB; timeouts 90s for JSON, 180s for audio.

## Host Runtime Gotchas
- IPC control channel is **not** HTTP. Second `mz` invocation is a client that sends one JSON line `{"action","value","key"}` and reads one line back. Linux: Unix socket at `$XDG_RUNTIME_DIR/musiczero.sock` else `~/.cache/mz/musiczero.sock` (`MCZ/src/paths.rs:cache_dir`), mode `0600`, "already running" check via connect probe. Windows: named pipe `\\.\pipe\musiczero-mz` with retry on `231`.
- `mz` without args shows `status` if host running else `help`. `mz <id>` is shortcut for `mz start <id>` (case-insensitive). `mz switch <id>` re-discovers + validates before switching; it clears queue and keeps same audio/MPRIS endpoint. Only one player at a time.
- Selected playlist persisted to `config_dir("mz")/<id>.playlist` (`MCZ/src/paths.rs:config_dir` — `APPDATA`/`XDG_CONFIG_HOME`/`HOME/.config` fallback). Queue loaded via `plugin::selected_playlist`.
- Credentials: `YM_TOKEN` env or `~/.config/ymz/token` (`%APPDATA%\ymz\token` on Windows); YouMZ browser login via `YOUMZ_BROWSER` override, requires `yt-dlp` on `PATH`. `MZ_MODULES_DIR` overrides module search for dev.
- Linux-only: `tray` feature (`ksni`, default) and MPRIS `org.mpris.MediaPlayer2.mz` on session bus — disabled with a warning if D-Bus unavailable. Windows has no tray/media keys (CLI only).

## CI / Release
- `.github/workflows/ci.yml`: matrix `ubuntu-24.04` / `windows-2022`, `dtolnay/rust-toolchain@stable` + `Swatinem/rust-cache`. Push to `main`/`master` or `v*` tags builds + packages `dist/musiczero` into `musiczero-linux-x64.tar.gz` / `musiczero-windows-x64.zip`; auto-prerelease `build-<run_number>` on `main`, versioned release on `v*`. Trust this file over README when they differ.
