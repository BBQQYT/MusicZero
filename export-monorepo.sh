#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
destination=${1:-}

if [ -z "$destination" ]; then
    echo "Использование: $0 КАТАЛОГ_НОВОГО_РЕПОЗИТОРИЯ" >&2
    exit 2
fi
if [ -e "$destination" ]; then
    echo "Каталог уже существует: $destination" >&2
    exit 2
fi

mkdir -p "$destination"
tar -C "$root" \
    --exclude='*/.git' --exclude='*/target' \
    --exclude='MCZ/Cargo.lock' --exclude='YMZ/Cargo.lock' --exclude='YouMZ/Cargo.lock' \
    --exclude='YMZ/.github' --exclude='YouMZ/.github' \
    --exclude='*.env' --exclude='.env*' \
    --exclude='*/cookie' --exclude='*/token' --exclude='*/client_secret' \
    -cf - Cargo.toml Cargo.lock .gitignore README-MCZ.md \
    install.sh bootstrap.sh .github MCZ YMZ YouMZ |
    tar -C "$destination" -xf -

echo "Готово: $destination"
echo "Проверьте файлы перед публикацией, затем создайте Git-репозиторий в этом каталоге."
