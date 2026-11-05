import com.diffplug.spotless.LineEnding
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
  alias(libs.plugins.android.application)
  alias(libs.plugins.kotlin.android)
  alias(libs.plugins.kotlin.compose)
  alias(libs.plugins.spotless)
}

android {
  namespace = "dev.phonealvr.client"
  compileSdk = 35

  defaultConfig {
    applicationId = "dev.phonealvr.client"
    minSdk = 26
    targetSdk = 35
    versionCode = System.getenv("PHONEALVR_VERSION_CODE")?.toIntOrNull() ?: 1
    versionName = System.getenv("PHONEALVR_VERSION_NAME") ?: "0.1.0"

    testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    vectorDrawables.useSupportLibrary = true

    // The normal app build consumes prebuilt native artifacts. Build them
    // with scripts/build-core.sh (or `make core`) for the ABIs you need.
    ndk { abiFilters += listOf("arm64-v8a") }

    buildConfigField("String", "ALVR_VERSION", "\"v20.14.1\"")
  }

  buildTypes {
    debug {
      versionNameSuffix = "-debug"
      ndk { abiFilters += listOf("x86_64") }
    }
    release {
      isMinifyEnabled = false
      proguardFiles(
        getDefaultProguardFile("proguard-android-optimize.txt"),
        "proguard-rules.pro",
      )
      // Release signing is supplied only by CI secrets. There is no
      // keystore or password in the repository.
      val keystorePath = System.getenv("PHONEALVR_KEYSTORE")
      if (keystorePath != null) {
        signingConfig =
          signingConfigs.create("release") {
            storeFile = file(keystorePath)
            storePassword = System.getenv("PHONEALVR_KEYSTORE_PASSWORD")
            keyAlias = System.getenv("PHONEALVR_KEY_ALIAS")
            keyPassword = System.getenv("PHONEALVR_KEY_PASSWORD")
          }
      }
    }
  }

  compileOptions {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
  }

  buildFeatures {
    compose = true
    buildConfig = true
  }

  testOptions { unitTests { isReturnDefaultValues = true } }

  packaging {
    resources {
      excludes +=
        setOf(
          "/META-INF/{AL2.0,LGPL2.1}",
          "META-INF/DEPENDENCIES",
          "META-INF/LICENSE*",
          "META-INF/NOTICE*",
        )
    }
  }

  lint {
    abortOnError = true
    checkReleaseBuilds = true
  }
}

kotlin { compilerOptions { jvmTarget.set(JvmTarget.JVM_17) } }

spotless {
  lineEndings = LineEnding.GIT_ATTRIBUTES
  kotlin {
    target("src/**/*.kt")
    ktfmt().googleStyle()
  }
  kotlinGradle {
    target("*.gradle.kts")
    ktfmt().googleStyle()
  }
}

dependencies {
  implementation(libs.androidx.core.ktx)
  implementation(libs.androidx.lifecycle.runtime.ktx)
  implementation(libs.androidx.lifecycle.runtime.compose)
  implementation(libs.androidx.lifecycle.viewmodel.compose)
  implementation(libs.androidx.activity.compose)
  implementation(libs.kotlinx.coroutines.android)
  implementation(libs.zxing.embedded)

  implementation(platform(libs.androidx.compose.bom))
  implementation(libs.androidx.compose.ui)
  implementation(libs.androidx.compose.ui.tooling.preview)
  implementation(libs.androidx.compose.material3)
  implementation(libs.androidx.compose.material.icons)

  debugImplementation(libs.androidx.compose.ui.tooling)
  debugImplementation(libs.androidx.compose.ui.test.manifest)

  testImplementation(libs.junit)
  testImplementation(libs.json)
  androidTestImplementation(libs.androidx.junit)
  androidTestImplementation(libs.androidx.test.runner)
  androidTestImplementation(libs.androidx.espresso.core)
  androidTestImplementation(platform(libs.androidx.compose.bom))
  androidTestImplementation(libs.androidx.compose.ui.test.junit4)
}
