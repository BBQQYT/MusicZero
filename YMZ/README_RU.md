# YMZ

Модуль Яндекс Музыки для MusicZero. Сборка: `cargo build -p ymz --release --locked`. Поместите `ymz-module` (или `ymz-module.exe`) рядом с `module.json` в `modules/ymz` около `mz`.

Выполните `mz login YMZ`: откроется сайт получения токена, после вставки в терминал токен проверяется и сохраняется. Переменная `YM_TOKEN` также поддерживается. Запуск: `mz start ymz`. Плейлисты и настройки «Моей волны» доступны через `mz playlists`, `mz settings` и `mz set`.

Описание протокола модулей — в [README_RU.md](../README_RU.md#свой-модуль).
