# Aragorn — 게임보이/게임보이 컬러 에뮬레이터 설계

- 작성일: 2026-10-07
- 상태: 리뷰 대기

## 1. 목적과 범위

### 1.1 목적
학습(하드웨어를 이해하며 직접 구현)과 실사용(포켓몬 시리즈를 일상적으로 플레이)을 모두 만족하는 데스크톱 게임보이(DMG) / 게임보이 컬러(CGB) 에뮬레이터를 Rust로 만든다.

### 1.2 대상 게임
| 게임 | 기기 | MBC | 추가 요구 |
|---|---|---|---|
| 포켓몬 레드/블루/그린 | DMG | MBC1 (그린: MBC3) | 배터리 세이브 |
| 포켓몬 피카츄(옐로) | DMG (CGB 컬러 지원) | MBC5 | 피카츄 음성(CH3) |
| 포켓몬 금/은 | DMG + CGB 겸용 | MBC3 + RTC | RTC |
| 포켓몬 크리스탈 | CGB 전용 | MBC3 + RTC | CGB 기능 전체 |

### 1.3 플랫폼
Windows, macOS, Linux (크로스플랫폼). 개발 환경은 WSL2.

### 1.4 1차 목표에 포함되는 편의 기능
- 세이브 스테이트
- 배속 / 일시정지
- 키 설정 + 게임패드
- 디버거 UI
- 업데이트: 강제 업데이트, 소프트 업데이트, 자동 업데이트

### 1.5 범위 밖 (Non-goals)
- 게임보이 어드밴스
- 링크 케이블(통신 교환/대전), 적외선
- 치트, 리와인드
- 사이클 단위 완벽 정확도 (PPU 픽셀 FIFO 등)
- 코드 서명 (초기에는 미서명, 이후 검토)

### 1.6 성공 기준
1. 포켓몬 크리스탈을 처음부터 진행할 수 있고, 배터리 세이브와 RTC가 재실행 후에도 유지된다.
2. 소리 끊김 없이 60fps(59.73fps)를 유지한다.
3. 다음 테스트 ROM 통과가 `cargo test`로 자동 검증된다: Blargg `cpu_instrs`, `instr_timing`, `mem_timing`, `dmg_sound`, `cgb_sound`, `dmg-acid2`, `cgb-acid2`, 선별된 mooneye 테스트.
4. 태그 푸시만으로 3개 OS 릴리스가 생성되고, 설치된 앱이 강제/소프트/자동 업데이트를 수행한다.

## 2. 개발 원칙

| 원칙 | 적용 방식 |
|---|---|
| TDD | 모든 코어 모듈과 앱 로직은 테스트를 먼저 작성한다. 테스트 ROM은 인수 테스트로 쓴다. |
| SDD | 본 설계 문서 → 구현 계획 → 구현 순서를 따른다. 하드웨어 동작의 기준 명세는 Pan Docs다. |
| 클린 아키텍처 | 3계층 크레이트, 의존성은 바깥에서 안쪽으로만 흐른다 (`desktop → app → core`). |
| SOLID | SRP/ISP는 전체에 적용한다. DIP/OCP는 앱 경계(포트 trait)에 적용한다. 에뮬레이션 코어 내부의 핫 패스에는 trait 객체(동적 디스패치)를 쓰지 않고 `enum`과 구체 타입을 쓴다. |
| DDD (경량) | 유비쿼터스 언어: 코드의 이름은 Pan Docs 용어를 따른다 (`ly`, `stat`, `lcdc`, `Mbc3`, `rtc_latch`). 바운디드 컨텍스트: Emulation, Session, Distribution, Configuration. 애그리거트, 도메인 이벤트 같은 전술 패턴은 쓰지 않는다. |

## 3. 아키텍처

