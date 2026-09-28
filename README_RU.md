# MusicZero

<p align="center">
  <a href="README.md"><img src="https://img.shields.io/badge/Language-English-lightgrey?style=for-the-badge" alt="English" /></a>
  <a href="README_RU.md"><img src="https://img.shields.io/badge/Язык-Русский-red?style=for-the-badge" alt="Русская версия" /></a>
</p>

MusicZero — лёгкий модульный музыкальный плеер на Rust с единым интерфейсом управления **`mz`**:

- **`mz`** — единая консольная утилита запуска и управления. Запускает любой плеер; в Linux также запускает системный трей.
- **`YMZ`** — модуль воспроизведения Яндекс Музыки («Моя волна» и плейлисты аккаунта).
- **`YouMZ`** — модуль воспроизведения YouTube Music («Мой джем», миксы, плейлисты библиотеки).
- **`MCZ (Music Core Zero)`** — общее ядро: аудиовывод (rodio/symphonia), MPRIS v2, очередь, конфигурация и системный трей (ksni по D-Bus).

Для воспроизведения не нужен браузер. В Linux иконка в трее запускается вместе с плеером. В Windows команды `mz` передаются через локальный именованный канал; трея и интеграции с мультимедийными клавишами пока нет.

---

## Быстрый старт

### Запуск (плеер + трей запускаются одной командой):

```sh
mz ymz        # Запуск Яндекс Музыки (с треем)
mz youmz      # Запуск YouTube Music (с треем)
mz all        # Запуск обоих плееров одновременно
```

Флаг `--no-tray` позволяет запустить плеер без иконки в трее (например, на сервере).

### Управление воспроизведением одной командой `mz`:

```sh
mz toggle     # Play / Pause (также: mz play, mz pause, mz pp)
mz next       # Следующий трек (также: mz skip)
mz stop       # Остановить воспроизведение
mz status     # Показать текущий трек, сервис и статус воспроизведения
```

*Команда автоматически определяет активный плеер. Если запущены оба — можно явно указать сервис, например `mz next ymz` или `mz pause youmz`. Возврат к предыдущему треку пока не поддерживается.*

### Плейлисты и настройки волны:

```sh
mz playlists        # Показать список доступных плейлистов
mz playlist 2       # Переключиться на плейлист №2
mz wave             # Показать настройки «Моей волны» (настроение/разнообразие/язык)
mz wave mood calm   # Установить настроение волны: calm
mz login            # Авторизация в аккаунте YouTube Music
```

---

## Установка

### Windows

Скачайте `musiczero-windows-x64.zip` из релиза GitHub и распакуйте три `.exe` в одну папку. Для сборки из исходников нужен Rust с MSVC: `cargo build --workspace --release --no-default-features --locked`. Токен Яндекс Музыки сохраните в `%APPDATA%\ymz\token` либо задайте `YM_TOKEN`. Для YouTube Music выполните `mz.exe login`; для загрузки аудио `yt-dlp.exe` должен быть в `PATH`. Запустите `mz.exe ymz` или `mz.exe youmz`, затем из второго окна PowerShell управляйте плеером через `mz.exe status`, `mz.exe next` и другие команды.

В Windows архив входят `mz.exe`, `ymz.exe`, `youmz.exe`. Трей и MPRIS доступны только в Linux. Команда `mz prev` пока не поддерживается.

### Linux: сборка из исходников

Установите Rust через [rustup](https://rustup.rs), затем установите системные зависимости для своей Linux-системы.

| Дистрибутив | Зависимости для сборки |
| --- | --- |
| Debian, Ubuntu, Linux Mint | `sudo apt install build-essential pkg-config libasound2-dev libsqlite3-dev` |
| Fedora | `sudo dnf install gcc pkgconf-pkg-config alsa-lib-devel sqlite-devel` |
| Arch Linux, Manjaro | `sudo pacman -S base-devel pkgconf alsa-lib sqlite` |
| openSUSE | `sudo zypper install gcc pkg-config alsa-devel sqlite3-devel` |
| Alpine Linux | `sudo apk add build-base pkgconf alsa-lib-dev sqlite-dev` |

### Сборка и установка:

Установить всё в `~/.local/bin` с помощью скрипта:

```sh
./install.sh
```

Или собрать вручную через Cargo:

```sh
cargo build --workspace --release --features ymz/tray,youmz/tray,mz/tray
```

Установить единую утилиту `mz` напрямую через Cargo:

```sh
cargo install --git https://github.com/BBQQYT/MusicZero.git mz --features tray --locked
```

Для установки конкретных модулей отдельно:

```sh
cargo install --git https://github.com/BBQQYT/MusicZero.git ymz --features tray --locked
cargo install --git https://github.com/BBQQYT/MusicZero.git youmz --features tray --locked
```

### Автоматические релизы

При отправке тега с префиксом `v`, например `v0.2.1`, GitHub Actions собирает архивы Linux и Windows и публикует их в GitHub Releases после успешной сборки обеих платформ.

Для другого каталога установки можно задать префикс: `MUSICZERO_PREFIX=/usr/local ./install.sh`.

---

## Системные требования для работы

- Аудиовыход, поддерживаемый ALSA/CPAL (PipeWire и PulseAudio полностью поддерживаются через ALSA-слой).
- Пользовательская D-Bus-сессия для MPRIS и трея.
- Панель или трей с поддержкой StatusNotifierItem (Waybar, Polybar, KDE, GNOME AppIndicator, XFCE и др.).
- `yt-dlp` в `PATH` для загрузки аудио YouMZ.
- OAuth-токен Яндекс Музыки для YMZ и авторизованная сессия YouTube Music для YouMZ.

---

## Настройка сервисов

### Настройка YMZ (Яндекс Музыка)

Сохраните OAuth-токен Яндекс Музыки в каталоге настроек XDG и ограничьте доступ:

```sh
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/ymz"
printf '%s\n' 'ВАШ_ТОКЕН_ЯНДЕКСА' > "${XDG_CONFIG_HOME:-$HOME/.config}/ymz/token"
chmod 600 "${XDG_CONFIG_HOME:-$HOME/.config}/ymz/token"
```

Вместо файла можно задать переменную окружения `YM_TOKEN`.

### Настройка YouMZ (YouTube Music)

При первом запуске выполните вход через команду:

```sh
mz login
```

*(или `youmz login`).* Либо сохраните cookie авторизованной сессии в `${XDG_CONFIG_HOME:-$HOME/.config}/youmz/cookie` (права `600`). При необходимости укажите прокси в файле `proxy` или через `YOUMZ_PROXY`, например `socks5h://127.0.0.1:2080`.

---

## Горячие клавиши (Sway / i3 / Hyprland)

Благодаря единой команде `mz` управление воспроизведением легко вешается на мультимедиа-клавиши:

```ini
# Sway / i3
bindsym XF86AudioPlay exec mz toggle
bindsym XF86AudioNext exec mz next
bindsym XF86AudioStop exec mz stop
```

---

## Лицензия

MusicZero распространяется по лицензии MIT.
