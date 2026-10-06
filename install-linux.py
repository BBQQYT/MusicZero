#!/usr/bin/env python3
"""Install the newest published MusicZero Linux release for the current user."""

import argparse
import contextlib
import hashlib
import json
import os
import platform
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path


API = "https://api.github.com/repos/BBQQYT/MusicZero/releases"
ASSET = "musiczero-linux-x64.tar.gz"
FILES = (
    ("musiczero/modules/ymz/module.json", "modules/ymz/module.json", 0o644),
    ("musiczero/modules/ymz/ymz-module", "modules/ymz/ymz-module", 0o755),
    ("musiczero/modules/youmz/module.json", "modules/youmz/module.json", 0o644),
    ("musiczero/modules/youmz/youmz-module", "modules/youmz/youmz-module", 0o755),
    ("musiczero/modules/local/module.json", "modules/local/module.json", 0o644),
    ("musiczero/modules/local/README.md", "modules/local/README.md", 0o644),
    ("musiczero/modules/local/local-module", "modules/local/local-module", 0o755),
    ("musiczero/modules/icecast/module.json", "modules/icecast/module.json", 0o644),
    ("musiczero/modules/icecast/README.md", "modules/icecast/README.md", 0o644),
    ("musiczero/modules/icecast/icecast-module", "modules/icecast/icecast-module", 0o755),
    ("musiczero/mz", "mz", 0o755),
)


def open_url(url, *, api=False):
    headers = {"User-Agent": "MusicZero-Linux-Installer"}
    if api:
        headers["Accept"] = "application/vnd.github+json"
    request = urllib.request.Request(
        url,
        headers=headers,
    )
    return urllib.request.urlopen(request, timeout=30)


def release_asset():
    tag = os.environ.get("MUSICZERO_TAG")
    url = f"{API}/tags/{urllib.parse.quote(tag, safe='')}" if tag else f"{API}?per_page=30"
    with open_url(url, api=True) as response:
        data = json.load(response)
    releases = [data] if tag else data
    for release in releases:
        if release.get("draft"):
            continue
        for asset in release.get("assets", []):
            if asset.get("name") == ASSET and asset.get("state") == "uploaded":
                digest = asset.get("digest") or ""
                if not digest.startswith("sha256:"):
                    raise RuntimeError("GitHub не сообщил SHA-256 архива")
                return release["tag_name"], asset["browser_download_url"], digest[7:]
    raise RuntimeError("Linux архив не найден среди опубликованных релизов")


def install(archive, destination):
    """Stage every file before replacing anything; restore the old set on failure."""
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination) as temporary_dir:
        staging = Path(temporary_dir)
        targets = []
        with tarfile.open(archive, "r:gz") as bundle:
            names = bundle.getnames()
            if len(names) != len(set(names)):
                raise RuntimeError("Duplicate archive entries")
            absent = {module for module in ("local", "icecast")
                      if f"musiczero/modules/{module}/module.json" not in names}
            for index, (source_name, target_name, mode) in enumerate(FILES):
                if any(source_name.startswith(f"musiczero/modules/{module}/") for module in absent):
                    continue
                member = bundle.getmember(source_name)
                if not member.isfile() or member.size > 100 * 1024 * 1024:
                    raise RuntimeError(f"Invalid archive file: {source_name}")
                target = destination / target_name
                # Do not follow a pre-existing module directory symlink on updates.
                for parent in target.parents:
                    if parent == destination:
                        break
                    if parent.is_symlink():
                        raise RuntimeError(f"Installation directory is a symlink: {parent}")
                if target.is_symlink() or (target.exists() and not target.is_file()):
                    raise RuntimeError(f"Invalid installation target: {target}")
                staged = staging / str(index)
                with bundle.extractfile(member) as source, staged.open("wb") as output:
                    shutil.copyfileobj(source, output)
                    output.flush()
                    os.fsync(output.fileno())
                os.chmod(staged, mode)
                targets.append((staged, target))
        replaced = []
        try:
            for index, (staged, target) in enumerate(targets):
                target.parent.mkdir(parents=True, exist_ok=True)
                backup = staging / f"backup-{index}" if target.exists() else None
                if backup is not None:
                    shutil.copy2(target, backup)
                os.replace(staged, target)
                replaced.append((target, backup))
        except BaseException:
            for target, backup in reversed(replaced):
                if backup is None:
                    target.unlink()
                else:
                    os.replace(backup, target)
            raise


