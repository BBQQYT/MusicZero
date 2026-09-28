# MusicZero: MCZ + YMZ + YouMZ

This directory is a Cargo workspace. `MCZ` is the shared core; `YMZ` and `YouMZ` are service adapters.

## Linux: one build command

```sh
cargo build --workspace --release --features ymz/tray,youmz/tray
```

To build and install all four binaries to `~/.local/bin`:

```sh
./install.sh
```

`MUSICZERO_PREFIX=/some/path ./install.sh` changes the installation prefix. The build needs Rust/Cargo, `pkg-config`, ALSA development headers, and a C compiler. YouMZ needs `yt-dlp` at runtime. Both tray processes need a session D-Bus and a StatusNotifierItem host. No systemd service is required.

## Publishing plan

Publish this **whole workspace** as a new GitHub repository such as `BBQQYT/MusicZero`. The three directories must be committed together; existing YMZ and YouMZ repositories alone cannot resolve their `../MCZ` dependency. Once the new repository is public, users can build and install with one Cargo command:

Create a clean upload directory without the nested Git histories or build artifacts:

```sh
./export-monorepo.sh ../MusicZero-publish
```

```sh
cargo install --git https://github.com/BBQQYT/MusicZero.git ymz youmz --features ymz/tray,youmz/tray --locked
```

There is also `bootstrap.sh` for a future one-line installer hosted in that repository:

```sh
curl -fsSL https://raw.githubusercontent.com/BBQQYT/MusicZero/main/bootstrap.sh | sh
```

**That URL is not live until publication.** GitHub Releases can later offer prebuilt archives; source installation remains available for distributions or CPU architectures without a matching binary.

## Windows

MCZ passes `cargo check --target x86_64-pc-windows-msvc`. Native Windows tray and media controls are still pending. Full Windows app cross-build needs a Windows C toolchain/SDK on this Linux host.
