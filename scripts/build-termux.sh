#!/bin/sh
# Cross-build the native Termux alpha using the official Android NDK.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
ndk=${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-${ANDROID_NDK:-}}}
if [ -z "$ndk" ]; then
    echo 'Set ANDROID_NDK_HOME to an Android NDK (r27 or newer).' >&2
    exit 1
fi
toolchain="$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin"
if [ ! -x "$toolchain/aarch64-linux-android24-clang" ]; then
    echo 'The Linux x64 NDK toolchain with Android API 24 is required.' >&2
    exit 1
fi
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$toolchain/aarch64-linux-android24-clang"
export CC_aarch64_linux_android="$toolchain/aarch64-linux-android24-clang"
export CXX_aarch64_linux_android="$toolchain/aarch64-linux-android24-clang++"
export AR_aarch64_linux_android="$toolchain/llvm-ar"
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--sysroot=\"$ndk/toolchains/llvm/prebuilt/linux-x86_64/sysroot\" --target=aarch64-linux-android24"
cargo build --manifest-path "$root/Cargo.toml" --workspace --release --no-default-features --locked --target aarch64-linux-android
