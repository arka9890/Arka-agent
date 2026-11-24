import java.io.File
import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
}

// Penandatanganan APK.
//
// Urutan prioritas:
//  1. `android/keystore.properties` + keystore milik sendiri (RAHASIA,
//     sengaja tidak di-commit) — dipakai kalau ada.
//  2. `android/app/ci-debug.keystore` — kunci debug bersama yang sengaja
//     di-commit dengan password publik. Bukan kredensial: hanya untuk APK
//     sideload/CI supaya tiap build punya signature SAMA dan pengguna bisa
//     meng-upgrade aplikasi tanpa uninstall.
//  3. Tanpa keduanya, Gradle memakai debug key bawaan yang berubah-ubah.
val keystorePropsFile = rootProject.file("keystore.properties")
val keystoreProps = Properties().apply {
    if (keystorePropsFile.exists()) keystorePropsFile.inputStream().use { load(it) }
}
val hasSigning = keystorePropsFile.exists() &&
    keystoreProps.getProperty("storeFile")?.let { rootProject.file(it).exists() } == true
val ciKeystore = rootProject.file("app/ci-debug.keystore")
val hasCiSigning = !hasSigning && ciKeystore.exists()
val ciStorePassword = "arka1234"
val ciKeyAlias = "arkaci"
val ciKeyPassword = "arka1234"

android {
    namespace = "com.arka.agent"
    compileSdk = 35
    ndkVersion = "27.0.12077973"

    defaultConfig {
        applicationId = "com.arka.agent"
        minSdk = 24
        targetSdk = 35
        // PRD: ARM64 default, ARMv7 opsional. Binary Rust dibangun untuk
        // keduanya oleh crates/arka-core/build-android.sh. Saat ini yang tersedia hanya
        // arm64-v8a; kalau armeabi-v7a ikut didaftarkan tanpa
        // libarka_core.so-nya, APK tetap terpasang di perangkat 32-bit lalu
        // crash saat memuat library. Jadi daftar ABI harus cocok dengan isi
        // jniLibs (jalankan `crates/arka-core/build-android.sh all` untuk menambah v7).
        ndk {
            abiFilters += listOf("arm64-v8a")
        }
        versionCode = (System.currentTimeMillis() / 60000).toInt()
        versionName = "1.0.0"
    }

    signingConfigs {
        if (hasSigning) {
            create("arka") {
                storeFile = rootProject.file(keystoreProps.getProperty("storeFile"))
                storePassword = keystoreProps.getProperty("storePassword")
                keyAlias = keystoreProps.getProperty("keyAlias")
                keyPassword = keystoreProps.getProperty("keyPassword")
            }
        } else if (hasCiSigning) {
            create("ci") {
                storeFile = ciKeystore
                storePassword = ciStorePassword
                keyAlias = ciKeyAlias
                keyPassword = ciKeyPassword
            }
        }
    }

    val signingName = if (hasSigning) "arka" else if (hasCiSigning) "ci" else null
    buildTypes {
        debug {
            if (signingName != null) signingConfig = signingConfigs.getByName(signingName)
            ndk { debugSymbolLevel = "none" }
        }
        release {
            isMinifyEnabled = false
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            if (signingName != null) signingConfig = signingConfigs.getByName(signingName)
            ndk { debugSymbolLevel = "none" }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }

    kotlinOptions {
        jvmTarget = "1.8"
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    // PRoot dieksekusi dari nativeLibraryDir, jadi .so harus benar-benar
    // diekstrak ke disk saat instalasi APK (bukan dibaca langsung dari APK).
    packaging {
        jniLibs {
            useLegacyPackaging = true
        }
    }
}

// Peringatan dini: APK tanpa binary proot tetap bisa dibangun, tetapi tab
// Terminal tidak akan bisa menjalankan perintah sama sekali. Lebih baik
// terlihat di log build daripada baru ketahuan di perangkat.
tasks.named("preBuild") {
    doFirst {
        val abiDir = file("src/main/jniLibs/arm64-v8a")
        if (!File(abiDir, "libproot.so").exists()) {
            logger.warn(
                "[arka] libproot.so tidak ada di ${abiDir.path}. " +
                    "Terminal tidak akan berfungsi di APK ini " +
                    "(lihat android/app/src/main/jniLibs/README.md).",
            )
        }
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.graphics)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons.extended)
    implementation(libs.kotlinx.serialization.json)
    debugImplementation(libs.compose.ui.tooling)
    testImplementation("junit:junit:4.13.2")
}
