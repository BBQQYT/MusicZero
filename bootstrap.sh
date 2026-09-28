#!/bin/sh
set -eu

repo=${MUSICZERO_REPO:-https://github.com/BBQQYT/MusicZero.git}
ref=${MUSICZERO_REF:-main}
checkout=$(mktemp -d)
trap 'rm -rf -- "$checkout"' EXIT HUP INT TERM

if ! command -v git >/dev/null 2>&1; then
    echo "Нужен git для загрузки исходников" >&2
    exit 1
fi

git clone --depth 1 --branch "$ref" "$repo" "$checkout/source"
"$checkout/source/install.sh"