### 3.1 워크스페이스 구성
```
aragorn/
├── Cargo.toml                 # workspace, 버전 단일 출처
├── crates/
│   ├── gb-core/               # 도메인: 하드웨어 에뮬레이션 (UI/OS/네트워크 의존 없음)
│   ├── aragorn-app/           # 유스케이스 + 포트 trait
│   └── aragorn-desktop/       # 어댑터 + 조립(main)
├── release/policy.toml        # 업데이트 정책 원본 (minimum_supported 등)
├── scripts/fetch-test-roms.sh
├── tests/roms/                # 테스트 ROM (gitignore, 스크립트로 내려받음)
└── docs/
```

### 3.2 계층별 책임
| 크레이트 | 바운디드 컨텍스트 | 책임 | 주요 의존성 |
|---|---|---|---|
| `gb-core` | Emulation | CPU, 버스, 카트리지(MBC/RTC), PPU, APU, 타이머, 조이패드, 인터럽트, DMA, 디스어셈블러, 상태 직렬화 | `serde` (직렬화만) |
| `aragorn-app` | Session, Distribution, Configuration | 에뮬레이션 세션(ROM 로드, 배터리 세이브, 스테이트 슬롯, 속도 제어 계산), 업데이트 정책 판단, 설정 모델, 포트 trait 정의 | `gb-core`, `semver`, `serde` |
| `aragorn-desktop` | — | eframe UI, cpal 오디오, gilrs 게임패드, 파일시스템 저장소, GitHub 정책 소스, Velopack 업데이터, 의존성 조립 | `eframe`, `cpal`, `gilrs`, `rfd`, `directories`, `toml`, HTTP 클라이언트, `ed25519-dalek`, `velopack` |

### 3.3 포트 (aragorn-app에 정의)
```rust
trait AudioSink      { fn push(&mut self, samples: &[f32]); fn fill_ratio(&self) -> f32; fn sample_rate(&self) -> u32; }
trait SaveStore      { fn load_battery(&self, rom_id: &RomId) -> io::Result<Option<Vec<u8>>>;
                       fn save_battery(&self, rom_id: &RomId, data: &[u8]) -> io::Result<()>;   // .bak 백업 포함
                       fn load_state(&self, rom_id: &RomId, slot: u8) -> io::Result<Option<Vec<u8>>>;
                       fn save_state(&self, rom_id: &RomId, slot: u8, data: &[u8], thumb: &[u32]) -> io::Result<()>; }
trait Clock          { fn now_unix(&self) -> u64; }
trait ConfigStore    { fn load(&self) -> Config; fn save(&self, c: &Config) -> io::Result<()>; }
trait UpdateSource   { fn fetch_policy(&self) -> Result<UpdatePolicy, UpdateError>; }   // 서명 검증 포함
trait Updater        { fn download(&self, v: &Version) -> Result<(), UpdateError>;
                       fn apply_and_restart(&self) -> Result<(), UpdateError>; }
```
테스트에서는 모든 포트를 가짜 구현으로 대체한다.

## 4. 코어 (gb-core)

### 4.1 공개 API
```rust
pub struct GameBoy { /* cpu, bus */ }

impl GameBoy {
    pub fn new(rom: Vec<u8>, model: Model /* Auto | Dmg | Cgb */) -> Result<Self, CartError>;
    pub fn run_frame(&mut self);
    pub fn framebuffer(&self) -> &[u32];                 // 160x144 RGBA
    pub fn set_sample_rate(&mut self, hz: f64);         // 프론트엔드가 보정된 비율을 매 프레임 전달 가능
    pub fn drain_audio(&mut self, out: &mut Vec<f32>);   // 스테레오 interleaved
    pub fn set_button(&mut self, b: Button, pressed: bool);

    pub fn battery_ram(&self) -> Option<Vec<u8>>;        // SRAM (+ RTC 48바이트)
    pub fn load_battery_ram(&mut self, data: &[u8], now_unix: u64);
    pub fn battery_dirty(&mut self) -> bool;             // 읽으면 초기화
    pub fn save_state(&self) -> Vec<u8>;
    pub fn load_state(&mut self, data: &[u8]) -> Result<(), StateError>;

    pub fn debug(&self) -> DebugView<'_>;
    pub fn set_breakpoints(&mut self, pcs: &[u16]);
}
```

