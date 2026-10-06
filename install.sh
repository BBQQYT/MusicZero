#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
build_dir=${CARGO_TARGET_DIR:-"$root/target"}

if ! command -v cargo >/dev/null 2>&1; then
    echo "Нужен Rust/Cargo: https://rustup.rs" >&2
    exit 1
fi

case "$(rustc -vV | sed -n 's/^host: //p')" in
    *-android|*-androideabi)
        prefix=${MUSICZERO_PREFIX:-${PREFIX:?Run the Android build inside Termux}}
        cargo build --manifest-path "$root/Cargo.toml" --workspace --release --no-default-features --locked --target-dir "$build_dir"
        ;;
    *)
        prefix=${MUSICZERO_PREFIX:-"$HOME/.local"}
        cargo build --manifest-path "$root/Cargo.toml" --workspace --release --locked --target-dir "$build_dir"
        ;;
esac

mkdir -p "$prefix/bin"
install -m 755 "$build_dir/release/mz" "$prefix/bin/mz"
for module in ymz youmz local icecast; do
    mkdir -p "$prefix/bin/modules/$module"
    install -m 644 "$root/modules/$module/module.json" "$prefix/bin/modules/$module/module.json"
    if [ -f "$root/modules/$module/README.md" ]; then
        install -m 644 "$root/modules/$module/README.md" "$prefix/bin/modules/$module/README.md"
    fi
    install -m 755 "$build_dir/release/$module-module" "$prefix/bin/modules/$module/$module-module"
done

echo "Установлено в $prefix/bin"
echo "Запуск: mz modules; mz start <ymz|youmz|local|icecast>"
echo "Управление: mz toggle, mz next, mz status, mz playlist, mz switch youmz."
echo "Настройки, трей и автозапуск: mz config"
if [ "${MUSICZERO_SETUP:-1}" != "0" ] && [ -t 0 ] && [ -t 1 ]; then
    "$prefix/bin/mz" config
fi
