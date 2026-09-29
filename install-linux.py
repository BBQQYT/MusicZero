#!/usr/bin/env python3
"""Install the newest published MusicZero Linux release for the current user."""

import hashlib
import json
import os
import platform
import shutil
import sys
import tarfile
import tempfile
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
    with tarfile.open(archive, "r:gz") as bundle:
        members = {}
        for source_name, _, _ in FILES:
            member = bundle.getmember(source_name)
            if not member.isfile() or member.size > 100 * 1024 * 1024:
                raise RuntimeError(f"Некорректный файл в архиве: {source_name}")
            members[source_name] = member
        for source_name, target_name, mode in FILES:
            source = bundle.extractfile(members[source_name])
            if source is None:
                raise RuntimeError(f"Не удалось прочитать {source_name}")
            target = destination / target_name
            target.parent.mkdir(parents=True, exist_ok=True)
            with tempfile.NamedTemporaryFile(dir=target.parent, delete=False) as temporary:
                temporary_path = Path(temporary.name)
                try:
                    with source:
                        shutil.copyfileobj(source, temporary)
                    os.chmod(temporary_path, mode)
                    os.replace(temporary_path, target)
                finally:
                    temporary_path.unlink(missing_ok=True)


def main():
    if sys.platform != "linux" or platform.machine().lower() not in ("x86_64", "amd64"):
        raise RuntimeError("Сейчас готовый Linux архив доступен только для x86_64")
    prefix = Path(os.environ.get("MUSICZERO_PREFIX") or "~/.local").expanduser().resolve()
    tag, url, expected_hash = release_asset()
    print(f"MusicZero {tag}: загрузка {ASSET}...", flush=True)
    with tempfile.TemporaryDirectory() as temporary_dir:
        archive = Path(temporary_dir) / ASSET
        checksum = hashlib.sha256()
        with open_url(url) as response, archive.open("wb") as output:
            while block := response.read(1024 * 1024):
                output.write(block)
                checksum.update(block)
        if checksum.hexdigest() != expected_hash:
            raise RuntimeError("SHA-256 загруженного архива не совпадает с релизом")
        install(archive, prefix / "bin")
    executable = prefix / "bin/mz"
    print(f"Установлено: {executable}")
    if str(executable.parent) in os.environ.get("PATH", "").split(os.pathsep):
        print("Запуск: mz modules; mz start ymz")
    else:
        print(f"Запуск: {executable} modules; {executable} start ymz")
        print(f"Для команды mz добавьте {executable.parent} в PATH.")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, tarfile.TarError, urllib.error.URLError, RuntimeError) as error:
        print(f"Ошибка установки MusicZero: {error}", file=sys.stderr)
        sys.exit(1)