### 4.2 원칙
- 코어는 실제 시간을 모른다. 속도 조절은 프론트엔드가 담당한다.
- 실제 시간이 필요한 RTC 오프라인 경과 보정은 `load_battery_ram`의 `now_unix` 인자로 주입한다.
- 게임 데이터 때문에 패닉하지 않는다. 정의되지 않은 옵코드(0xD3, 0xDB, 0xDD, 0xE3, 0xE4, 0xEB, 0xEC, 0xED, 0xF4, 0xFC, 0xFD)를 만나면 CPU를 정지(lock) 상태로 만들고 `DebugView`에 노출한다.

### 4.3 실행 모델
- **M-사이클 tick 모델**: CPU의 모든 메모리 접근과 내부 지연마다 `Bus::tick()`을 호출해 주변장치를 4 T-사이클만큼 진행한다.
- 정상 속도에서는 PPU/APU에 4를, GBC 2배속 모드에서는 2를 전달한다. 타이머는 CPU 클럭을 따른다.
- **소유 구조**: `GameBoy { cpu: Cpu, bus: Bus }`. `Bus`가 cart, ppu, apu, timer, joypad, wram, hram, IE/IF, DMA를 모두 소유한다. CPU는 `cpu.step(&mut bus)`로 실행한다. `Rc<RefCell>`은 쓰지 않는다.
- **CPU 스텝**: ① 인터럽트 디스패치(5 M-사이클) → ② HALT면 1 M-사이클 진행(HALT 버그 포함) → ③ fetch/decode/execute(단일 `match` + r8/r16 인코딩 테이블) → `EI` 1명령 지연, `STOP`(속도 전환) 처리.
- **프레임 경계**: `run_frame()`은 PPU가 VBlank에 진입할 때까지 실행한다. LCD가 꺼져 있으면 70224 T-사이클(2배속이면 CPU 기준 그 2배) 후 종료한다.
- 브레이크포인트에 걸리면 `run_frame()`이 프레임 중간에 반환하고 `DebugView`에 정지 상태를 표시한다.

### 4.4 PPU
- 모드 상태 머신: 한 줄 456 T-사이클. 모드 2(0–79), 모드 3(80–251 고정), 모드 0(나머지), 144–153번째 줄은 모드 1.
- 모드 3에 진입할 때 해당 줄을 한 번에 렌더링한다(스캔라인 렌더러). 렌더러는 별도 함수로 분리해 나중에 픽셀 FIFO로 교체할 수 있게 한다.
- STAT 인터럽트는 (모드 0/1/2 조건 OR LY=LYC) STAT 라인의 상승 에지에서만 발생한다.
- 구현 범위: BG(SCX/SCY, 타일맵 선택, 타일 데이터 주소 모드), 윈도우(WX/WY, 내부 줄 카운터), 스프라이트(8×8/8×16, 줄당 10개, DMG는 X좌표 우선 / CGB는 OAM 순서 우선, BG 우선순위 속성), LCD on/off 처리, OAM DMA.
- CGB: VRAM 뱅크 1 타일 속성(팔레트, 뒤집기, 뱅크, BG 우선순위), BCPS/BCPD·OCPS/OCPD 팔레트 RAM, LCDC.0 의미 변화, HDMA(범용/HBlank), WRAM 뱅크(SVBK), VRAM 뱅크(VBK), KEY1 속도 전환.
- 색 변환: DMG는 선택 가능한 팔레트, CGB는 15비트→RGBA (LCD 색감 보정은 옵션).

### 4.5 APU
- CH1(구형파 + 스윕), CH2(구형파), CH3(웨이브 RAM), CH4(노이즈 LFSR 15/7비트).
- 프레임 시퀀서 512Hz (DIV 비트 연동): 길이 카운터, 엔벨로프, 스윕.
- 트리거, DAC on/off, NR50/51/52 패닝과 마스터 볼륨.
- 출력: 매 T-사이클 진행, 프론트엔드가 지정한 샘플레이트로 평균 다운샘플링, 하이패스 필터(DC 제거). `set_sample_rate`는 `f64`를 받으며, 프론트엔드는 오디오 버퍼 상태에 따라 보정한 값(예: 48000 × 1.003)을 전달한다.

