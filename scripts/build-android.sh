#!/bin/sh
# Builds the Android application and its bundled Rust engine.
#
# Cross-compiles the whole workspace for aarch64 (API 26) with the Android
# NDK, stages the binaries as jniLibs plus the module manifests as assets and
# assembles the APK. Requires ANDROID_NDK_HOME (r27 or newer), the
# aarch64-linux-android rust target and a JDK 17 or newer.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

toolchain_host_cpu=$(uname -m)
case "$(uname -s) $toolchain_host_cpu" in
    "Linux x86_64") toolchain_host=linux-x86_64 ;;
    "Darwin x86_64") toolchain_host=darwin-x86_64 ;;
    "Darwin arm64") toolchain_host=darwin-arm64 ;;
    *) echo "Unsupported toolchain host: $(uname -s) $toolchain_host_cpu" >&2
       exit 1 ;;
esac

ndk=${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-${ANDROID_NDK:-}}}
if [ -z "$ndk" ]; then
    echo 'Set ANDROID_NDK_HOME to an Android NDK (r27 or newer).' >&2
    exit 1
fi
toolchain="$ndk/toolchains/llvm/prebuilt/$toolchain_host/bin"
if [ ! -x "$toolchain/aarch64-linux-android26-clang" ]; then
    echo "The $toolchain_host NDK toolchain with Android API 26 is required." >&2
    exit 1
fi
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$toolchain/aarch64-linux-android26-clang"
export CC_aarch64_linux_android="$toolchain/aarch64-linux-android26-clang"
export CXX_aarch64_linux_android="$toolchain/aarch64-linux-android26-clang++"
export AR_aarch64_linux_android="$toolchain/llvm-ar"
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--sysroot=\"$ndk/toolchains/llvm/prebuilt/$toolchain_host/sysroot\" --target=aarch64-linux-android26"
cargo build --manifest-path "$root/Cargo.toml" --workspace --release --no-default-features --locked --target aarch64-linux-android

out="$root/target/aarch64-linux-android/release"
jni="$root/android/app/src/main/jniLibs/arm64-v8a"
assets="$root/android/app/src/main/assets/modules"
rm -rf "$jni" "$assets"
mkdir -p "$jni" "$assets"

# The APK-side naming: jniLibs mandates the lib*.so pattern while the engine
# expects its CLI names on disk; Engine.kt maps between the two.
for entry in \
    "mz:libmz.so" \
    "ymz-module:libymz_module.so" \
    "youmz-module:libyoumz_module.so" \
    "spotify-module:libspotify_module.so" \
    "local-module:liblocal_module.so" \
    "icecast-module:libicecast_module.so"
do
    binary=${entry%%:*}
    library=${entry#*:}
    if [ ! -f "$out/$binary" ]; then
        echo "Missing Android binary $binary" >&2
        exit 1
    fi
    cp "$out/$binary" "$jni/$library"
done

for module in ymz youmz spotify local icecast; do
    mkdir -p "$assets/$module"
    cp "$root/modules/$module/module.json" "$assets/$module/module.json"
done

cd "$root/android"
./gradlew --no-daemon :app:assembleRelease
cd "$root"

mkdir -p dist
cp android/app/build/outputs/apk/release/app-release.apk dist/musiczero-android-arm64.apk
echo "Built dist/musiczero-android-arm64.apk"
