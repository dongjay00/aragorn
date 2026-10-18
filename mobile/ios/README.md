# Aragorn iOS

에뮬레이션·세이브·오디오는 Rust(`crates/aragorn-mobile`)가 맡고, 이 앱(SwiftUI)은 화면 표시와 입력만 맡는다.
Swift 바인딩은 UniFFI가 만든다. Android 앱(`mobile/android`)과 같은 Rust API를 쓴다.

## 준비

- Xcode 16 이상 (iOS 17 이상을 대상으로 빌드한다)
- `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`
- 시뮬레이터는 Apple Silicon Mac만 지원한다 (Rust 라이브러리를 arm64로만 빌드한다).

## 빌드

처음 한 번은 Xcode를 열기 전에 저장소 루트에서 실행한다. Xcode는 빌드를 시작할 때 폴더 안 파일 목록을 정하므로,
Swift 바인딩이 먼저 있어야 한다.

```sh
cargo xtask ios
```

그다음 `mobile/ios/Aragorn.xcodeproj`를 열고 실행한다. 빌드할 때마다 첫 단계(`build-rust.sh`)가
`cargo xtask ios --platform=<기기 또는 시뮬레이터>`를 불러 Rust 라이브러리와 바인딩을 갱신한다.
실기기에서 실행하려면 Signing & Capabilities에서 개발 팀을 고른다.

앱 버전은 워크스페이스 버전(`Cargo.toml`)과 같다 (`build/Version.xcconfig`).

## UI 테스트

```sh
cd mobile/ios
xcodebuild -project Aragorn.xcodeproj -scheme Aragorn -destination 'platform=iOS Simulator,name=iPhone 17 Pro' test
```

디버그 빌드는 개발용 실행 인자를 받는다: `-rom <경로>`로 ROM을 바로 열고, `-page settings|debugger`로 화면을 고른다.

## 기능 (데스크톱·Android와 같다)

- 스테이트 슬롯 10개, 기본 속도·배속·일시정지, 음소거
- 키보드·게임패드 키 설정 (GameController 프레임워크). 키보드 이름은 `UP`, `X`, `RETURN`처럼 쓰고,
  게임패드는 Android와 같은 이름(`DPAD_UP`, `BUTTON_A`)을 쓴다.
- 디버거: CPU, 메모리, VRAM, I/O
- 업데이트 확인: 정책에 `ios` 패키지가 있을 때만 알리고, 릴리스 페이지로 안내한다.

## 세이브

`Documents/saves/<ROM 파일 이름>`에 데스크톱과 같은 형식(`.sav`, `.ss0`–`.ss9`)으로 저장한다.
파일 앱의 "나의 iPhone › Aragorn › saves"에 보이므로, 데스크톱 세이브를 ROM 파일 이름에 맞춰 넣으면 이어서 할 수 있다.

## 배포

데스크톱·Android와 같은 `v<버전>` 태그로 함께 나간다. 릴리스 CI가 서명하지 않은 `Aragorn-<버전>.ipa`를 만들고,
업데이트 정책에 IPA의 SHA256을 `ios` 채널로 넣는다. 사용자는 AltStore·SideStore로 자기 Apple ID로 서명해 설치한다.
TestFlight·App Store 배포에는 Apple Developer Program(연 $99) 가입이 필요하다.
