use super::terminal::{Row, Ui};
use crate::{config::Language, Result};
use serde_json::Value;

pub fn label(lang: Language, key: &str) -> &str {
    let (ru, en) = match key {
        "path" => ("Папка музыки", "Music folder"),
        "recursive" => ("Включать подпапки", "Include subfolders"),
        "hidden" => ("Скрытые файлы", "Hidden files"),
        "shuffle" => ("Перемешивать треки", "Shuffle tracks"),
        "scan_limit" => ("Лимит сканирования", "Scan limit"),
        "probe_timeout" => ("Таймаут анализа (сек.)", "Probe timeout (seconds)"),
        "ffmpeg" => ("Путь к FFmpeg", "FFmpeg executable"),
        "ffprobe" => ("Путь к ffprobe", "ffprobe executable"),
        "fluidsynth" => ("Путь к FluidSynth", "FluidSynth executable"),
        "soundfont" => ("Банк звуков MIDI", "MIDI SoundFont"),
        "station" => ("Выбранная станция", "Selected station"),
        "stations" => ("Радиостанции", "Radio stations"),
        "id" => ("Идентификатор", "Identifier"),
        "name" => ("Название", "Name"),
        "url" => ("Адрес потока", "Stream URL"),
        "server" => ("Адрес сервера", "Server URL"),
        "mountpoint" => ("Путь потока", "Mountpoint"),
        "username" => ("Логин", "Username"),
        "password" => ("Пароль", "Password"),
        "proxy" => ("Прокси", "Proxy"),
        "user_agent" => ("User-Agent", "User-Agent"),
        "reconnect" => ("Переподключаться", "Reconnect"),
        "reconnect_delay_max" => (
            "Задержка переподключения (сек.)",
            "Reconnect delay (seconds)",
        ),
        "timeout" => ("Сетевой таймаут (сек.)", "Network timeout (seconds)"),
        "buffer_ms" => ("Буфер (мс)", "Buffer (ms)"),
        "tls_verify" => ("Проверять HTTPS-сертификат", "Verify HTTPS certificate"),
        "ca_file" => ("Файл сертификатов CA", "CA certificate file"),
        "mood" => ("Настроение волны", "Wave mood"),
        "diversity" => ("Разнообразие волны", "Wave variety"),
        "language" => ("Язык песен", "Song language"),
        "token" => ("Токен Яндекс Музыки", "Yandex Music token"),
        "session" => ("Сессия YouTube (вручную)", "YouTube session (manual)"),
        "browser" => ("Браузер для входа", "Login browser"),
        "modules_dir" => ("Папка модулей", "Modules folder"),
        "log_filter" => ("Уровень журналирования", "Log filter"),
        "temp_dir" => ("Папка временного аудио", "Temporary audio folder"),
        "tray_enabled" => ("Показывать значок в трее", "Show tray icon"),
        "notifications_enabled" => (
            "Кнопки в уведомлении Android",
            "Android notification controls",
        ),
        _ => return key,
    };
    lang.text(ru, en)
}
pub fn hint(lang: Language, key: &str) -> &str {
    let (ru, en) = match key {
        "path" => ("Существующая папка с музыкой. Можно вставить путь с пробелами или ~/Music.", "Existing music folder. Spaces and ~/Music are supported."),
        "recursive" => ("Искать музыку во всех подпапках.", "Search all subfolders for music."),
        "hidden" => ("Включать файлы и папки, имя которых начинается с точки.", "Include files and folders whose names start with a dot."),
        "shuffle" => ("Перемешивать библиотеку при пополнении очереди.", "Shuffle the library whenever the queue is refilled."),
        "scan_limit" => ("От 1 до 100000 файлов и папок.", "1–100000 examined files and folders."),
        "probe_timeout" => ("От 1 до 30 секунд на файл.", "1–30 seconds per file."),
        "soundfont" => ("Существующий SF2/SF3 для MIDI. Пустое значение отключает банк.", "Existing SF2/SF3 bank for MIDI. Empty clears the bank."),
        "ffmpeg" | "ffprobe" | "fluidsynth" => ("Имя программы из PATH или полный путь к исполняемому файлу.", "Program name on PATH or full executable path."),
        "stations" => ("Добавление, редактирование и удаление станций по полям.", "Add, edit and remove stations using a form."),
        "station" => ("Станция для плейлиста «По умолчанию». Для воспроизведения также выберите плейлист.", "Station used by the Default playlist. Select a playlist to choose playback."),
        "id" => ("Уникальное имя: латинские буквы, цифры, дефис и подчёркивание; до 64 символов.", "Unique ID: ASCII letters, digits, dash, underscore; up to 64 characters."),
        "url" => ("Прямой HTTP(S) адрес аудиопотока. Логин и пароль вводятся отдельно.", "Direct HTTP(S) audio stream URL. Enter credentials in separate fields."),
        "server" => ("HTTP(S) адрес сервера. Текущий путь потока сохранится.", "HTTP(S) server URL. Keeps the current mountpoint."),
        "mountpoint" => ("Путь начинается с /, например /live.mp3. Сначала задайте адрес потока.", "Path begins with /, e.g. /live.mp3. Set the stream URL first."),
        "password" | "token" | "session" => ("Значение скрыто. Введите новое для замены; Enter без ввода сохраняет прежнее.", "Value is hidden. Type a replacement; Enter without input keeps the existing value."),
        "proxy" => ("Пусто — без прокси (если он не задан окружением). Icecast: http://; YouMZ также поддерживает SOCKS.", "Empty disables proxy unless supplied by the environment. Icecast: http://; YouMZ also supports SOCKS."),
        "browser" => ("Путь или имя Firefox/Chromium/Chrome. Пусто — автоопределение. YOUMZ_BROWSER имеет приоритет.", "Firefox/Chromium/Chrome executable. Empty means auto-detect. YOUMZ_BROWSER takes precedence."),
        "buffer_ms" => ("От 250 до 10000 миллисекунд.", "250–10000 milliseconds."),
        "timeout" => ("От 1 до 120 секунд.", "1–120 seconds."),
        "reconnect_delay_max" => ("От 1 до 60 секунд.", "1–60 seconds."),
        "tls_verify" => ("Проверять сертификат HTTPS-сервера при подключении.", "Verify the HTTPS server certificate when connecting."),
        "ca_file" => ("Файл CA для частного HTTPS-сервера. Пусто — системные сертификаты.", "CA file for a private HTTPS server. Empty uses system certificates."),
        "mood" | "diversity" | "language" => ("Настройка «Моей волны» на сервере Яндекса. Нужны вход и сеть.", "Yandex My Wave preference. Requires login and network access."),
        "modules_dir" => ("Пусто — modules рядом с mz. MZ_MODULES_DIR имеет приоритет. Плеер нужно перезапустить.", "Empty uses modules beside mz. MZ_MODULES_DIR takes precedence. Restart the player."),
        "log_filter" => ("Пусто — warn,mz=info. Например debug или warn. RUST_LOG имеет приоритет; нужен перезапуск.", "Empty uses warn,mz=info. Examples: debug, warn. RUST_LOG takes precedence; restart required."),
        "temp_dir" => ("Существующая папка. Пусто — системная. При пустом значении учитываются TMPDIR/TMP/TEMP.", "Existing folder. Empty uses the system default. When empty, uses TMPDIR/TMP/TEMP."),
        "tray_enabled" => ("Значок и меню в трее Linux. Изменение применяется после перезапуска плеера.", "Linux tray icon and menu. Restart the player to apply."),
        "notifications_enabled" => ("Назад, пауза и вперёд вне консоли. Нужны приложение Termux:API и pkg install termux-api. Перезапустите плеер.", "Previous, pause and next outside the terminal. Requires the Termux:API app and pkg install termux-api. Restart the player."),
        _ => ("Enter — изменить; изменения сохраняются сразу.", "Enter to edit; changes are saved immediately."),
    };
    lang.text(ru, en)
}
pub fn secret(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    ["password", "token", "session", "cookie", "secret", "proxy"]
        .iter()
        .any(|s| key.contains(s))
}
fn options(module: &str, key: &str) -> Vec<(&'static str, &'static str, &'static str)> {
    match (module, key) {
        ("ymz", "mood") => vec![
            ("all", "Любое", "Any"),
            ("fun", "Весёлое", "Cheerful"),
            ("active", "Энергичное", "Energetic"),
            ("calm", "Спокойное", "Calm"),
            ("sad", "Грустное", "Sad"),
        ],
        ("ymz", "diversity") => vec![
            ("default", "Обычное", "Balanced"),
            ("favorite", "Любимое", "Favorites"),
            ("popular", "Популярное", "Popular"),
            ("discover", "Открытия", "Discover"),
        ],
        ("ymz", "language") => vec![
            ("any", "Любой", "Any"),
            ("russian", "Русский", "Russian"),
            ("not-russian", "Иностранный", "Non-Russian"),
        ],
        _ => vec![],
    }
}
pub fn display(lang: Language, module: &str, key: &str, value: &Value) -> String {
    if secret(key) {
        return if value.as_str().is_none_or(str::is_empty) {
            lang.text("не задано", "not set").into()
        } else {
            "••••••".into()
        };
    }
    if let Some(value) = value.as_bool() {
        return lang
            .text(
                if value { "Да" } else { "Нет" },
                if value { "Yes" } else { "No" },
            )
            .into();
    }
    if let Some((_, ru, en)) = options(module, key)
        .iter()
        .find(|(id, _, _)| value.as_str() == Some(id))
    {
        return lang.text(ru, en).to_string();
    }
    if value.is_null() {
        return lang.text("недоступно", "unavailable").into();
    }
    if let Some(values) = value.as_array() {
        return values.len().to_string();
    }
    if value.as_str() == Some("") {
        return lang.text("по умолчанию / пусто", "default / empty").into();
    }
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
pub async fn edit(ui: &mut Ui, module: &str, key: &str, value: &Value) -> Result<Option<String>> {
    let options = options(module, key);
    if value.is_boolean() || !options.is_empty() {
        let options = if value.is_boolean() {
            vec![("true", "Да", "Yes"), ("false", "Нет", "No")]
        } else {
            options
        };
        let current = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
        let mut selected = options
            .iter()
            .position(|(id, _, _)| *id == current)
            .unwrap_or(0);
        let rows: Vec<_> = options
            .iter()
            .map(|(_, ru, en)| Row::new(ui.tr(ru, en), hint(ui.lang, key)))
            .collect();
        return Ok(ui
            .choose(label(ui.lang, key), &rows, &mut selected)
            .await?
            .map(|i| options[i].0.to_owned()));
    }
    let initial = value.as_str().map(str::to_owned).unwrap_or_else(|| {
        if value.is_null() {
            String::new()
        } else {
            value.to_string()
        }
    });
    let changed = ui
        .input(
            label(ui.lang, key),
            hint(ui.lang, key),
            &initial,
            secret(key),
        )
        .await?;
    if let Some(text) = &changed {
        if value.is_number() {
            let _: serde_json::Number = serde_json::from_str(text)?;
        }
        if value.is_array() || value.is_object() {
            let _: Value = serde_json::from_str(text)?;
        }
    }
    Ok(changed)
}
