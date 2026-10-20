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
- 되감기 버튼·키(누르는 동안, 최근 약 20초), 치트(게임 지니·게임샤크, 세이브 옆 `.cht`)
- 화면 필터(흑백 팔레트, 컬러 색 보정, LCD 격자), 가상 패드 크기·불투명도·높이
- 이어하기(백그라운드로 가거나 게임을 바꿀 때 `.ssr`을 남긴다), 메뉴의 스크린샷(`Pictures/Aragorn`), 처음부터 다시 시작
- 게임패드 왼쪽 아날로그 스틱도 방향키로 쓴다
- 슈퍼 게임보이 색·테두리(테두리를 그리면 화면이 256×224가 된다), 가상 패드 위치 편집(묶음마다 끌어 옮기고 세로·가로를 따로 기억한다)
- 최근 게임: 고른 ROM을 앱 폴더 `roms/`에 복사해 두고 툴바 ROM 메뉴와 빈 화면에서 다시 연다
- 키보드·게임패드 키 설정. 게임패드의 방향 패드(hat)와 트리거 축도 버튼처럼 지정할 수 있다
- 디버거: CPU(레지스터, 디스어셈블리, 단계 실행, 브레이크포인트), 메모리, VRAM(타일, 배경 맵, OAM, 팔레트), I/O(채널 음소거)
- 업데이트 확인: 데스크톱과 같은 서명된 정책(`update-policy.json`)을 검증한다. 정책에 `android` 패키지가 있을 때만
  새 버전을 알리고, 앱이 스스로 설치할 수는 없어 릴리스 페이지로 안내한다. 강제 업데이트면 게임을 멈춘다.
- 앱 버전은 워크스페이스 버전(`Cargo.toml`)과 같다.

설정은 앱 전용 폴더의 `config.json`에 둔다. 형식은 데스크톱 설정과 같고, 키 이름만 Android 이름(`DPAD_UP`, `BUTTON_A`)이다.

## 세이브

ROM은 문서 선택기로 고르고, 세이브는 앱 전용 폴더 `files/saves/<ROM 파일 이름>`에 데스크톱과 같은 형식으로 둔다
(`.sav`, `.ss0`–`.ss9`). 앱이 백그라운드로 가면 일시정지하고 게임 세이브를 저장한다.

세이브 폴더는 시스템 파일 앱과 파일 선택기에 **"Aragorn – 게임 세이브"**로 보인다(`SavesProvider`).
- 꺼내기: 파일 앱에서 세이브를 골라 "복사"로 다른 폴더(다운로드, 클라우드 드라이브)에 둔다.
- 넣기: 데스크톱 세이브(`<ROM 이름>.sav` 등)를 Aragorn으로 복사한다. 이름이 ROM 파일 이름과 맞아야 한다.
  같은 이름이 있으면 덮어쓰기 전에 `<이름>.bak`으로 남긴다.
- 실행 중인 게임의 세이브를 바꾸면, 바꾸기 전에 그 게임을 저장하고 닫는다. 그러지 않으면 게임이 나중에
  자기 진행으로 덮어쓴다. ROM을 다시 열면 바뀐 세이브로 시작한다.

## 릴리스

데스크톱과 같은 `v<버전>` 태그 하나로 함께 나간다. 릴리스 CI(`.github/workflows/release.yml`)가 다음을 한다.

1. 서명된 릴리스 APK를 `Aragorn-<버전>.apk`로 빌드한다 (arm64-v8a, x86_64).
2. 업데이트 정책에 APK의 SHA256을 `android` 채널로 넣는다. Android 앱은 이 채널이 있을 때만 새 버전을 알린다.

릴리스 서명 키는 GitHub 시크릿 네 개로 넣는다.

| 시크릿 | 내용 |
|---|---|
| `ARAGORN_ANDROID_KEYSTORE_BASE64` | 키스토어(`.jks`) 파일을 base64로 바꾼 것 |
| `ARAGORN_ANDROID_KEYSTORE_PASSWORD` | 키스토어 비밀번호 |
| `ARAGORN_ANDROID_KEY_ALIAS` | 키 별칭 |
| `ARAGORN_ANDROID_KEY_PASSWORD` | 키 비밀번호 |

**키스토어를 잃어버리면 기존 사용자에게 업데이트를 줄 수 없다.** 같은 키로 서명한 APK만 기존 앱 위에 설치되기 때문이다.
키스토어와 비밀번호는 저장소 밖 안전한 곳에 백업해 둔다. 키가 없으면 릴리스 CI가 멈춘다.

로컬에서 릴리스 APK를 만들 때도 같은 환경 변수(`ARAGORN_ANDROID_KEYSTORE`에는 파일 경로)를 쓴다. 없으면 디버그 키로 서명한다.
