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

로컬 빌드에서 업데이트 확인을 시험하려면 저장소를 넘긴다 (릴리스 CI는 `ARAGORN_REPO` 환경 변수로 넣는다):

```sh
./gradlew installDebug -Paragorn.repo=dongjay00/aragorn
```

## 기능 (데스크톱과 같다)

- 스테이트 슬롯 10개 (썸네일, 저장 시각). 키보드 F1–F10 저장, Shift+F1–F10 불러오기
- 기본 속도(1·2·4배), 배속 버튼·키(누르는 동안 2·4배·무제한), 일시정지, 음소거
- 키보드·게임패드 키 설정. 게임패드의 방향 패드(hat)와 트리거 축도 버튼처럼 지정할 수 있다
- 디버거: CPU(레지스터, 디스어셈블리, 단계 실행, 브레이크포인트), 메모리, VRAM(타일, 배경 맵, OAM, 팔레트), I/O(채널 음소거)
- 업데이트 확인: 데스크톱과 같은 서명된 정책(`update-policy.json`)을 검증한다. 정책에 `android` 패키지가 있을 때만
  새 버전을 알리고, 앱이 스스로 설치할 수는 없어 릴리스 페이지로 안내한다. 강제 업데이트면 게임을 멈춘다.
- 앱 버전은 워크스페이스 버전(`Cargo.toml`)과 같다.

설정은 앱 전용 폴더의 `config.json`에 둔다. 형식은 데스크톱 설정과 같고, 키 이름만 Android 이름(`DPAD_UP`, `BUTTON_A`)이다.

## 세이브

ROM은 문서 선택기로 고르고, 세이브는 앱 전용 폴더 `files/saves/<ROM 파일 이름>`에 데스크톱과 같은 형식으로 둔다
(`.sav`, `.ss0`–`.ss9`). 앱이 백그라운드로 가면 일시정지하고 게임 세이브를 저장한다.
