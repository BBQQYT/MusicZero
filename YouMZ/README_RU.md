# YouMZ

Модуль YouTube Music для MusicZero. Сборка: `cargo build -p youmz --release --locked`. Поместите `youmz-module` (или `youmz-module.exe`) рядом с `module.json` в `modules/youmz` около `mz`.

Для аудио нужен `yt-dlp` в `PATH`. Выполните `mz login YouMZ`, войдите в YouTube Music в отдельном окне браузера и дождитесь сообщения об успешном входе. Модуль получает активную сессию через DevTools или WebDriver BiDi без чтения cookie-файлов браузера. Затем запустите `mz start youmz`. Путь к браузеру можно задать через `YOUMZ_BROWSER`. Описание протокола модулей — в [README_RU.md](../README_RU.md#свой-модуль).
