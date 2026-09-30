#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
prefix=${MUSICZERO_PREFIX:-"$HOME/.local"}

if ! command -v cargo >/dev/null 2>&1; then
    echo "Нужен Rust/Cargo: https://rustup.rs" >&2
    exit 1
fi

cargo build --manifest-path "$root/Cargo.toml" --workspace --release --locked

mkdir -p "$prefix/bin"
install -m 755 "$root/target/release/mz" "$prefix/bin/mz"
for module in ymz youmz local icecast; do
    mkdir -p "$prefix/bin/modules/$module"
    install -m 644 "$root/modules/$module/module.json" "$prefix/bin/modules/$module/module.json"
    if [ -f "$root/modules/$module/README.md" ]; then
        install -m 644 "$root/modules/$module/README.md" "$prefix/bin/modules/$module/README.md"
    fi
    install -m 755 "$root/target/release/$module-module" "$prefix/bin/modules/$module/$module-module"
done

echo "Установлено в $prefix/bin"
echo "Запуск: mz modules; mz start <ymz|youmz|local|icecast>"
echo "Управление: mz toggle, mz next, mz status, mz playlist, mz switch youmz."
