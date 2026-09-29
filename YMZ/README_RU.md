# YMZ

Модуль Яндекс Музыки для MusicZero. Сборка: `cargo build -p ymz --release --locked`. Поместите `ymz-module` (или `ymz-module.exe`) рядом с `module.json` в `modules/ymz` около `mz`.

Токен: `~/.config/ymz/token` на Linux, `%APPDATA%\ymz\token` на Windows или переменная `YM_TOKEN`. Запуск: `mz start ymz`. Плейлисты и настройки «Моей волны» доступны через `mz playlists`, `mz settings` и `mz set`.

Описание протокола модулей — в [README_RU.md](../README_RU.md#свой-модуль).