### 4.6 카트리지
- 헤더 파싱: 제목, CGB 플래그, 카트리지 타입, ROM/RAM 크기, 헤더 체크섬.
- 모델 자동 선택: CGB 플래그가 0x80 또는 0xC0이면 CGB, 아니면 DMG. 사용자가 강제로 지정할 수 있다.
- `enum Mbc { None, Mbc1, Mbc3, Mbc5 }` (직렬화 용이성 때문에 trait 객체 대신 enum).
- 지원하지 않는 타입이면 `CartError::Unsupported(u8)`를 반환한다.
- MBC1: 뱅크 0→1 보정, 모드 레지스터. MBC3: RAM 뱅크 0–3, RTC 레지스터 0x08–0x0C, 래치(0→1). MBC5: 9비트 ROM 뱅크, 뱅크 0 선택 가능.

### 4.7 RTC (MBC3)
- 레지스터: 초, 분, 시, 일(하위 8비트), 일(상위 1비트 + 정지 + 캐리).
- 내부 사이클 기준으로 진행한다 (4194304 사이클 = 1초). 배속하면 게임 시간도 빨라진다.
- 오프라인 경과 시간: 배터리 세이브에 저장된 타임스탬프와 주입된 현재 시각의 차이만큼 로드할 때 진행시킨다.

### 4.8 세이브 형식
- **배터리 세이브 `.sav`**: SRAM 원본 + (MBC3+RTC인 경우) 48바이트 RTC 블록(현재 레지스터 5개 + 래치 레지스터 5개, 각 u32 LE, + u64 LE 유닉스 타임스탬프). BGB/VBA-M/mGBA와 호환된다.
- **세이브 스테이트**: `"ARGN"` 매직 + 스테이트 포맷 버전(u16) + ROM 체크섬(헤더 전역 체크섬 + 제목) + `bincode` 본문. ROM 데이터는 제외한다.
- 스테이트 포맷 버전은 앱 버전과 별도로 관리한다. minor/patch 릴리스에서는 이전 스테이트를 읽을 수 있어야 한다(새 필드는 `#[serde(default)]`). 호환이 깨지면 앱 major 버전을 올린다.
- 다른 ROM의 스테이트나 지원하지 않는 포맷 버전은 `StateError`로 거부한다.

## 5. 앱 계층 (aragorn-app)

### 5.1 세션
- ROM 로드: `SaveStore`에서 배터리 세이브를 읽고 `Clock` 시각과 함께 코어에 주입한다.
- 배터리 세이브 저장 시점: `battery_dirty()`가 참이고 게임이 외부 RAM을 비활성화한 직후(프레임 단위로 확인), 앱 종료, ROM 교체, 업데이트 적용 직전. 덮어쓰기 전에 `.sav.bak`을 1개 남긴다.
- 스테이트 슬롯: 10개(0–9), 각 슬롯에 썸네일을 포함한다.
- 속도 제어 계산(순수 함수): 경과 시간 → 실행할 프레임 수(최대 3), 오디오 버퍼 채움 비율 → 리샘플링 비율 보정(±0.5%), 배속 모드(2×/4×/무제한, 배속 중 음소거), 일시정지.

### 5.2 설정
- 키 매핑, 게임패드 매핑, DMG 팔레트, CGB 색 보정, 세이브 위치(데이터 디렉터리 / ROM 옆), 자동 업데이트 켜짐 여부(기본 켜짐), 건너뛴 업데이트 버전.
- `config.toml`로 저장한다 (OS 설정 디렉터리).

### 5.3 업데이트 정책 판단
```rust
struct UpdatePolicy { latest: Version, minimum_supported: Version, message: String }
enum UpdateDecision { UpToDate, Soft { version: Version, auto_download: bool }, Forced { version: Version } }
fn decide(current: &Version, policy: &UpdatePolicy, prefs: &UpdatePrefs) -> UpdateDecision;
```
- `current < minimum_supported` → `Forced`
- `current < latest`이고 `latest`가 건너뛴 버전이 아님 → `Soft`
- 그 외 → `UpToDate`
- 프리릴리스 버전은 무시한다.