class Cancelled(Exception):
    pass


class TerminalUI:
    """Curses prompts with normal terminal ownership between screens and commands."""
    def __init__(self):
        try:
            import curses
        except ImportError as error:
            raise RuntimeError("Python curses is missing; install it or use --non-interactive") from error
        self.curses = curses

    def screen(self, title, draw):
        def run(window):
            window.keypad(True)
            while True:
                window.erase()
                rows, cols = window.getmaxyx()
                if rows < 12 or cols < 50:
                    window.addnstr(0, 0, "Resize terminal to at least 50 x 12", max(1, cols - 1))
                    window.refresh()
                    if window.getch() == 27:
                        raise Cancelled()
                    continue
                window.addnstr(1, 2, "MusicZero | " + title, cols - 4, self.curses.A_BOLD)
                window.addnstr(rows - 2, 2, "Arrows: select   Enter: confirm   Esc: cancel", cols - 4)
                result = draw(window, rows, cols)
                if result is not None:
                    return result
        return self.curses.wrapper(run)

    def choose(self, title, options):
        selected = 0
        def draw(window, rows, cols):
            nonlocal selected
            count = rows - 6
            start = max(0, selected - count + 1)
            for index in range(start, min(len(options), start + count)):
                window.addnstr(3 + index - start, 2, options[index], cols - 4,
                              self.curses.A_REVERSE if index == selected else 0)
            window.refresh()
            key = window.getch()
            if key == 27:
                raise Cancelled()
            if key in (10, 13, self.curses.KEY_ENTER):
                return selected
            if key in (self.curses.KEY_UP, ord("k")):
                selected = (selected - 1) % len(options)
            elif key in (self.curses.KEY_DOWN, ord("j")):
                selected = (selected + 1) % len(options)
        return self.screen(title, draw)

    def text(self, title, default="", secret=False):
        value = list(default)
        def draw(window, rows, cols):
            shown = "*" * len(value) if secret else "".join(value)
            window.addnstr(4, 2, "> " + shown[-(cols - 8):], cols - 4)
            window.refresh()
            key = window.get_wch()
            if key == "\x1b":
                raise Cancelled()
            if key in ("\n", "\r", self.curses.KEY_ENTER):
                return "".join(value)
            if key in ("\x7f", "\b", self.curses.KEY_BACKSPACE):
                if value:
                    value.pop()
            elif key == "\x15":
                value.clear()
            elif isinstance(key, str) and key.isprintable():
                value.append(key)
        return self.screen(title + " (Ctrl+U clears)", draw)

    def notice(self, title, message):
        def draw(window, rows, cols):
            lines = []
            for line in message.splitlines():
                lines.extend(textwrap.wrap(line, cols - 4) or [""])
            for index, line in enumerate(lines[:rows - 6]):
                window.addnstr(index + 3, 2, line, cols - 4)
            window.refresh()
            key = window.getch()
            if key == 27:
                raise Cancelled()
            if key in (10, 13, self.curses.KEY_ENTER):
                return True
        self.screen(title, draw)

    def yes(self, title):
        return self.choose(title, ["No", "Yes"]) == 1


@contextlib.contextmanager
def terminal(enabled):
    if not enabled:
        yield None
        return
    # Python has already consumed the piped script. Prompts and module login must
    # read the controlling terminal, rather than the exhausted curl pipe.
    tty_fd = os.open("/dev/tty", os.O_RDWR)
    try:
        saved = [os.dup(fd) for fd in (0, 1, 2)]
        try:
            for fd in (0, 1, 2):
                os.dup2(tty_fd, fd)
            yield TerminalUI()
        finally:
            sys.stdout.flush()
            sys.stderr.flush()
            for fd, original in enumerate(saved):
                os.dup2(original, fd)
                os.close(original)
    finally:
        os.close(tty_fd)


