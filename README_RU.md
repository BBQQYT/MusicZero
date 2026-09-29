# MusicZero

`mz` — один музыкальный плеер. Аудиовывод, очередь, команды и MPRIS на Linux находятся внутри него. Сервисы лежат отдельными папками в `modules/` рядом с исполняемым файлом:

```text
musiczero/
├── mz                 # Windows: mz.exe
└── modules/
    ├── ymz/
    │   ├── module.json
    │   └── ymz-module     # Windows: ymz-module.exe
    └── youmz/
        ├── module.json
        └── youmz-module   # Windows: youmz-module.exe
```

Работает только один аудиоплеер. При смене сервиса `mz` очищает очередь и запускает нужный модуль по запросу. Модуль возвращает сведения о треках в JSON и записывает закодированное аудио в stdout. `mz` сохраняет аудио во временный файл и декодирует его, поэтому размер трека не определяет расход памяти основного процесса. Модули не остаются в памяти между запросами.

## Установка и запуск

**Linux x86_64 — установка одной командой** (нужны `curl` и Python 3.8+):

```sh
curl -fsSL https://raw.githubusercontent.com/BBQQYT/MusicZero/main/install-linux.py | python3
```

Скрипт берёт последний опубликованный релиз, проверяет SHA-256 и устанавливает `mz` с YMZ и YouMZ в `~/.local/bin`. Повторный запуск обновляет их; папки собственных модулей сохраняются. Если `~/.local/bin` не входит в `PATH`, скрипт напечатает полный путь для запуска.

Для ручной установки скачайте архив из [Releases](https://github.com/BBQQYT/MusicZero/releases) и распакуйте папку `musiczero` целиком. Для Linux нужна аудиосистема ALSA/PipeWire и пользовательская D-Bus сессия для MPRIS и трея. Для YouMZ установите `yt-dlp` и добавьте его в `PATH`.

```sh
mz modules              # список доступных модулей
mz start ymz            # запустить Яндекс Музыку
mz start youmz          # запустить YouTube Music
mz switch youmz         # сменить источник в уже запущенном mz
mz status
mz toggle
mz next
mz playlists
mz playlist 2           # номер или ID
mz settings             # настройки выбранного сервиса
mz set mood calm        # пример для YMZ
mz quit
```

Можно писать `mz ymz` вместо `mz start ymz`. Оставьте первое окно терминала с плеером открытым; команды управления запускайте во втором. На Windows из папки распаковки используйте `./mz.exe` в PowerShell. В Linux `mz` также показывает один общий трей, если панель поддерживает StatusNotifierItem. В Windows управление доступно через CLI; родного трея и системных медиаклавиш пока нет.

Для YMZ сохраните OAuth токен в `%APPDATA%\ymz\token` на Windows или `~/.config/ymz/token` на Linux. Также подходит переменная `YM_TOKEN`. Для YouMZ выполните `mz login youmz`: модуль покажет ссылку входа в консоли и сохранит сессию. Дополнительно можно задать `YOUMZ_COOKIE`.

## Свой модуль

Модуль — обычная консольная программа. Его можно написать на любом языке и собрать отдельно под Linux и Windows. Формат вызовов одинаков; сами бинарные файлы должны быть собраны для соответствующей ОС. Ни Rust, ни исходники MusicZero модулю не требуются.

Создайте папку `modules/<id>` и положите туда `module.json` и исполняемый файл. Пример:

```json
{
  "protocol": 1,
  "id": "demo",
  "name": "Мой сервис",
  "binary": "demo-module",
  "default_playlist": "main"
}
```

`id` должен совпадать с именем папки. В `binary` пишется имя файла без `.exe`; в Windows `mz` добавит расширение сам. Имена состоят из латинских букв, цифр, `_` и `-`. `mz` ищет папку `modules` рядом с собой. Для разработки можно задать `MZ_MODULES_DIR`.

Каждый вызов запускает программу заново с командой в аргументах. JSON пишите **только в stdout**, сообщения об ошибках — в stderr и завершайте процесс с ненулевым кодом:

| Команда | Ответ в stdout |
| --- | --- |
| `info` | `{"protocol":1,"id":"demo","name":"Мой сервис","default_playlist":"main"}` |
| `playlists` | `{"playlists":[{"id":"main","name":"Основной"}]}` |
| `tracks main` | `{"tracks":[{"id":"track-1","title":"Трек","artist":"Автор","duration_ms":120000,"art_url":""}]}` |
| `audio track-1` | Только байты MP3, M4A или WAV. JSON и текст здесь запрещены. |
| `settings` | `{"settings":{}}` или объект настроек |
| `set-setting key value` | Новые настройки в формате `settings`; необязательно, если настроек нет |
| `login` | Необязательный интерактивный вход; stdin/stdout подключены к терминалу |

`info`, `playlists`, `tracks`, `audio` и `settings` обязательны. ID трека возвращается модулю без изменений. Выдавайте плейлисты и треки в нужном порядке. Модуль сам отвечает за сеть, авторизацию и получение аудиобайтов. Программа `mz` берёт на себя декодирование, очередь, паузу, следующий трек и смену источника.

Рабочий образец находится в [examples/module-template](examples/module-template). Он не использует зависимости, создаёт WAV тон и собирается обычным `cargo build --release --manifest-path examples/module-template/Cargo.toml`. Скопируйте `module.json` и полученный `demo-module` (`demo-module.exe` на Windows) в `modules/demo`.

## Сборка проекта

```sh
cargo build --workspace --release --locked
```

Для сборки на Linux нужны `pkg-config` и `libasound2-dev` (или аналоги вашего дистрибутива). `./install.sh` собирает проект из исходников и устанавливает его в `~/.local/bin`. Автоматические сборки `main` публикуются как предварительные релизы; теги `v*` создают обычные релизы с архивами Linux и Windows.