## 6. 데스크톱 계층 (aragorn-desktop)

### 6.1 실행 루프
- 에뮬레이션은 eframe `update()` 안, UI와 같은 스레드에서 돌린다. 디버거가 락 없이 상태를 읽을 수 있다.
- 매 `update()`: 속도 제어로 계산한 프레임 수만큼 `run_frame()` → `drain_audio()` → 링 버퍼 → cpal 콜백 스레드 → 프레임버퍼를 egui 텍스처로 갱신 → `request_repaint()`.
- 오디오 장치가 없으면 소리 없이 계속 실행한다.

### 6.2 입력
- 키보드 기본값: 방향키, Z=B, X=A, Enter=Start, Backspace=Select, Tab=배속(누르는 동안), Esc=일시정지, F1–F10=스테이트 저장, Shift+F1–F10=스테이트 로드.
- 게임패드: gilrs로 자동 인식, 기본 매핑(남쪽=A, 동쪽=B, Start, Select, D-pad/왼쪽 스틱).
- 설정 창에서 모두 다시 지정할 수 있다.

### 6.3 디버거 UI
- CPU: 레지스터, 플래그, IME, HALT 상태, PC 주변 디스어셈블리
- 실행 제어: 일시정지, 명령 1개 실행, 프레임 1개 실행, PC 브레이크포인트
- 메모리: 16진수 뷰어, 주소 이동
- VRAM: 타일 데이터(뱅크 0/1), BG/윈도우 타일맵, OAM 목록, CGB 팔레트
- I/O: LCDC, STAT, 타이머, APU 레지스터, 채널별 음소거

### 6.4 오류 표시
- ROM 로드 실패, 세이브 I/O 오류, 업데이트 오류는 대화상자나 상태 표시줄로 알린다.
- 로그는 `log` + `env_logger`로 남긴다.

## 7. 업데이트와 배포

### 7.1 구성
- 설치와 업데이트 적용은 **Velopack**(Rust SDK + `vpk` CLI)으로 한다. Windows Setup, macOS 패키지, Linux AppImage를 만들고, 업데이트 채널은 GitHub Releases다.
- 강제/소프트 정책은 별도 파일 `update-policy.json`으로 관리하고, 같은 위치에 ed25519 서명 `update-policy.json.sig`를 둔다.
  ```json
  { "latest": "1.4.2", "minimum_supported": "1.2.0", "message": "..." }
  ```
- 정책 URL: `https://github.com/<owner>/aragorn/releases/latest/download/update-policy.json`
- 서명 개인키는 GitHub Secrets에 두고, 공개키는 빌드할 때 바이너리에 내장한다.
- 저장소는 public이다 (인증 없이 다운로드하기 위해).

### 7.2 동작
1. 앱을 시작하면 백그라운드에서 정책을 확인한다 (타임아웃 5초). 실행 중에는 6시간마다 다시 확인한다.
2. `Forced`: 에뮬레이션 시작 전이면 시작하지 않고, 실행 중이면 일시정지한 뒤 모달을 띄운다. 버튼은 "업데이트" / "종료"뿐이다.
3. `Soft`: 게임을 방해하지 않는 배너를 띄운다. "지금 업데이트 / 나중에 / 이 버전 건너뛰기"를 고를 수 있다.
4. 자동 업데이트 설정이 켜져 있으면 `Soft` 버전을 백그라운드에서 다운로드해 두고, 다음 실행 시 적용하거나 "재시작하여 업데이트"로 즉시 적용한다. 꺼져 있으면 알림만 띄운다.
5. 적용 직전에 배터리 세이브와 설정을 저장한다.
6. 네트워크 오류, 서명 불일치, 다운로드 실패가 나면 로그를 남기고 현재 버전으로 계속 실행한다. 서명이 맞지 않는 정책이나 패키지는 절대 적용하지 않는다. 오프라인이면 강제 업데이트도 적용되지 않는다(의도된 동작).