def has_terminal():
    if os.environ.get("TERM", "dumb") == "dumb":
        return False
    try:
        fd = os.open("/dev/tty", os.O_RDWR)
        os.close(fd)
        return True
    except OSError:
        return False


def run_command(args, *, capture=False):
    result = subprocess.run([str(arg) for arg in args], text=True,
                            stdout=subprocess.PIPE if capture else None,
                            stderr=subprocess.PIPE if capture else None)
    if result.returncode:
        # Never include arguments here: they may contain an authentication secret.
        raise RuntimeError(result.stderr.strip() if capture else "Command failed; see terminal output")
    return result.stdout


def dependencies(ui, module):
    needed = {"local": ["ffmpeg", "ffprobe"], "icecast": ["ffmpeg"],
              "youmz": ["yt-dlp", "ffmpeg"], "ymz": []}[module]
    missing = [name for name in needed if not shutil.which(name)]
    if not missing:
        return True
    packages = sorted({"ffmpeg" if name == "ffprobe" else name for name in missing})
    managers = [("apt-get", ["install", "-y"]), ("dnf", ["install", "-y"]),
                ("pacman", ["-S", "--needed", "--noconfirm"]),
                ("zypper", ["--non-interactive", "install"])]
    manager = next(((name, flags) for name, flags in managers if shutil.which(name)), None)
    print("Missing dependencies: " + ", ".join(missing), flush=True)
    if manager and ui.yes("Install " + ", ".join(packages) + " with " + manager[0] + "?"):
        name, flags = manager
        sudo = [] if os.geteuid() == 0 else ["sudo"]
        if sudo and not shutil.which("sudo"):
            raise RuntimeError("sudo is missing; install dependencies with your package manager")
        if name == "apt-get":
            run_command(sudo + [name, "update"])
        run_command(sudo + [name] + flags + packages)
        missing = [name for name in needed if not shutil.which(name)]
    if missing:
        ui.choose("Install dependencies manually, then retry", ["Back to setup"])
        return False
    return True


def edit_settings(ui, executable, module):
    """Expose scalar provider settings without round-tripping masked station secrets."""
    if 'mz config' in run_command([executable, 'help'], capture=True):
        run_command([executable, 'config', module])
        return
    while True:
        response = json.loads(run_command([executable, "settings", module], capture=True))
        if not isinstance(response, dict):
            raise RuntimeError("Provider settings must be a JSON object")
        # The CLI prints the settings object itself; direct protocol responses
        # from older integrations may still include the settings wrapper.
        settings = response.get("settings", response)
        if not isinstance(settings, dict):
            raise RuntimeError("Provider settings must be a JSON object")
        keys = [key for key, value in settings.items() if isinstance(value, (str, int, bool))]
        if module == "icecast":
            # These editable aliases apply to the station chosen by `station`.
            selected = next(station for station in settings["stations"]
                            if station["id"] == settings["station"])
            settings.update({key: selected[key] for key in ("url", "name", "username", "password")})
            keys += ["url", "name", "username", "password", "server", "mountpoint"]
        options = [key + " = " + str(settings.get(key, "")) for key in keys] + ["Back"]
        chosen = ui.choose(module + " settings", options)
        if chosen == len(keys):
            return
        key = keys[chosen]
        current = settings.get(key, "")
        if isinstance(current, bool):
            value = ["true", "false"][ui.choose(key, ["true", "false"])]
        else:
            secret = key in ("password", "token", "session")
            value = ui.text(key, "" if secret else str(current), secret=secret)
            if secret and not value:
                continue
        try:
            run_command([executable, "set", module, key, value], capture=True)
        except RuntimeError as error:
            print(str(error), file=sys.stderr, flush=True)
            ui.notice("Setting rejected", str(error))


