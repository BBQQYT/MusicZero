# YouMZ

Модуль YouTube Music для MusicZero. Сборка: `cargo build -p youmz --release --locked`. Поместите `youmz-module` (или `youmz-module.exe`) рядом с `module.json` в `modules/youmz` около `mz`.

Для аудио нужен `yt-dlp` в `PATH`. Выполните `mz login youmz`, затем `mz start youmz`. Также поддерживается `YOUMZ_COOKIE`. Описание протокола модулей — в [README_RU.md](../README_RU.md#свой-модуль).
