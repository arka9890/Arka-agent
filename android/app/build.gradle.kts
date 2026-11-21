import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
}

// Penandatanganan APK.
//
// Kunci bersama (committed) dipakai supaya APK dari CI mana pun bisa
// di-upgrade di tempat tanpa uninstall. Kalau `keystore.properties` tidak ada,
// build tetap jalan dengan debug key bawaan Gradle.
val keystorePropsFile = rootProject.file("keystore.properties")
val keystoreProps = Properties().apply {
    if (keystorePropsFile.exists()) keystorePropsFile.inputStream().use { load(it) }
}
val hasSigning = keystorePropsFile.exists() &&
    keystoreProps.getProperty("storeFile")?.let { rootProject.file(it).exists() } == true

android {
    namespace = "com.arka.agent"
    compileSdk = 35
    ndkVersion = "27.0.12077973"

    defaultConfig {
        applicationId = "com.arka.agent"
        minSdk = 24
        targetSdk = 35
        // PRD: ARM64 default, ARMv7 opsional. Binary Rust dibangun untuk
        // keduanya oleh rust/build-android.sh; ABI yang tidak punya .so tidak
        // dimasukkan supaya APK tidak membengkak.
        ndk {
            abiFilters += listOf("arm64-v8a", "armeabi-v7a")
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
        }
    }

    buildTypes {
        debug {
            if (hasSigning) signingConfig = signingConfigs.getByName("arka")
            ndk { debugSymbolLevel = "none" }
        }
        release {
            isMinifyEnabled = false
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            if (hasSigning) signingConfig = signingConfigs.getByName("arka")
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