def configure(ui, executable):
    # Ignore dev module overrides: configure the installation we just created.
    os.environ.pop("MZ_MODULES_DIR", None)
    help_text = run_command([executable, 'help'], capture=True)
    native_config = 'mz config' in help_text
    desktop_setup = 'mz tray' in help_text and 'mz service' in help_text
    if not native_config:
        print("Этот релиз ещё не содержит `mz config`. Для TUI обновите плеер после публикации новой сборки.",
              flush=True)
    listing = run_command([executable, "modules"], capture=True)
    available = [module for module in ("local", "icecast", "ymz", "youmz")
                 if any(line.startswith(module + " —") for line in listing.splitlines())]
    labels = {"local": "Local music folder", "icecast": "Icecast radio",
              "ymz": "Yandex Music login", "youmz": "YouTube Music browser login"}
    if desktop_setup:
        configure_tray(ui, executable)
    extra = ["Tray / Трей", "Install service / Установить сервис", "All settings / Все настройки"] if desktop_setup else []
    while True:
        selected = ui.choose("Configure sources", [labels[m] for m in available] + extra + ["Finish setup"])
        if selected == len(available) + len(extra):
            return
        try:
            if selected >= len(available):
                action = selected - len(available)
                if action == 0:
                    configure_tray(ui, executable)
                elif action == 1:
                    if not available:
                        raise RuntimeError("No providers are installed")
                    index = ui.choose("Service source / Источник для сервиса", [labels[m] for m in available] + ["Back / Назад"])
                    if index == len(available):
                        continue
                    module = available[index]
                    if not dependencies(ui, module):
                        continue
                    run_command([executable, "service", "install", module], capture=True)
                    ui.notice("Service installed / Сервис установлен", "Autostart at login is enabled.\nАвтозапуск при входе включён.\n" +
                              "Manage it in mz config → Service / autostart.\nУправление: mz config → Сервис / автозапуск.")
                    if ui.yes("Start service now? / Запустить сервис сейчас?"):
                        run_command([executable, "service", "start"], capture=True)
                else:
                    run_command([executable, "config"])
                continue
            module = available[selected]
            if not dependencies(ui, module):
                continue
            if module == "local":
                path = ui.text("Music folder", str(Path.home() / "Music"))
                run_command([executable, "set", module, "path", str(Path(path).expanduser())], capture=True)
                run_command([executable, "set", module, "recursive",
                             "true" if ui.yes("Include subfolders?") else "false"], capture=True)
                run_command([executable, "set", module, "shuffle",
                             "true" if ui.yes("Shuffle tracks?") else "false"], capture=True)
                if ui.yes("Configure MIDI SoundFont? (requires fluidsynth)"):
                    soundfont = ui.text("SF2/SF3 file path")
                    run_command([executable, "set", module, "soundfont",
                                 str(Path(soundfont).expanduser())], capture=True)
            elif module == "icecast":
                # Use the provider's selected station; preserve existing other stations.
                for key, title in (("url", "Station HTTP(S) URL"), ("name", "Station name")):
                    value = ui.text(title)
                    run_command([executable, "set", module, key, value], capture=True)
                if ui.yes("Station requires username/password?"):
                    for key in ("username", "password"):
                        value = ui.text("Station " + key, secret=key == "password")
                        run_command([executable, "set", module, key, value], capture=True)
            else:
                print("Follow the login instructions below.", flush=True)
                run_command([executable, "login", module])
            if ui.yes("Edit advanced " + module + " settings?"):
                edit_settings(ui, executable, module)
            if ui.yes("Start " + module + " now? Ctrl+C returns to setup"):
                try:
                    run_command([executable, "start", module])
                except KeyboardInterrupt:
                    pass
        except RuntimeError as error:
            print(str(error), file=sys.stderr, flush=True)
            ui.notice("Setup failed; Enter returns to source menu", str(error))


def configure_tray(ui, executable):
    current = run_command([executable, "tray"], capture=True).strip()
    chosen = ui.choose("Tray icon / Значок трея", ["Keep current / Сохранить: " + current,
                                                    "Enable / Включить", "Disable / Выключить"])
    if chosen:
        run_command([executable, "tray", "on" if chosen == 1 else "off"], capture=True)


