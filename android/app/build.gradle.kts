plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "com.musiczero.app"
    compileSdk = 37

    defaultConfig {
        applicationId = "com.musiczero.app"
        minSdk = 26
        // Termux-style exec allowance: Android 10 blocks exec() from app data
        // for apps targeting API 29+, and the engine binaries must run from
        // the private files directory.
        targetSdk = 28
        versionCode = 4
        versionName = "0.4.0"
    }

    buildFeatures {
        compose = true
    }

    buildTypes {
        release {
            // Release APKs are signed with the debug key by default so the
            // archive installs without creating a keystore. R8 stays off: the
            // media3 session bridges need reflection-free bytecode.
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    lint {
        // targetSdk stays at 28 deliberately (Android 10 exec() policy, see
        // defaultConfig); the app is sideloaded, never published on Play.
        disable += "ExpiredTargetSdkVersion"
    }
    packaging {
        jniLibs {
            useLegacyPackaging = true
        }
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.19.1")
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.11.0")

    implementation("androidx.compose.ui:ui:1.10.6")
    implementation("androidx.compose.ui:ui-graphics:1.10.6")
    implementation("androidx.compose.foundation:foundation:1.10.6")
    implementation("androidx.compose.animation:animation:1.10.6")
    implementation("androidx.compose.material3:material3:1.4.0")

    implementation("androidx.media3:media3-session:1.11.1")
}
