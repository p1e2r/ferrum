// SPDX-License-Identifier: MIT
// Ferrum for Android: a native browser using Android's WebView, with
// Ferrum's look and features. Built by GitHub Actions (see
// .github/workflows/build.yml); the version comes from FERRUM_VERSION, or
// the desktop app's Cargo.toml.
import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

/** "1.2.3" from FERRUM_VERSION or ../../Cargo.toml. */
fun ferrumVersion(): String {
    System.getenv("FERRUM_VERSION")?.takeIf { it.isNotBlank() }?.let { return it }
    val cargo = rootDir.resolve("../Cargo.toml")
    if (cargo.exists()) {
        Regex("""^version\s*=\s*"([^"]+)"""", RegexOption.MULTILINE).find(cargo.readText())?.let { return it.groupValues[1] }
    }
    return "0.0.0"
}

/** 1.2.3 → 10203 (each part up to 99). */
fun versionCodeOf(name: String): Int {
    val p = name.split('.', '-').mapNotNull { it.toIntOrNull() } + listOf(0, 0, 0)
    return p[0] * 10000 + p[1].coerceAtMost(99) * 100 + p[2].coerceAtMost(99)
}

// Release signing, from environment variables (GitHub Actions secrets) or keystore.properties.
val signing = Properties().apply {
    val f = rootDir.resolve("keystore.properties")
    if (f.exists()) f.inputStream().use { load(it) }
    System.getenv("ANDROID_KEYSTORE_FILE")?.let { setProperty("storeFile", it) }
    System.getenv("ANDROID_KEYSTORE_PASSWORD")?.let { setProperty("storePassword", it) }
    System.getenv("ANDROID_KEY_ALIAS")?.let { setProperty("keyAlias", it) }
    System.getenv("ANDROID_KEY_PASSWORD")?.let { setProperty("keyPassword", it) }
}
val canSign = listOf("storeFile", "storePassword", "keyAlias", "keyPassword").all { !signing.getProperty(it).isNullOrBlank() }

android {
    namespace = "io.github.p1e2r.ferrum"
    compileSdk = 35

    defaultConfig {
        applicationId = "io.github.p1e2r.ferrum"
        minSdk = 26
        targetSdk = 35
        val v = ferrumVersion()
        versionName = v
        versionCode = versionCodeOf(v)
    }

    signingConfigs {
        if (canSign) {
            create("release") {
                storeFile = file(signing.getProperty("storeFile"))
                storePassword = signing.getProperty("storePassword")
                keyAlias = signing.getProperty("keyAlias")
                keyPassword = signing.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            if (canSign) signingConfig = signingConfigs.getByName("release")
        }
        debug {
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-debug"
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    buildFeatures {
        compose = true
        buildConfig = true
    }
    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2024.12.01")
    implementation(composeBom)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.webkit:webkit:1.12.1")
    testImplementation("junit:junit:4.13.2")
}