def add_to_path(ui, directory):
    if str(directory) in os.environ.get("PATH", "").split(os.pathsep):
        return
    shell = Path(os.environ.get("SHELL", "")).name
    config = {"bash": Path.home() / ".bashrc", "zsh": Path.home() / ".zshrc",
              "fish": Path.home() / ".config/fish/config.fish"}.get(shell)
    if config and ui.yes("Add mz to PATH in " + str(config) + "?"):
        if shell == "fish":
            # Fish double quotes only expand $, backslash and double quote.
            quoted = '"' + str(directory).replace("\\", "\\\\").replace('"', '\\"').replace("$", "\\$") + '"'
            line = "fish_add_path " + quoted
        else:
            line = "export PATH=" + shlex.quote(str(directory)) + ':"$PATH"'
        config.parent.mkdir(parents=True, exist_ok=True)
        previous = config.read_text() if config.exists() else ""
        if line not in previous.splitlines():
            with config.open("a") as output:
                output.write("\n# MusicZero\n" + line + "\n")
    os.environ["PATH"] = str(directory) + os.pathsep + os.environ.get("PATH", "")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--non-interactive", action="store_true", help="install only, no terminal prompts")
    mode.add_argument("--tui", action="store_true", help="require interactive terminal setup")
    parser.add_argument("--setup-only", action="store_true", help="configure an existing installation")
    args = parser.parse_args()
    interactive = not args.non_interactive and has_terminal()
    if args.tui and not interactive:
        raise RuntimeError("TUI requires a controlling terminal and TERM; use --non-interactive in automation")
    if args.setup_only and args.non_interactive:
        parser.error("--setup-only cannot be combined with --non-interactive")
    with terminal(interactive) as ui:
        perform_install(args, ui)


def perform_install(args, ui):
    if sys.platform != "linux" or platform.machine().lower() not in ("x86_64", "amd64"):
        raise RuntimeError("Сейчас готовый Linux архив доступен только для x86_64")
    prefix = Path(os.environ.get("MUSICZERO_PREFIX") or "~/.local").expanduser().resolve()
    if ui and not args.setup_only:
        value = ui.text("Installation prefix", str(prefix)).strip()
        if not value:
            raise RuntimeError("Installation prefix cannot be empty")
        prefix = Path(value).expanduser().resolve()
    if args.setup_only:
        if not ui:
            raise RuntimeError("--setup-only requires a terminal")
        add_to_path(ui, prefix / "bin")
        configure(ui, prefix / "bin/mz")
        return
    tag, url, expected_hash = release_asset()
    print(f"MusicZero {tag}: загрузка {ASSET}...", flush=True)
    with tempfile.TemporaryDirectory() as temporary_dir:
        archive = Path(temporary_dir) / ASSET
        checksum = hashlib.sha256()
        with open_url(url) as response, archive.open("wb") as output:
            while block := response.read(1024 * 1024):
                if output.tell() + len(block) > 512 * 1024 * 1024:
                    raise RuntimeError("Release archive exceeds 512 MiB")
                output.write(block)
                checksum.update(block)
        if checksum.hexdigest() != expected_hash:
            raise RuntimeError("SHA-256 загруженного архива не совпадает с релизом")
        install(archive, prefix / "bin")
    executable = prefix / "bin/mz"
    if ui:
        add_to_path(ui, executable.parent)
        configure(ui, executable)
    print(f"Установлено: {executable}")
    if str(executable.parent) in os.environ.get("PATH", "").split(os.pathsep):
        print("Запуск: mz modules; mz start ymz")
    else:
        print(f"Запуск: {executable} modules; {executable} start ymz")
        print(f"Для команды mz добавьте {executable.parent} в PATH.")


if __name__ == "__main__":
    try:
        main()
    except (Cancelled, KeyboardInterrupt):
        print("Setup cancelled. Any completed installation/settings are preserved.", file=sys.stderr)
        sys.exit(130)
    except (OSError, ValueError, KeyError, tarfile.TarError, urllib.error.URLError, RuntimeError) as error:
        print(f"Ошибка установки MusicZero: {error}", file=sys.stderr)
        sys.exit(1)
