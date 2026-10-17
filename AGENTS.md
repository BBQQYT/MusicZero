# AGENTS.md

## Structure
- Cargo workspace (`resolver = "2"`): `MCZ`, `YMZ`, `YouMZ`, `Spotify`, `mz`, `Local`, `Icecast`, `ModuleSupport`. Root `Cargo.toml` only defines workspace + `profile.release` (`opt-level="s"`, `lto=true`, `strip=true`).
- `MCZ` (`mcz` crate) — shared lib: `src/paths.rs` (XDG/APPDATA resolution), `src/mpris.rs`, `src/shutdown.rs`. No binary.
- `mz` — host player binary `src/main.rs` + `player.rs` (queue/playback/MPRIS), `ipc.rs`, `plugin.rs` (module discovery/execution), `tray.rs` (Linux-only, `ksni` feature), `tray_win.rs` + `smtc.rs` (Windows-only: Win32 tray menu and System Media Transport Controls on the `windows` crate). `rodio` with `symphonia-all`, `zbus`, `tokio`; `ksni` is `[target.'cfg(unix)'.dependencies]` so Windows builds keep default features.
- `android/` — native Android front-end (Kotlin + Compose Material 3, media3 `MediaLibraryService`, Android Auto manifest). Gradle project (AGP 9.4.1 built-in Kotlin, Gradle 9.8.1, compileSdk 37, **targetSdk 28** — Android 10 forbids exec() from app data for higher targets, Termux-style). `scripts/build-android.sh` cross-builds the workspace for `aarch64-linux-android`, stages binaries as `android/app/src/main/jniLibs/arm64-v8a/lib*.so` (jniLib name ↔ CLI name map lives in `Engine.kt`/build script), copies `modules/<id>/module.json` verbatim into assets and runs Gradle → `dist/musiczero-android-arm64.apk`. App unpacks jniLibs → `filesDir/bin`, assets → `filesDir/modules` (`MZ_MODULES_DIR`), runs `mz start <module>` with HOME/XDG/TMP env inside app storage, talks IPC over `cacheDir/musiczero.sock` (`LocalSocket`), service owns the process (kill on notification swipe) and mirrors status as a media session (playlist browse for Auto; seek/±15s via IPC µs with `key:"absolute|relative"`).
- `YMZ` (`ymz` crate, `autobins=false`) — binary `ymz-module` at `YMZ/src/bin/ymz-module.rs`. `YouMZ` (`youmz` crate, `autobins=false`) — binary `youmz-module` at `YouMZ/src/bin/youmz-module.rs`. `Spotify` (`spotify` crate, `autobins=false`) — binary `spotify-module` at `Spotify/src/bin/spotify-module.rs` (Web API + Librespot 0.8 audio; `vergen` pinned `~9.0.6` — 9.1 breaks `vergen-gitcl` build scripts).
- Repo module manifests: `modules/ymz/module.json`, `modules/youmz/module.json`, `modules/spotify/module.json`. Installed layout is `musiczero/mz` (+ `.exe` on Windows) beside `musiczero/modules/<id>/`.
- `examples/module-template` — standalone crate (own `Cargo.lock`/`target/`, `[workspace]` isolated). Copy its `module.json` + built binary to `modules/demo/` to test protocol.

- `Local`/`Icecast` expose `local-module`/`icecast-module`; `ModuleSupport` shares atomic settings, paths and FFmpeg execution without depending on the audio host. Module setup and runtime dependencies are documented in `modules/local/README.md`, `modules/icecast/README.md` and `modules/spotify/README.md`.
- Live tracks set `stream=true` and write s16le 48 kHz stereo PCM; `mz/src/live.rs` owns a bounded reader and cancels its provider. Do not apply the finite-file EOF/download limit or next-stream preloading to live radio.
- `mz settings <module>` / `mz set <module> <key> <value>` configure providers before startup; active Local/Icecast changes reset the playback generation to discard obsolete downloads.

