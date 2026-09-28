#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
prefix=${MUSICZERO_PREFIX:-"$HOME/.local"}

if ! command -v cargo >/dev/null 2>&1; then
    echo "Нужен Rust/Cargo: https://rustup.rs" >&2
    exit 1
fi

cargo build --manifest-path "$root/Cargo.toml" --workspace --release \
    --features ymz/tray,youmz/tray,mz/tray

mkdir -p "$prefix/bin"
for binary in mz ymz ymz-tray youmz youmz-tray; do
    install -m 755 "$root/target/release/$binary" "$prefix/bin/$binary"
done

echo "Установлено в $prefix/bin"
echo "Запуск: mz ymz (Яндекс Музыка) или mz youmz (YouTube Music) — трей включён сразу!"
echo "Управление: mz toggle, mz next, mz prev, mz status, mz playlist, mz wave."
