plugins {
    id("com.android.application")
}

android {
    namespace = "org.aaadaw.app"
    compileSdk = 35

    defaultConfig {
        applicationId = "org.aaadaw.app"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    sourceSets["main"].jniLibs.srcDir("src/main/jniLibs")
}
