# Aragorn Android

에뮬레이션·세이브·오디오는 Rust(`crates/aragorn-mobile`)가 맡고, 이 앱(Kotlin + Compose)은 화면 표시와 입력만 맡는다.
Kotlin 바인딩은 UniFFI가 만든다.

## 준비

```sh
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk
```

Android SDK에서 NDK `29.0.14206865`와 SDK Platform 37을 설치한다 (Android Studio SDK Manager 또는 `sdkmanager`).

## 빌드

Android Studio에서 `mobile/android`를 열고 실행하면 된다. Gradle의 `preBuild`가 저장소 루트에서
`cargo xtask android`를 먼저 불러 Rust 라이브러리(.so)와 Kotlin 바인딩을 `app/build/` 아래에 만든다.

명령줄:

```sh
cd mobile/android
./gradlew installDebug          # 연결된 기기·에뮬레이터에 설치
```

Rust 쪽만 다시 만들 때: `cargo xtask android [--release] [--abi=arm64-v8a]`

## 세이브

ROM은 문서 선택기로 고르고, 세이브는 앱 전용 폴더 `files/saves/<ROM 파일 이름>`에 데스크톱과 같은 형식으로 둔다
(`.sav`, `.ss0`–`.ss9`). 앱이 백그라운드로 가면 일시정지하고 게임 세이브를 저장한다.