## Build & Run
- Canonical build: `cargo build --workspace --release --locked`
- Windows: default features work (`tray_win.rs`/`smtc.rs` are Windows-only code). Headless alias: `cargo build --workspace --release --no-default-features --locked`. Packaging: `musiczero-windows-*.zip` + `install-windows.ps1` (`-Check` verifies a `musiczero\` layout; `-Uninstall` removes install dir, user PATH entry and Start Menu shortcut).
- Android APK: needs NDK r27+ (`ANDROID_NDK_HOME`), `rustup target add aarch64-linux-android`, JDK 17+: `./scripts/build-android.sh` (workspace `--release --no-default-features --locked --target aarch64-linux-android`, jniLibs+assets staging, `./gradlew :app:assembleRelease`; `assembleDebug` works locally without an NDK for Kotlin-only changes; note `lintVitalRelease` disables `ExpiredTargetSdkVersion` because targetSdk 28 is deliberate).
- Linux build deps: `pkg-config`, `libasound2-dev` (CI also installs `libsqlite3-dev`). Without them `rodio`/ALSA fails to link.
- Single crate: `cargo build -p mcz --release` / `cargo build -p ymz --release` / `cargo build -p youmz --release` / `cargo build -p spotify --release`.
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
  `info` → manifest identity fields; `playlists` → `{"playlists":[...]}`, `tracks <playlist>` → `{"tracks":[...]}`, `audio <id>` → raw MP3/M4A/WAV/Ogg-Vorbis/MP4 bytes, `settings` → `{"settings":{}}`, `set-setting <key> <value>` → updated settings, `login` (inherits stdio, optional).
- Limits: JSON stdout capped at 4 MiB (`take(4MiB+1)`), audio stdout capped at 512 MiB; timeouts 90s for JSON, 180s for audio.

## Host Runtime Gotchas
- IPC control channel is **not** HTTP. Second `mz` invocation is a client that sends one JSON line `{"action","value","key"}` and reads one line back. Linux: Unix socket at `$XDG_RUNTIME_DIR/musiczero.sock` else `~/.cache/mz/musiczero.sock` (`MCZ/src/paths.rs:cache_dir`), mode `0600`, "already running" check via connect probe. Windows: named pipe `\\.\pipe\musiczero-mz` with retry on `231`.
- `mz` without args shows `status` if host running else `help`. `mz <id>` is shortcut for `mz start <id>` (case-insensitive). `mz switch <id>` re-discovers + validates before switching; it clears queue and keeps same audio/MPRIS endpoint. Only one player at a time.
- Selected playlist persisted to `config_dir("mz")/<id>.playlist` (`MCZ/src/paths.rs:config_dir` — `APPDATA`/`XDG_CONFIG_HOME`/`HOME/.config` fallback). Queue loaded via `plugin::selected_playlist`.
- Credentials: `YM_TOKEN` env or `~/.config/ymz/token` (`%APPDATA%\ymz\token` on Windows); YouMZ browser login via `YOUMZ_BROWSER` override, requires `yt-dlp` on `PATH`; Spotify PKCE tokens in `~/.config/mz-spotify/tokens.json` + `settings.json` (Windows `%APPDATA%\mz-spotify\`). `MZ_MODULES_DIR` overrides module search for dev.
- Linux-only: `tray` feature (`ksni`, default) and MPRIS `org.mpris.MediaPlayer2.mz` on session bus — disabled with a warning if D-Bus unavailable.
- Windows-only: `tray_win::spawn` native tray (menu mirrors `tray.rs`; obeys `tray_enabled`; programmatic 32×32 RGBA DIB icon, no assets) and `smtc.rs` System Media Transport Controls (media keys/lock screen; `media_keys_enabled` default true gates it at spawn in `main.rs` — changes need a player restart). Tray and SMTC button actions go through the same IPC loopback (`ipc::call`); ±15 s seeks are relative IPC seeks in µs with `key:"relative"`.

## CI / Release
- `.github/workflows/ci.yml`: matrix `ubuntu-24.04` / `windows-2022` (+ `windows-11-arm` alpha), `dtolnay/rust-toolchain@stable` + `Swatinem/rust-cache`. Push to `main`/`master` or `v*` tags builds + packages `dist/musiczero` into `musiczero-linux-x64.tar.gz` / `musiczero-windows-x64.zip`; `termux-alpha` job cross-builds `musiczero-termux-arm64-alpha.tar.gz`; `android` job (linux, NDK 27 via `android-actions/setup-android`, JDK 17) builds the APK via `scripts/build-android.sh` and uploads `musiczero-android-arm64.apk` (release publish includes `musiczero-*.apk`). Auto-prerelease `build-<run_number>` on `main`, versioned release on `v*`. Trust this file over README when they differ.
