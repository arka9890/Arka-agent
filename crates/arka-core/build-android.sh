#!/bin/sh
# Bangun `libarka_core.so` untuk Android lalu salin ke jniLibs.
#
# Sejak M1, kode JNI ada di crate `arka-android` (bukan `arka-core`): crate
# core bebas JNI supaya bisa dipakai versi Windows. Nama file .so tetap
# `libarka_core.so` karena `Arka.kt` memanggil `System.loadLibrary("arka_core")`.
#
# Kenapa script ini ada padahal ada cargo? Build Android butuh tiga hal yang
# tidak ada di `cargo build` biasa: target NDK, linker clang, dan penyalinan
# ke folder yang dibaca Gradle. Script ini juga dipakai CI.
#
# Pemakaian:
#   ./build-android.sh            # arm64-v8a (default)
#   ./build-android.sh all        # semua ABI yang didukung PRD
set -eu

TARGET_ARM64="aarch64-linux-android"
TARGET_ARMv7="armv7-linux-androideabi"
# Harus sama dengan minSdk di android/app/build.gradle.kts (24). Kalau lebih
# tinggi, APK terpasang di Android lama tetapi gagal memuat libarka_core.so.
API=24

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
# Repo ini Cargo workspace: crate ini ada di crates/arka-core, jadi root
# workspace dua level di atas, dan folder Android satu level di atas crate.
WORKSPACE_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
ANDROID_DIR="$WORKSPACE_ROOT/android"
JNI_LIBS="$ANDROID_DIR/app/src/main/jniLibs"
# Semua artefak build masuk ke target/ root workspace (di-ignore git).
TARGET_DIR="$WORKSPACE_ROOT/target"

: "${ANDROID_NDK_HOME:=${ANDROID_NDK_ROOT:-/opt/android-sdk/ndk/27.0.12077973}}"
NDK_BIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64"
if [ ! -d "$NDK_BIN" ]; then
    echo "NDK tidak ditemukan di $ANDROID_NDK_HOME" >&2
    echo "Set ANDROID_NDK_HOME ke folder NDK (mis. \$ANDROID_HOME/ndk/27.0.12077973)." >&2
    exit 1
fi

# Mesin build ini bisa arm64 maupun x86_64; pakai clang NDK kalau bisa
# dijalankan, kalau tidak jatuh ke clang sistem + sysroot NDK (dipakai CI arm64).
if "$NDK_BIN/bin/clang" --version >/dev/null 2>&1; then
    CC_ARM64="$NDK_BIN/bin/aarch64-linux-android$API-clang"
    CC_ARMV7="$NDK_BIN/bin/armv7a-linux-androideabi$API-clang"
    AR_BIN="$NDK_BIN/bin/llvm-ar"
    EXTRA_FLAGS=""
else
    echo "clang NDK tidak bisa dijalankan; memakai clang sistem + sysroot NDK." >&2
    SYSROOT="$NDK_BIN/sysroot"
    RESOURCE_DIR="$NDK_BIN/lib/clang/18"
    # shortcut: path clang sistem diasumsikan 'clang'; upgrade kalau build
    # memakai image dengan versi clang berbeda.
    mkdir -p "$TARGET_DIR/cc"
    cat > "$TARGET_DIR/cc/aarch64.sh" <<EOF
#!/bin/sh
exec clang --target=aarch64-linux-android$API --sysroot=$SYSROOT \\
  -resource-dir=$RESOURCE_DIR --rtlib=compiler-rt -fuse-ld=lld "\$@"
EOF
    cat > "$TARGET_DIR/cc/armv7.sh" <<EOF
#!/bin/sh
exec clang --target=armv7a-linux-androideabi$API --sysroot=$SYSROOT \\
  -resource-dir=$RESOURCE_DIR --rtlib=compiler-rt -fuse-ld=lld "\$@"
EOF
    chmod +x "$TARGET_DIR/cc/aarch64.sh" "$TARGET_DIR/cc/armv7.sh"
    CC_ARM64="$TARGET_DIR/cc/aarch64.sh"
    CC_ARMV7="$TARGET_DIR/cc/armv7.sh"
    AR_BIN=$(command -v llvm-ar || command -v llvm-ar-18)
    EXTRA_FLAGS=""
fi

build() {
    target=$1
    abi=$2
    cc=$3
    ar_name=$(echo "$target" | tr '-' '_')
    echo "==> $abi ($target)"
    env \
        "CC_${ar_name}=$cc" \
        "AR_${ar_name}=$AR_BIN" \
        "CARGO_TARGET_$(echo "$target" | tr 'a-z-' 'A-Z_')_LINKER=$cc" \
        CARGO_TARGET_DIR="$TARGET_DIR" \
        cargo build --release --target "$target" --package arka-android --manifest-path "$WORKSPACE_ROOT/Cargo.toml"
    mkdir -p "$JNI_LIBS/$abi"
    cp "$TARGET_DIR/$target/release/libarka_android.so" "$JNI_LIBS/$abi/libarka_core.so"
    echo "    -> $JNI_LIBS/$abi/libarka_core.so"
}

case "${1:-arm64}" in
    all)
        build "$TARGET_ARM64" arm64-v8a "$CC_ARM64"
        build "$TARGET_ARMv7" armeabi-v7a "$CC_ARMV7"
        ;;
    arm64|arm64-v8a) build "$TARGET_ARM64" arm64-v8a "$CC_ARM64" ;;
    *)
        echo "ABI tidak dikenal: $1 (pakai arm64 atau all)" >&2
        exit 1
        ;;
esac

echo "Selesai."
