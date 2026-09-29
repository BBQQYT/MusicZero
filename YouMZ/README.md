# YouMZ module

Build with `cargo build -p youmz --release --locked` and put `youmz-module` (or `youmz-module.exe`) beside `module.json` in `modules/youmz` next to `mz`. Install `yt-dlp` on `PATH`, run `mz login YouMZ`, sign in using the separate browser window, and then run `mz start youmz`. The module reads the live browser session via DevTools or WebDriver BiDi without opening browser cookie files. Set `YOUMZ_BROWSER` if needed. See the [module protocol](../README.md#write-a-module).
