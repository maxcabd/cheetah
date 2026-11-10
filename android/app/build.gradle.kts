import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// Supply MAPS_API_KEY in android/local.properties for a native Google map.
// Without it the map leaf renders a development placeholder.
val mapsApiKey: String = run {
    val properties = Properties()
    val file = rootProject.file("local.properties")
    if (file.exists()) {
        file.inputStream().use { properties.load(it) }
    }
    (
        properties.getProperty("MAPS_API_KEY")
            ?: System.getenv("MAPS_API_KEY")
            ?: ""
    ).trim()
}

android {
    namespace = "com.example.rustui"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.example.rustui"
        minSdk = 24
        targetSdk = 34
        versionCode = 1
        versionName = "0.1"

        manifestPlaceholders["MAPS_API_KEY"] = mapsApiKey
        buildConfigField("String", "MAPS_API_KEY", "\"$mapsApiKey\"")

        ndk {
            abiFilters += listOf("x86_64", "arm64-v8a")
        }
    }

    buildFeatures {
        buildConfig = true
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            // Local benchmarking only. Use a real keystore for distribution.
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    implementation("com.google.android.gms:play-services-maps:20.1.0")
    implementation("com.google.android.material:material:1.12.0")
    // Keyless fallback used by the Nightshade lab when MAPS_API_KEY is absent.
    implementation("org.maplibre.gl:android-sdk:11.13.5")
}