### 7.3 버저닝 (Semantic Versioning: major.minor.patch)
- 버전의 단일 출처는 워크스페이스 `Cargo.toml`의 `workspace.package.version`이다.
- major: 세이브 스테이트나 설정 호환성이 깨지는 변경. minor: 기능 추가. patch: 버그 수정.
- 강제 업데이트(`minimum_supported` 인상)는 세이브 손상 같은 치명적 결함에만 쓴다. `release/policy.toml`에서 사람이 직접 수정한다.

### 7.4 릴리스 CI (GitHub Actions)
- PR/푸시: Windows, macOS, Linux에서 `cargo build`, `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check`.
- `vX.Y.Z` 태그 푸시: 태그와 `Cargo.toml` 버전 일치 확인 → 3개 OS 빌드와 테스트 → `vpk pack` → `release/policy.toml`로 `update-policy.json` 생성 → ed25519 서명 → GitHub Release 업로드.

## 8. 테스트 전략
1. **단위 테스트 (TDD)**: CPU 명령별 결과와 플래그, 인터럽트와 HALT, 타이머, MBC 뱅크 전환, RTC 래치와 롤오버, PPU 모드 전이와 STAT, 세이브 형식 왕복 변환, 스테이트 버전 호환, `decide()` 버전 조합 표, 정책 서명 검증(정상/변조/잘못된 키), 속도 제어 계산.
2. **포트 가짜 구현 테스트**: 세션(배터리 세이브 저장 시점, 백업), 업데이트 흐름 상태 전이.
3. **테스트 ROM 통합 테스트**:
   - Blargg: 시리얼 출력(0xFF01/0xFF02)에서 `"Passed"` 확인
   - mooneye: 종료 시 B,C,D,E,H,L = 3,5,8,13,21,34 확인
   - dmg-acid2 / cgb-acid2: 프레임버퍼를 기준 PNG와 픽셀 단위 비교
   - ROM은 `scripts/fetch-test-roms.sh`로 내려받는다. 없으면 테스트를 건너뛰고 경고를 출력한다. CI에서는 반드시 내려받아 실행한다.
4. **수동 체크리스트** (상용 ROM은 저장소에 넣지 않는다):
   - 레드: 타이틀 → 첫 배틀 → 저장 → 재시작 후 이어하기
   - 피카츄: 피카츄 울음소리 확인
   - 금/은: 시계 설정 → 앱 종료 후 시간 경과 → 시간 이벤트 반영 확인
   - 크리스탈: 오프닝 → 첫 마을 → 저장/로드, 세이브 스테이트, 다른 에뮬레이터의 `.sav` 가져오기
   - 업데이트: v0.1.0 설치 → v0.1.1 릴리스로 소프트 업데이트 → `minimum_supported` 인상으로 강제 업데이트

## 9. 마일스톤
0. **걸어다니는 뼈대**: 3계층 워크스페이스, 빈 eframe 창, CI, 릴리스 CI(Velopack), 정책 서명, 강제/소프트/자동 업데이트
1. CPU + 버스 → Blargg `cpu_instrs` (헤드리스)
2. 타이머, 인터럽트, HALT → `instr_timing`, `mem_timing`
3. PPU + 화면 출력 → `dmg-acid2`, 레드/블루 화면
4. MBC1/3/5, 입력, 배터리 세이브 → 레드/블루/피카츄 플레이
5. APU + cpal → 사운드 (`dmg_sound`)
6. CGB 기능 + RTC → `cgb-acid2`, `cgb_sound`, 금/은/크리스탈
7. 세이브 스테이트, 배속/일시정지, 키 설정, 게임패드
8. 디버거 UI 완성 (1단계부터 필요한 만큼 점진적으로 추가)

## 10. 참고 자료
- Pan Docs: https://gbdev.io/pandocs/
- SM83 옵코드 표: https://gbdev.io/gb-opcodes/optables/
- GBEDG: https://hacktix.github.io/GBEDG/
- 테스트 ROM: Blargg gb-test-roms, mooneye-test-suite, dmg-acid2, cgb-acid2
- Velopack: https://velopack.io/
