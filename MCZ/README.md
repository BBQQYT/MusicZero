# MCZ — Music Core Zero

Shared Rust crate for YMZ and YouMZ. It owns MPRIS, playback commands, queue, shutdown handling, XDG/Windows profile paths, and the Linux tray menu. Each service keeps its own authentication, API, and audio decoding adapter.

Place `MCZ`, `YMZ`, and `YouMZ` in the same parent directory. Both applications refer to `../MCZ` from their Cargo manifests.

Linux runtime needs an audio device and a user D-Bus session. The tray additionally needs a StatusNotifierItem host (a compatible desktop panel). `systemd` is optional. On Windows the CLI uses local named pipes for playback and playlist control. Native Windows media controls and tray are not implemented yet.
