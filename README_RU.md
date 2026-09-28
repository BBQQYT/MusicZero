# MusicZero

<p align="center">
  <a href="README.md"><img src="https://img.shields.io/badge/Language-English-lightgrey?style=for-the-badge" alt="English" /></a>
  <a href="README_RU.md"><img src="https://img.shields.io/badge/Язык-Русский-red?style=for-the-badge" alt="Русская версия" /></a>
</p>

MusicZero — лёгкий музыкальный плеер из двух сервисных клиентов и общего ядра на Rust:

- **YMZ** воспроизводит Яндекс Музыку, включая «Мою волну» и плейлисты аккаунта.
- **YouMZ** воспроизводит миксы YouTube Music, понравившиеся треки и сохранённые плейлисты.
- **MCZ (Music Core Zero)** отвечает за общее воспроизведение, MPRIS, очередь, пути настроек и дополнительное меню в системном трее.

Для воспроизведения не нужно открывать браузер. В меню трея можно переключать плейлисты; у YMZ там же доступны настроение, разнообразие и язык «Моей волны».

## Установка из исходников

Установите Rust через [rustup](https://rustup.rs), затем установите системные зависимости для своей Linux-системы.

| Дистрибутив | Зависимости для сборки |
| --- | --- |
| Debian, Ubuntu, Linux Mint | `sudo apt install build-essential pkg-config libasound2-dev libsqlite3-dev` |
| Fedora | `sudo dnf install gcc pkgconf-pkg-config alsa-lib-devel sqlite-devel` |
| Arch Linux, Manjaro | `sudo pacman -S base-devel pkgconf alsa-lib sqlite` |
| openSUSE | `sudo zypper install gcc pkg-config alsa-devel sqlite3-devel` |
| Alpine Linux | `sudo apk add build-base pkgconf alsa-lib-dev sqlite-dev` |

Собрать демоны и оба приложения трея:

```sh
cargo build --workspace --release --features ymz/tray,youmz/tray
```

Или установить все четыре исполняемых файла в `~/.local/bin`:

```sh
./install.sh
```

Установить оба клиента напрямую через Cargo:

```sh
cargo install --git https://github.com/BBQQYT/MusicZero.git ymz youmz --features tray --locked
```

Или по отдельности:

```sh
cargo install --git https://github.com/BBQQYT/MusicZero.git ymz --features tray --locked
cargo install --git https://github.com/BBQQYT/MusicZero.git youmz --features tray --locked
```

Для другого каталога установки можно задать префикс, например `MUSICZERO_PREFIX=/usr/local ./install.sh`.

## Системные требования для работы

- Аудиовыход, поддерживаемый ALSA/CPAL. В установках PipeWire и PulseAudio обычно есть слой совместимости с ALSA.
- Пользовательская D-Bus-сессия для управления MPRIS.
- Панель или трей с поддержкой StatusNotifierItem для отображения значков.
- `yt-dlp` в `PATH` для загрузки аудио YouMZ.
- OAuth-токен Яндекс Музыки для YMZ и авторизованная сессия YouTube Music для YouMZ.

Конкретная система инициализации не требуется. Приложения трея связываются с плеером через пользовательскую D-Bus-сессию.

## Настройка YMZ

Сохраните OAuth-токен Яндекс Музыки в каталоге настроек XDG и ограничьте доступ к файлу:

```sh
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/ymz"
printf '%s\n' 'ВАШ_ТОКЕН_ЯНДЕКСА' > "${XDG_CONFIG_HOME:-$HOME/.config}/ymz/token"
chmod 600 "${XDG_CONFIG_HOME:-$HOME/.config}/ymz/token"
```

Вместо файла можно задать переменную `YM_TOKEN`. По умолчанию YMZ запускает «Мою волну». В трее можно выбрать плейлист аккаунта и изменить настройки волны.

## Настройка YouMZ

При первом запуске можно использовать вход через команду:

```sh
youmz login
```

Либо сохраните cookie авторизованной сессии YouTube Music в `${XDG_CONFIG_HOME:-$HOME/.config}/youmz/cookie` и установите права `600`. При необходимости укажите прокси в файле `proxy` или через `YOUMZ_PROXY`, например `socks5h://127.0.0.1:2080`.

По умолчанию YouMZ запускает **«Мой джем»** (`RDMM`). В трее можно выбрать понравившуюся музыку или сохранённый плейлист. Выбор сохраняется для следующего запуска.

## Запуск

Запустите плеер и, если нужно, его трей в той же пользовательской сессии:

```sh
ymz
ymz-tray
```

Или:

```sh
youmz
youmz-tray
```

Трей необязателен. Без него управлять воспроизведением можно через любой MPRIS-клиент, например `playerctl`:

```sh
playerctl -p ymz play-pause
playerctl -p youmz next
```

## Каталоги настроек

В Linux MusicZero использует `XDG_CONFIG_HOME` и `XDG_CACHE_HOME`. Если переменные не заданы, используются `~/.config` и `~/.cache`.

## Windows

Поддержка Windows пока неполная. В MCZ есть переносимые пути профиля и обработка завершения процесса, но клиенты пока зависят от Linux D-Bus. Нативные трей и медиауправление Windows ещё не реализованы.

## Лицензия

MusicZero распространяется по лицензии MIT. Тексты лицензии находятся в каталогах проектов.
