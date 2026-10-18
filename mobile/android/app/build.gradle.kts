plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// 앱 버전은 데스크톱과 같은 워크스페이스 버전(Cargo.toml의 [workspace.package] version)이다.
// 업데이트 정책도 이 버전으로 판단한다.
val workspaceVersion: String = rootDir.resolve("../../Cargo.toml").readText()
    .substringAfter("[workspace.package]")
    .let { Regex("""version\s*=\s*"([^"]+)"""").find(it)!!.groupValues[1] }

/** 1.2.3 → 10203. 마이너·패치는 99까지 쓴다. */
fun versionCodeOf(version: String): Int {
    val (major, minor, patch) = version.substringBefore('-').split('.').map(String::toInt)
    return major * 10000 + minor * 100 + patch
}

// 업데이트를 확인할 GitHub 저장소(owner/repo). 데스크톱처럼 릴리스 CI가 ARAGORN_REPO로 넣는다.
// 없으면 업데이트를 확인하지 않는다. 로컬에서는 -Paragorn.repo=owner/repo로 줄 수 있다.
val githubRepo: String = providers.environmentVariable("ARAGORN_REPO")
    .orElse(providers.gradleProperty("aragorn.repo"))
    .getOrElse("")

android {
    namespace = "io.github.dongjay00.aragorn"
    compileSdk = 37
    // cargo-ndk가 이 NDK로 Rust 라이브러리를 빌드한다.
    ndkVersion = "29.0.14206865"

    defaultConfig {
        applicationId = "io.github.dongjay00.aragorn"
        // cpal의 AAudio 출력이 API 26부터 있다. xtask/src/android.rs의 MIN_SDK와 맞춘다.
        minSdk = 26
        targetSdk = 36
        versionCode = versionCodeOf(workspaceVersion)
        versionName = workspaceVersion
        buildConfigField("String", "GITHUB_REPO", "\"$githubRepo\"")
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            // 아직 릴리스 키가 없어 디버그 키로 서명한다. 배포 전에 바꾼다.
            signingConfig = signingConfigs.getByName("debug")
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

    sourceSets {
        getByName("main") {
            // `cargo xtask android`가 만드는 .so와 Kotlin 바인딩
            jniLibs.directories.add(layout.buildDirectory.dir("rustJniLibs").get().asFile.path)
            kotlin.directories.add(
                layout.buildDirectory.dir("generated/uniffi/kotlin").get().asFile.path
            )
        }
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    // UniFFI가 만든 Kotlin 바인딩이 JNA로 Rust 라이브러리를 부른다.
    implementation("${libs.jna.get()}@aar")
}

val ndkDirectory = androidComponents.sdkComponents.ndkDirectory

// Rust 라이브러리와 Kotlin 바인딩을 만든다. 저장소 루트의 `cargo xtask android`를 부른다.
// cargo가 바뀐 것만 다시 빌드하므로 매번 실행한다.
val buildRust = tasks.register<Exec>("buildRust") {
    val repoRoot = rootDir.resolve("../..")
    val home = System.getProperty("user.home")
    // Android Studio를 Dock에서 열면 PATH에 ~/.cargo/bin이 없다.
    val cargo = File(home, ".cargo/bin/cargo").takeIf { it.exists() }?.path ?: "cargo"
    val release = gradle.startParameter.taskNames.any { it.contains("Release", ignoreCase = true) }
    workingDir = repoRoot
    commandLine(listOfNotNull(cargo, "xtask", "android", "--release".takeIf { release }))
    environment("PATH", "${File(home, ".cargo/bin")}${File.pathSeparator}${System.getenv("PATH")}")
    doFirst {
        environment("ANDROID_NDK_HOME", ndkDirectory.get().asFile.path)
    }
}

tasks.named("preBuild") {
    dependsOn(buildRust)
}
