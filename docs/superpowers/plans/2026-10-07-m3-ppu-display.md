# 마일스톤 3: PPU, 화면 출력, ROM 뱅크 전환 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** DMG PPU(모드·STAT·스캔라인 렌더러)와 MBC1/3/5 ROM·RAM 뱅크 전환을 구현하고, 데스크톱 앱에서 ROM을 열어 화면을 실시간으로 보여 준다. `dmg-acid2`가 기준 화면과 픽셀 단위로 일치하고 포켓몬 레드/블루의 타이틀 화면이 뜨는 것이 목표다.

**Architecture:**
- `gb-core`:
  - `Cartridge`가 `enum Mbc`로 MBC1/3/5 레지스터를 가진다.
  - `Ppu`는 4 dot 단위로 모드를 진행하고 STAT 신호의 상승 에지에서 인터럽트를 요청한다. 모드 3에 들어갈 때 한 줄을 그린다.
  - `GameBoy::framebuffer()`가 160×144 RGBA(0xRRGGBBAA)를 준다.
- `aragorn-app`: 순수 함수 `FramePacer`(경과 시간 → 프레임 수, 최대 3)와 `Session`(ROM 하나의 실행)을 둔다.
- `aragorn-desktop`:
  - ROM 열기(파일 대화상자, 끌어다 놓기, 명령줄 인자), `ScreenView`(정수 배율 텍스처), `logic()`의 에뮬레이션 루프를 둔다.
  - 강제 업데이트 중에는 에뮬레이션을 멈춘다.

**Tech Stack:** Rust 1.97 (edition 2024), eframe/egui 0.36.2, rfd 0.17.2(파일 대화상자), png 0.17(gb-core 테스트 전용)

**Spec:** `docs/superpowers/specs/2026-10-07-gameboy-emulator-design.md` (§4.4 PPU, §4.6 카트리지, §5.1 세션·속도 제어, §6.1 실행 루프, §8-3, §9의 마일스톤 3)

**작업 브랜치:** `feat/m3-ppu-display`

## Global Constraints

- `gb-core`의 일반 의존성은 추가하지 않는다. 테스트 전용 `png`(dev-dependency)만 추가한다.
- 핫 패스에서 `dyn`과 `Rc<RefCell>`을 쓰지 않는다. MBC는 trait 객체 대신 `enum`이다(스펙 §4.6).
- 게임 데이터 때문에 패닉하지 않는다. ROM/RAM 뱅크 번호는 실제 크기로 감싼다(modulo).
- 이름은 Pan Docs 용어를 따른다. 사용자에게 보이는 문구는 한국어다.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace --no-fail-fast`가 모든 커밋에서 통과해야 한다.
- 커밋 메시지 끝에는 `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`를 붙인다.
- 이 마일스톤에서는 릴리스하지 않는다. 사용자 확인 뒤에 v0.2.0으로 릴리스할지 정한다.

## 계획 전 검증 (스파이크)

이 계획의 코드는 별도 worktree에 먼저 적용해서 실제 ROM과 테스트로 확인한 것이다.
- **dmg-acid2**: 기준 PNG와 픽셀 차이가 0이다.
- **mooneye**: 50개 → 77개다.
  - MBC1 12개, MBC5 8개, `oam_dma/sources-GS`, PPU 6개가 늘었다.
  - 이 중 PPU 6개는 렌더러가 없는 Task 2 상태에서도 통과한다.
- **Blargg**: 15개를 유지한다.
- **단위 테스트**: core 141, app 54, desktop 31이다. clippy도 깨끗하다.
- **WSLg 실행**: 데스크톱 앱을 dmg-acid2로 띄워 보면 Wayland 연결이 가끔 끊겨 종료된다(`Connection reset by peer`).
  - ROM 없이 띄운 경우는 그렇지 않았다.
  - repaint 빈도를 낮추거나 창 제목 변경을 빼도 끊겼다. 그래서 앱 문제가 아니라 WSLg Wayland 문제로 판단했다.
  - 화면 확인은 네이티브 Windows/macOS에서 한다(Task 6).

## 스펙과 다른 결정 (리뷰어 확인용)

1. **MBC1/3/5의 ROM·RAM 뱅크 전환을 M4에서 M3로 앞당긴다.** 사용자가 결정했다. 포켓몬 화면을 보려면 필요하다. 배터리 세이브 저장, MBC3 RTC, 입력은 계획대로 M4/M6에서 한다.
   - MBC3의 RTC 레지스터 선택(0x08–0x0C)은 0xFF를 읽고 쓰기를 무시한다.
   - MBC1 멀티카트(MBC1M)와 MBC2는 지원하지 않는다. MBC2는 `Unsupported`다.
2. **mooneye PPU 테스트는 12개 중 6개만 대상이다.** 나머지 6개는 이런 기능이 필요하다.
   - 가변 길이 모드 3: 스펙상 고정 길이다.
   - 모드 2/3 중 OAM 접근 차단
   - LCD 켤 때의 미세 동작(`lcdon_*`, `stat_lyc_onoff`)
3. **DMG 기본 팔레트는 고전 녹색 계열**(`DEFAULT_DMG_PALETTE`)이다. 팔레트 선택 설정(스펙 §5.2)은 설정 화면과 함께 M7에서 한다.
4. **스펙 수정**(Task 6에서 함께 커밋한다):
   - 대상 게임 표의 레드/블루 MBC 표기를 바로잡는다. 북미판 레드/블루는 MBC3다.
   - mooneye 판정 방법을 시리얼 출력으로 바꾼다(M2 리뷰 지적).
   - 마일스톤 표를 고친다. M2에는 OAM DMA가 들어가고, M3에는 MBC 뱅크 전환이 들어간다.

## Review Focus

1. **헤더에 적힌 것보다 작은 ROM이나 큰 뱅크 번호**: 실제 뱅크 수로 감싸야 하고 패닉하면 안 된다. → Task 1 `rom_bank_wraps_to_rom_size`, M1 `short_rom_reads_open_bus`
2. **RAM이 없는 카트리지에서 RAM 접근, 또는 RAM을 끈 상태의 접근**: 0xFF를 읽고 쓰기는 무시해야 한다. → Task 1 `mbc1_ram_needs_enable_and_banks_only_in_mode1`, `rom_writes_are_ignored_and_ram_is_absent`
3. **스프라이트 11개 이상이 한 줄에 있거나, 화면 밖 좌표(X=0, Y=0)인 스프라이트**: 앞의 10개만 그리고 패닉하지 않아야 한다. → Task 3 `only_ten_sprites_per_line`, acid2
4. **잘못된 ROM 파일을 열거나 끌어다 놓는 경우**: 앱은 계속 돌고, 상태 표시줄에 한국어로 이유가 나와야 한다. → Task 5 `missing_rom_file_reports_read_error`, `invalid_rom_reports_cartridge_error`
5. **강제 업데이트 창이 떠 있는 동안**: 에뮬레이션이 진행되면 안 된다. → Task 5 `run_emulation`의 `blocks_emulation` 분기(수동 확인 항목)

---

## 파일 구조

```
aragorn/
├── docs/superpowers/specs/2026-10-07-gameboy-emulator-design.md   # 스펙 수정 (Task 6)
├── scripts/fetch-test-roms.sh               # dmg-acid2 ROM과 기준 PNG (Task 3)
├── crates/gb-core/
│   ├── Cargo.toml                           # [dev-dependencies] png (Task 3)
│   ├── src/cartridge.rs                     # Mbc enum, ROM/RAM 뱅크 (Task 1)
│   ├── src/ppu.rs                           # 모드·STAT (Task 2), 렌더러·프레임버퍼 (Task 3)
│   ├── src/bus.rs                           # PPU 레지스터 전체 연결, 인터럽트 비트 (Task 2), ppu() (Task 3)
│   ├── src/gameboy.rs                       # framebuffer() (Task 3), 테스트 수정 (Task 1)
│   └── tests/{mooneye.rs, acid2.rs}         # (Task 1–3)
├── crates/aragorn-app/src/{pacing.rs, session.rs, lib.rs}            # (Task 4)
└── crates/aragorn-desktop/
    ├── Cargo.toml                           # gb-core, rfd (Task 5)
    ├── src/ui/{screen.rs, mod.rs}           # (Task 5)
    ├── src/app.rs                           # ROM 열기, 에뮬레이션 루프 (Task 5)
    └── src/main.rs                          # 명령줄 ROM 경로 (Task 5)
```

---

### Task 1: MBC1/3/5 ROM·RAM 뱅크 전환

**Files:**
- Modify: `crates/gb-core/src/cartridge.rs`, `crates/gb-core/src/gameboy.rs`, `crates/gb-core/tests/mooneye.rs`

**Interfaces:**
- Consumes: M1 `Header`, `CartError`, `header_checksum`
- Produces:
  - `Cartridge::new`가 카트리지 타입 0x00/0x08/0x09(ROM, RAM 선택), 0x01–0x03(MBC1), 0x0F–0x13(MBC3), 0x19–0x1E(MBC5)를 받아들인다. 그 밖의 타입은 `Unsupported`다.
  - `read_rom`, `write_rom`, `read_ram`, `write_ram`의 시그니처는 그대로다.

- [ ] **Step 1: 브랜치를 만들고 실패하는 테스트를 작성한다**

```bash
git checkout main && git pull --ff-only && git checkout -b feat/m3-ppu-display
```

`cartridge.rs` tests 모듈에서 `accepts_32k_rom_only_and_mbc1`과 `rejects_banked_or_unknown_cartridges_for_now`를 지운다. 그 자리(`short_rom_reads_open_bus` 위)에 아래를 넣는다:

```rust
    /// 각 뱅크 첫 두 바이트에 (뱅크 번호 하위, 상위)를 적은 ROM. 0x4000 읽기로 매핑된 뱅크를 알 수 있다.
    fn banked_rom(cart_type: u8, rom_size_code: u8, ram_size_code: u8) -> Vec<u8> {
        let banks = 2usize << rom_size_code;
        let mut rom = vec![0; banks * 0x4000];
        for bank in 1..banks {
            rom[bank * 0x4000] = bank as u8;
            rom[bank * 0x4000 + 1] = (bank >> 8) as u8;
        }
        rom[0x0134..0x0138].copy_from_slice(b"TEST");
        rom[0x0147] = cart_type;
        rom[0x0148] = rom_size_code;
        rom[0x0149] = ram_size_code;
        rom[0x014D] = header_checksum(&rom);
        rom
    }

    fn mapped_bank(cart: &Cartridge, base: u16) -> u16 {
        u16::from_le_bytes([cart.read_rom(base), cart.read_rom(base + 1)])
    }

    #[test]
    fn accepts_rom_only_mbc1_mbc3_and_mbc5_types() {
        let types = [
            0x00, 0x01, 0x02, 0x03, 0x08, 0x09, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x19, 0x1A, 0x1B,
            0x1C, 0x1D, 0x1E,
        ];
        for t in types {
            assert!(
                Cartridge::new(banked_rom(t, 0x01, 0x00)).is_ok(),
                "{t:#04X}"
            );
        }
    }

    #[test]
    fn rejects_unsupported_cartridge_types() {
        for t in [0x05, 0x06, 0x0B, 0x20, 0xFC, 0xFF] {
            assert_eq!(
                Cartridge::new(banked_rom(t, 0x01, 0x00)).err(),
                Some(CartError::Unsupported(t))
            );
        }
    }

    #[test]
    fn mbc1_switches_rom_bank_and_maps_zero_to_one() {
        let mut cart = Cartridge::new(banked_rom(0x01, 0x05, 0x00)).unwrap();
        assert_eq!(mapped_bank(&cart, 0x4000), 1);
        cart.write_rom(0x2000, 0x05);
        assert_eq!(mapped_bank(&cart, 0x4000), 5);
        cart.write_rom(0x2000, 0x00);
        assert_eq!(mapped_bank(&cart, 0x4000), 1);
        cart.write_rom(0x3FFF, 0x21);
        assert_eq!(mapped_bank(&cart, 0x4000), 1);
    }

    #[test]
    fn mbc1_upper_bits_select_large_banks_and_mode_maps_bank0_area() {
        let mut cart = Cartridge::new(banked_rom(0x01, 0x06, 0x00)).unwrap();
        cart.write_rom(0x2000, 0x02);
        cart.write_rom(0x4000, 0x01);
        assert_eq!(mapped_bank(&cart, 0x4000), 0x22);
        assert_eq!(mapped_bank(&cart, 0x0000), 0);
        cart.write_rom(0x6000, 0x01);
        assert_eq!(mapped_bank(&cart, 0x0000), 0x20);
    }

    #[test]
    fn rom_bank_wraps_to_rom_size() {
        let mut cart = Cartridge::new(banked_rom(0x01, 0x03, 0x00)).unwrap();
        cart.write_rom(0x2000, 0x13);
        assert_eq!(mapped_bank(&cart, 0x4000), 0x13 % 16);
    }

    #[test]
    fn mbc1_ram_needs_enable_and_banks_only_in_mode1() {
        let mut cart = Cartridge::new(banked_rom(0x03, 0x01, 0x03)).unwrap();
        cart.write_ram(0xA000, 0x11);
        assert_eq!(cart.read_ram(0xA000), 0xFF, "꺼진 RAM은 0xFF");
        cart.write_rom(0x0000, 0x0A);
        cart.write_ram(0xA000, 0x11);
        cart.write_rom(0x4000, 0x01);
        assert_eq!(cart.read_ram(0xA000), 0x11, "모드 0에서는 RAM 뱅크 0 고정");
        cart.write_rom(0x6000, 0x01);
        assert_eq!(cart.read_ram(0xA000), 0x00);
        cart.write_ram(0xA000, 0x22);
        cart.write_rom(0x6000, 0x00);
        assert_eq!(cart.read_ram(0xA000), 0x11);
        cart.write_rom(0x0000, 0x00);
        assert_eq!(cart.read_ram(0xA000), 0xFF);
    }

    #[test]
    fn mbc3_selects_7bit_rom_bank_ram_banks_and_hides_rtc() {
        let mut cart = Cartridge::new(banked_rom(0x13, 0x06, 0x03)).unwrap();
        cart.write_rom(0x2000, 0x45);
        assert_eq!(mapped_bank(&cart, 0x4000), 0x45);
        cart.write_rom(0x2000, 0x00);
        assert_eq!(mapped_bank(&cart, 0x4000), 1);
        cart.write_rom(0x0000, 0x0A);
        cart.write_rom(0x4000, 0x02);
        cart.write_ram(0xA123, 0x33);
        cart.write_rom(0x4000, 0x00);
        assert_eq!(cart.read_ram(0xA123), 0x00);
        cart.write_rom(0x4000, 0x02);
        assert_eq!(cart.read_ram(0xA123), 0x33);
        cart.write_rom(0x4000, 0x08);
        assert_eq!(cart.read_ram(0xA123), 0xFF, "RTC 레지스터는 M6에서 구현");
    }

    #[test]
    fn mbc5_uses_9bit_rom_bank_including_zero() {
        let mut cart = Cartridge::new(banked_rom(0x19, 0x08, 0x00)).unwrap();
        cart.write_rom(0x2000, 0x00);
        assert_eq!(mapped_bank(&cart, 0x4000), 0);
        cart.write_rom(0x3000, 0x01);
        cart.write_rom(0x2000, 0x05);
        assert_eq!(mapped_bank(&cart, 0x4000), 0x105);
    }

    #[test]
    fn mbc5_banks_ram() {
        let mut cart = Cartridge::new(banked_rom(0x1B, 0x01, 0x04)).unwrap();
        cart.write_rom(0x0000, 0x0A);
        cart.write_rom(0x4000, 0x0F);
        cart.write_ram(0xBFFF, 0x44);
        cart.write_rom(0x4000, 0x00);
        assert_eq!(cart.read_ram(0xBFFF), 0x00);
        cart.write_rom(0x4000, 0x0F);
        assert_eq!(cart.read_ram(0xBFFF), 0x44);
    }

    #[test]
    fn rom_only_ram_is_always_enabled() {
        let mut cart = Cartridge::new(banked_rom(0x08, 0x00, 0x02)).unwrap();
        cart.write_ram(0xA000, 0x55);
        assert_eq!(cart.read_ram(0xA000), 0x55);
    }
```

`gameboy.rs`의 `unsupported_cartridge_is_an_error` 테스트에서 `0x13`(이제 MBC3로 지원)을 `0x05`(MBC2)로 바꾼다:

```rust
        assert_eq!(
            GameBoy::new(test_rom(0x05, 0x00, 0x00), Model::Auto).err(),
            Some(CartError::Unsupported(0x05))
        );
```

`tests/mooneye.rs`의 `mooneye_tests!` 목록 끝(`}` 앞)에 추가한다:

```rust
    oam_dma_sources_gs => "acceptance/oam_dma/sources-GS.gb",
    mbc1_bits_bank1 => "emulator-only/mbc1/bits_bank1.gb",
    mbc1_bits_bank2 => "emulator-only/mbc1/bits_bank2.gb",
    mbc1_bits_mode => "emulator-only/mbc1/bits_mode.gb",
    mbc1_bits_ramg => "emulator-only/mbc1/bits_ramg.gb",
    mbc1_ram_64kb => "emulator-only/mbc1/ram_64kb.gb",
    mbc1_ram_256kb => "emulator-only/mbc1/ram_256kb.gb",
    mbc1_rom_512kb => "emulator-only/mbc1/rom_512kb.gb",
    mbc1_rom_1mb => "emulator-only/mbc1/rom_1Mb.gb",
    mbc1_rom_2mb => "emulator-only/mbc1/rom_2Mb.gb",
    mbc1_rom_4mb => "emulator-only/mbc1/rom_4Mb.gb",
    mbc1_rom_8mb => "emulator-only/mbc1/rom_8Mb.gb",
    mbc1_rom_16mb => "emulator-only/mbc1/rom_16Mb.gb",
    mbc5_rom_512kb => "emulator-only/mbc5/rom_512kb.gb",
    mbc5_rom_1mb => "emulator-only/mbc5/rom_1Mb.gb",
    mbc5_rom_2mb => "emulator-only/mbc5/rom_2Mb.gb",
    mbc5_rom_4mb => "emulator-only/mbc5/rom_4Mb.gb",
    mbc5_rom_8mb => "emulator-only/mbc5/rom_8Mb.gb",
    mbc5_rom_16mb => "emulator-only/mbc5/rom_16Mb.gb",
    mbc5_rom_32mb => "emulator-only/mbc5/rom_32Mb.gb",
    mbc5_rom_64mb => "emulator-only/mbc5/rom_64Mb.gb",
```

- [ ] **Step 2: 테스트가 실패하는지 확인한다**

Run: `cargo test -p gb-core --lib cartridge:: 2>&1 | grep -E "FAILED|test result"; ARAGORN_REQUIRE_TEST_ROMS=1 cargo test -p gb-core --test mooneye 2>&1 | grep "test result"`
Expected: FAIL.
- 단위 테스트는 9개가 실패한다. 모두 `Unsupported`로 `unwrap`하다 패닉하거나, 타입 0x08을 받아들이지 않아서 실패한다.
- `rejects_unsupported_cartridge_types`는 이미 통과한다.
- mooneye는 `50 passed; 21 failed`다.

- [ ] **Step 3: 구현한다**

`cartridge.rs` 모듈 문서의 `//! M1은 뱅크 전환이 필요 없는 32KB ROM만 지원한다. MBC1/3/5는 M4에서 구현한다.`를 아래로 바꾼다:

```rust
//! ROM-only, MBC1, MBC3, MBC5의 ROM/RAM 뱅크 전환을 지원한다. 배터리 세이브와 MBC3 RTC는 M4/M6에서 다룬다.
```

`pub struct Cartridge {`부터 `fn header_checksum(` 앞까지를 아래로 교체한다:

```rust
/// 메모리 뱅크 컨트롤러 (Pan Docs "MBCs"). 직렬화를 위해 trait 객체 대신 enum으로 둔다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mbc {
    /// ROM-only (+RAM). 뱅크 전환 없음.
    None,
    /// `bank1`: 5비트 ROM 뱅크(0은 1로 보정), `bank2`: 2비트(ROM 상위 비트 또는 RAM 뱅크), `mode`: 뱅킹 모드.
    Mbc1 { bank1: u8, bank2: u8, mode: bool },
    /// `rom_bank`: 7비트(0은 1로 보정), `ram_select`: 0–3은 RAM 뱅크, 0x08–0x0C는 RTC 레지스터(M6).
    Mbc3 { rom_bank: u8, ram_select: u8 },
    /// `rom_bank`: 9비트(0도 그대로), `ram_bank`: 4비트.
    Mbc5 { rom_bank: u16, ram_bank: u8 },
}

pub struct Cartridge {
    header: Header,
    rom: Vec<u8>,
    ram: Vec<u8>,
    ram_enabled: bool,
    mbc: Mbc,
}

/// 헤더의 RAM 크기 코드(0x0149)를 바이트 수로 바꾼다.
fn ram_size(code: u8) -> usize {
    match code {
        0x02 => 0x2000,
        0x03 => 0x8000,
        0x04 => 0x20000,
        0x05 => 0x10000,
        _ => 0,
    }
}

impl Cartridge {
    pub fn new(rom: Vec<u8>) -> Result<Cartridge, CartError> {
        let header = Header::parse(&rom)?;
        let mbc = match header.cart_type {
            0x00 | 0x08 | 0x09 => Mbc::None,
            0x01..=0x03 => Mbc::Mbc1 {
                bank1: 1,
                bank2: 0,
                mode: false,
            },
            0x0F..=0x13 => Mbc::Mbc3 {
                rom_bank: 1,
                ram_select: 0,
            },
            0x19..=0x1E => Mbc::Mbc5 {
                rom_bank: 1,
                ram_bank: 0,
            },
            other => return Err(CartError::Unsupported(other)),
        };
        let ram = vec![0; ram_size(header.ram_size_code)];
        Ok(Cartridge {
            header,
            rom,
            ram,
            ram_enabled: false,
            mbc,
        })
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    fn rom_byte(&self, bank: usize, addr: u16) -> u8 {
        let banks = (self.rom.len() / 0x4000).max(1);
        let offset = (bank % banks) * 0x4000 + usize::from(addr & 0x3FFF);
        self.rom.get(offset).copied().unwrap_or(0xFF)
    }

    /// 0x0000–0x7FFF. ROM 파일보다 뒤쪽은 0xFF(오픈 버스)다.
    pub fn read_rom(&self, addr: u16) -> u8 {
        let bank = match (self.mbc, addr) {
            (Mbc::None, _) => return self.rom.get(usize::from(addr)).copied().unwrap_or(0xFF),
            (Mbc::Mbc1 { bank2, mode, .. }, 0x0000..=0x3FFF) => {
                if mode {
                    usize::from(bank2) << 5
                } else {
                    0
                }
            }
            (_, 0x0000..=0x3FFF) => 0,
            (Mbc::Mbc1 { bank1, bank2, .. }, _) => (usize::from(bank2) << 5) | usize::from(bank1),
            (Mbc::Mbc3 { rom_bank, .. }, _) => usize::from(rom_bank),
            (Mbc::Mbc5 { rom_bank, .. }, _) => usize::from(rom_bank),
        };
        self.rom_byte(bank, addr)
    }

    /// MBC 레지스터 쓰기 (0x0000–0x7FFF).
    pub fn write_rom(&mut self, addr: u16, value: u8) {
        match (&mut self.mbc, addr) {
            (Mbc::None, _) => {}
            (_, 0x0000..=0x1FFF) => self.ram_enabled = value & 0x0F == 0x0A,
            (Mbc::Mbc1 { bank1, .. }, 0x2000..=0x3FFF) => *bank1 = (value & 0x1F).max(1),
            (Mbc::Mbc1 { bank2, .. }, 0x4000..=0x5FFF) => *bank2 = value & 0x03,
            (Mbc::Mbc1 { mode, .. }, _) => *mode = value & 0x01 != 0,
            (Mbc::Mbc3 { rom_bank, .. }, 0x2000..=0x3FFF) => *rom_bank = (value & 0x7F).max(1),
            (Mbc::Mbc3 { ram_select, .. }, 0x4000..=0x5FFF) => *ram_select = value & 0x0F,
            // RTC 래치(0x6000–0x7FFF)는 M6에서 구현한다.
            (Mbc::Mbc3 { .. }, _) => {}
            (Mbc::Mbc5 { rom_bank, .. }, 0x2000..=0x2FFF) => {
                *rom_bank = (*rom_bank & 0x100) | u16::from(value);
            }
            (Mbc::Mbc5 { rom_bank, .. }, 0x3000..=0x3FFF) => {
                *rom_bank = (*rom_bank & 0xFF) | (u16::from(value & 0x01) << 8);
            }
            (Mbc::Mbc5 { ram_bank, .. }, 0x4000..=0x5FFF) => *ram_bank = value & 0x0F,
            (Mbc::Mbc5 { .. }, _) => {}
        }
    }

    /// 외부 RAM 안의 바이트 위치. RAM이 꺼져 있거나 없거나 RTC가 선택되었으면 `None`.
    fn ram_offset(&self, addr: u16) -> Option<usize> {
        if self.ram.is_empty() || (!self.ram_enabled && self.mbc != Mbc::None) {
            return None;
        }
        let bank = match self.mbc {
            Mbc::None => 0,
            Mbc::Mbc1 { bank2, mode, .. } => {
                if mode {
                    usize::from(bank2)
                } else {
                    0
                }
            }
            Mbc::Mbc3 { ram_select, .. } if ram_select <= 0x03 => usize::from(ram_select),
            Mbc::Mbc3 { .. } => return None,
            Mbc::Mbc5 { ram_bank, .. } => usize::from(ram_bank),
        };
        Some((bank * 0x2000 + usize::from(addr & 0x1FFF)) % self.ram.len())
    }

    /// 0xA000–0xBFFF. RAM이 꺼져 있거나 없으면 0xFF.
    pub fn read_ram(&self, addr: u16) -> u8 {
        self.ram_offset(addr).map_or(0xFF, |i| self.ram[i])
    }

    pub fn write_ram(&mut self, addr: u16, value: u8) {
        if let Some(i) = self.ram_offset(addr) {
            self.ram[i] = value;
        }
    }
}
```

- [ ] **Step 4: 테스트가 통과하는지 확인한다**

Run: `cargo fmt --all && cargo test -p gb-core --lib 2>&1 | grep -E "FAILED|test result" && ARAGORN_REQUIRE_TEST_ROMS=1 cargo test -p gb-core --no-fail-fast --test mooneye --test blargg 2>&1 | grep -E "FAILED|test result" && cargo clippy -p gb-core --all-targets -- -D warnings`
Expected: lib `128 passed`, mooneye `71 passed`, blargg `15 passed`. clippy 출력 없음.

- [ ] **Step 5: 커밋한다**

```bash
git add crates/gb-core
git commit -m "feat(core): add MBC1/MBC3/MBC5 ROM and RAM banking

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: PPU 모드, STAT, LYC

**Files:**
- Modify: `crates/gb-core/src/ppu.rs`, `crates/gb-core/src/bus.rs`, `crates/gb-core/tests/mooneye.rs`

**Interfaces:**
- Consumes: M1/M2 `Ppu`, `Bus`
- Produces (`gb_core::ppu`):
  - 상수 `LCDC, STAT, SCY, SCX, LY, LYC, BGP, OBP0, OBP1, WY, WX: u16`, `IRQ_VBLANK = 0x01`, `IRQ_STAT = 0x02`
  - `Ppu::tick(&mut self, dots: u32) -> u8`: 요청할 IF 비트를 반환한다.
  - `Ppu::write_reg(&mut self, addr, value) -> u8`: STAT 신호가 새로 켜지면 `IRQ_STAT`을 반환한다.
  - 버스는 0xFF40–0xFF45와 0xFF47–0xFF4B를 PPU로 보낸다.

- [ ] **Step 1: 실패하는 테스트를 작성한다**

`ppu.rs`의 `#[cfg(test)] mod tests { ... }` 전체를 아래로 교체한다:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// `m_cycles`번 4 dot씩 진행하고 요청된 인터럽트 비트를 모두 OR해서 반환한다.
    fn ticks(ppu: &mut Ppu, m_cycles: u32) -> u8 {
        (0..m_cycles).fold(0, |irq, _| irq | ppu.tick(4))
    }

    fn mode(ppu: &Ppu) -> u8 {
        ppu.read_reg(STAT) & 0x03
    }

    /// LCD 켜짐, BG 켜짐, 0x8000 타일 데이터, BGP=0xE4(색 번호 = 음영)인 PPU.
    fn ppu() -> Ppu {
        let mut p = Ppu::default();
        p.write_reg(LCDC, 0x91);
        p.write_reg(BGP, 0xE4);
        p.write_reg(OBP0, 0xE4);
        p
    }

    #[test]
    fn ly_advances_every_456_dots() {
        let mut p = ppu();
        ticks(&mut p, 113);
        assert_eq!(p.read_reg(LY), 0);
        ticks(&mut p, 1);
        assert_eq!(p.read_reg(LY), 1);
    }

    #[test]
    fn modes_follow_line_timing() {
        let mut p = ppu();
        assert_eq!(mode(&p), 2);
        ticks(&mut p, 20);
        assert_eq!(mode(&p), 3);
        ticks(&mut p, 43);
        assert_eq!(mode(&p), 0);
        ticks(&mut p, 51);
        assert_eq!((p.read_reg(LY), mode(&p)), (1, 2));
    }

    #[test]
    fn vblank_starts_at_line_144() {
        let mut p = ppu();
        assert_eq!(ticks(&mut p, 114 * 144 - 1) & IRQ_VBLANK, 0);
        assert!(!p.take_frame_ready());
        assert_eq!(ticks(&mut p, 1) & IRQ_VBLANK, IRQ_VBLANK);
        assert_eq!((p.read_reg(LY), mode(&p)), (144, 1));
        assert!(p.take_frame_ready());
        assert!(!p.take_frame_ready());
    }

    #[test]
    fn ly_wraps_after_line_153() {
        let mut p = ppu();
        ticks(&mut p, 114 * 154);
        assert_eq!((p.read_reg(LY), mode(&p)), (0, 2));
    }

    #[test]
    fn lcd_off_resets_ly_and_still_paces_frames() {
        let mut p = ppu();
        ticks(&mut p, 114 * 10);
        p.write_reg(LCDC, 0x11);
        assert_eq!((p.read_reg(LY), mode(&p)), (0, 0));
        assert_eq!(ticks(&mut p, DOTS_PER_FRAME / 4 - 1), 0);
        assert!(!p.take_frame_ready());
        ticks(&mut p, 1);
        assert!(p.take_frame_ready());
    }

    #[test]
    fn ly_is_read_only_and_stat_bit7_reads_one() {
        let mut p = ppu();
        p.write_reg(LY, 99);
        p.write_reg(STAT, 0xFF);
        assert_eq!(p.read_reg(LY), 0);
        assert_eq!(p.read_reg(STAT) & 0xF8, 0xF8);
    }

    #[test]
    fn hblank_stat_interrupt_fires_on_rising_edge_only() {
        let mut p = ppu();
        p.write_reg(STAT, 0x08);
        assert_eq!(ticks(&mut p, 62), 0);
        assert_eq!(ticks(&mut p, 1), IRQ_STAT);
        assert_eq!(ticks(&mut p, 50), 0);
    }

    #[test]
    fn lyc_match_sets_flag_and_requests_stat_interrupt() {
        let mut p = ppu();
        p.write_reg(LYC, 1);
        p.write_reg(STAT, 0x40);
        assert_eq!(p.read_reg(STAT) & 0x04, 0);
        assert_eq!(ticks(&mut p, 114), IRQ_STAT);
        assert_eq!(p.read_reg(STAT) & 0x04, 0x04);
    }

    #[test]
    fn writing_lyc_to_current_line_requests_stat_interrupt() {
        let mut p = ppu();
        p.write_reg(LYC, 5);
        p.write_reg(STAT, 0x40);
        assert_eq!(p.write_reg(LYC, 0), IRQ_STAT);
    }

    #[test]
    fn line_144_also_raises_mode2_stat_interrupt() {
        let mut p = ppu();
        p.write_reg(STAT, 0x20);
        ticks(&mut p, 114 * 144 - 1);
        assert_eq!(ticks(&mut p, 1), IRQ_VBLANK | IRQ_STAT);
    }

    #[test]
    fn lyc_flag_is_kept_while_lcd_off() {
        let mut p = ppu();
        assert_eq!(p.read_reg(STAT) & 0x04, 0x04);
        p.write_reg(LCDC, 0x11);
        p.write_reg(LYC, 5);
        assert_eq!(p.read_reg(STAT) & 0x04, 0x04);
        p.write_reg(LCDC, 0x91);
        assert_eq!(p.read_reg(STAT) & 0x04, 0x00);
    }

    #[test]
    fn vram_and_oam_store_bytes() {
        let mut p = Ppu::default();
        p.write_vram(0x8000, 1);
        p.write_vram(0x9FFF, 2);
        p.write_oam(0xFE00, 3);
        p.write_oam(0xFE9F, 4);
        assert_eq!(
            (
                p.read_vram(0x8000),
                p.read_vram(0x9FFF),
                p.read_oam(0xFE00),
                p.read_oam(0xFE9F)
            ),
            (1, 2, 3, 4)
        );
    }
}
```

`tests/mooneye.rs` 목록 끝에 추가한다:

```rust
    ppu_intr_1_2_timing => "acceptance/ppu/intr_1_2_timing-GS.gb",
    ppu_intr_2_0_timing => "acceptance/ppu/intr_2_0_timing.gb",
    ppu_intr_2_mode0_timing => "acceptance/ppu/intr_2_mode0_timing.gb",
    ppu_intr_2_mode3_timing => "acceptance/ppu/intr_2_mode3_timing.gb",
    ppu_stat_irq_blocking => "acceptance/ppu/stat_irq_blocking.gb",
    ppu_vblank_stat_intr => "acceptance/ppu/vblank_stat_intr-GS.gb",
```

- [ ] **Step 2: 테스트가 실패하는지 확인한다**

Run: `cargo test -p gb-core --lib ppu:: 2>&1 | grep -E "^error" | head -5`
Expected: FAIL(컴파일 오류). `STAT`, `BGP`, `IRQ_STAT`, `IRQ_VBLANK` 등이 아직 없고, `tick`이 `bool`을 반환하기 때문이다.

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test -p gb-core --test mooneye 2>&1 | grep "test result"`
Expected: `71 passed; 6 failed`(PPU 6개)

- [ ] **Step 3: 구현한다**

`ppu.rs`에서 `#[cfg(test)]` 앞까지(모듈 문서 포함)를 아래로 교체한다:

```rust
//! PPU (Pan Docs "Rendering", "LCD Control", "LCD Status", "Palettes", "OAM").
//!
//! 한 줄은 456 dot이다. 0–143번 줄은 모드 2(0–79) → 모드 3(80–251, 고정) → 모드 0(252–455),
//! 144–153번 줄은 모드 1이다. 모드 3에 들어갈 때 그 줄 전체를 한 번에 그린다(스캔라인 렌더러).

pub const LCDC: u16 = 0xFF40;
pub const STAT: u16 = 0xFF41;
pub const SCY: u16 = 0xFF42;
pub const SCX: u16 = 0xFF43;
pub const LY: u16 = 0xFF44;
pub const LYC: u16 = 0xFF45;
pub const BGP: u16 = 0xFF47;
pub const OBP0: u16 = 0xFF48;
pub const OBP1: u16 = 0xFF49;
pub const WY: u16 = 0xFF4A;
pub const WX: u16 = 0xFF4B;

pub const DOTS_PER_LINE: u32 = 456;
pub const LINES_PER_FRAME: u8 = 154;
pub const DOTS_PER_FRAME: u32 = DOTS_PER_LINE * LINES_PER_FRAME as u32;

/// PPU가 요청하는 인터럽트 (IF 비트와 같은 값).
pub const IRQ_VBLANK: u8 = 0x01;
pub const IRQ_STAT: u8 = 0x02;

const VBLANK_LINE: u8 = 144;
const MODE3_START: u32 = 80;
const MODE0_START: u32 = 252;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    HBlank = 0,
    VBlank = 1,
    OamScan = 2,
    Drawing = 3,
}

#[derive(Debug, Clone)]
pub struct Ppu {
    vram: Box<[u8; 0x2000]>,
    oam: Box<[u8; 0xA0]>,
    lcdc: u8,
    /// STAT의 쓰기 가능한 비트(3–6). 모드와 LYC 일치 비트는 읽을 때 만든다.
    stat: u8,
    scy: u8,
    scx: u8,
    ly: u8,
    lyc: u8,
    bgp: u8,
    obp0: u8,
    obp1: u8,
    wy: u8,
    wx: u8,
    mode: Mode,
    /// 현재 줄(LCD가 꺼져 있으면 현재 프레임)에서 지난 dot 수.
    dot: u32,
    /// LY == LYC 비교 결과(STAT 비트 2). LCD가 켜져 있을 때만 갱신되고, 꺼지면 마지막 값을 유지한다.
    lyc_match: bool,
    /// STAT 인터럽트 신호. 0→1로 바뀔 때만 인터럽트를 요청한다.
    stat_line: bool,
    frame_ready: bool,
}

impl Default for Ppu {
    fn default() -> Self {
        Self {
            vram: Box::new([0; 0x2000]),
            oam: Box::new([0; 0xA0]),
            lcdc: 0x91,
            stat: 0,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wy: 0,
            wx: 0,
            mode: Mode::OamScan,
            dot: 0,
            lyc_match: true,
            stat_line: false,
            frame_ready: false,
        }
    }
}

impl Ppu {
    pub fn read_vram(&self, addr: u16) -> u8 {
        self.vram[usize::from(addr & 0x1FFF)]
    }

    pub fn write_vram(&mut self, addr: u16, value: u8) {
        self.vram[usize::from(addr & 0x1FFF)] = value;
    }

    /// `addr`는 0xFE00–0xFE9F (버스가 보장한다).
    pub fn read_oam(&self, addr: u16) -> u8 {
        self.oam[usize::from(addr - 0xFE00)]
    }

    pub fn write_oam(&mut self, addr: u16, value: u8) {
        self.oam[usize::from(addr - 0xFE00)] = value;
    }

    pub fn read_reg(&self, addr: u16) -> u8 {
        match addr {
            LCDC => self.lcdc,
            STAT => {
                let mode = if self.lcd_on() { self.mode as u8 } else { 0 };
                0x80 | self.stat | (u8::from(self.lyc_match) << 2) | mode
            }
            SCY => self.scy,
            SCX => self.scx,
            LY => self.ly,
            LYC => self.lyc,
            BGP => self.bgp,
            OBP0 => self.obp0,
            OBP1 => self.obp1,
            WY => self.wy,
            WX => self.wx,
            _ => 0xFF,
        }
    }

    /// 레지스터 쓰기. STAT 신호가 새로 켜지면 STAT 인터럽트 비트를 반환한다.
    pub fn write_reg(&mut self, addr: u16, value: u8) -> u8 {
        match addr {
            LCDC => {
                let was_on = self.lcd_on();
                self.lcdc = value;
                if was_on && !self.lcd_on() {
                    self.ly = 0;
                    self.dot = 0;
                    self.mode = Mode::HBlank;
                } else if !was_on && self.lcd_on() {
                    self.ly = 0;
                    self.dot = 0;
                    self.mode = Mode::OamScan;
                }
            }
            STAT => self.stat = value & 0x78,
            SCY => self.scy = value,
            SCX => self.scx = value,
            LYC => self.lyc = value,
            BGP => self.bgp = value,
            OBP0 => self.obp0 = value,
            OBP1 => self.obp1 = value,
            WY => self.wy = value,
            WX => self.wx = value,
            // LY는 읽기 전용이다.
            _ => {}
        }
        if self.lcd_on() {
            self.lyc_match = self.ly == self.lyc;
        }
        self.update_stat_line()
    }

    fn lcd_on(&self) -> bool {
        self.lcdc & 0x80 != 0
    }

    /// STAT 신호를 다시 계산해 0→1로 바뀌었으면 `IRQ_STAT`을 반환한다.
    fn update_stat_line(&mut self) -> u8 {
        // DMG는 144번 줄이 시작될 때 모드 2 조건도 한 번 켠다.
        let oam_condition = self.mode == Mode::OamScan || (self.ly == VBLANK_LINE && self.dot == 0);
        let line = (self.stat & 0x40 != 0 && self.lyc_match)
            || (self.lcd_on()
                && ((self.stat & 0x08 != 0 && self.mode == Mode::HBlank)
                    || (self.stat & 0x10 != 0 && self.mode == Mode::VBlank)
                    || (self.stat & 0x20 != 0 && oam_condition)));
        let rising = line && !self.stat_line;
        self.stat_line = line;
        if rising { IRQ_STAT } else { 0 }
    }

    /// `dots` T-사이클(4의 배수) 진행한다. 요청할 인터럽트 비트(`IRQ_VBLANK`, `IRQ_STAT`)를 반환한다.
    pub fn tick(&mut self, dots: u32) -> u8 {
        if !self.lcd_on() {
            self.dot += dots;
            if self.dot >= DOTS_PER_FRAME {
                self.dot -= DOTS_PER_FRAME;
                self.frame_ready = true;
            }
            return 0;
        }
        let mut irq = 0;
        for _ in 0..dots / 4 {
            irq |= self.step4();
        }
        irq
    }

    /// 4 dot 진행한다. 모드 경계(80, 252, 456)는 모두 4의 배수다.
    fn step4(&mut self) -> u8 {
        self.dot += 4;
        let mut irq = 0;
        if self.ly < VBLANK_LINE {
            match self.dot {
                MODE3_START => self.mode = Mode::Drawing,
                MODE0_START => self.mode = Mode::HBlank,
                _ => {}
            }
        }
        if self.dot == DOTS_PER_LINE {
            self.dot = 0;
            self.ly = (self.ly + 1) % LINES_PER_FRAME;
            self.lyc_match = self.ly == self.lyc;
            if self.ly == VBLANK_LINE {
                self.mode = Mode::VBlank;
                self.frame_ready = true;
                irq |= IRQ_VBLANK;
            } else if self.ly < VBLANK_LINE {
                self.mode = Mode::OamScan;
            }
        }
        irq | self.update_stat_line()
    }

    /// 프레임 경계를 지났으면 `true`. 읽으면 초기화된다.
    pub fn take_frame_ready(&mut self) -> bool {
        std::mem::take(&mut self.frame_ready)
    }
}
```

`bus.rs`의 `read_io`에서 `ppu::LCDC | ppu::LY => self.ppu.read_reg(addr),`를 아래로 바꾼다:

```rust
            ppu::LCDC..=ppu::LYC | ppu::BGP..=ppu::WX => self.ppu.read_reg(addr),
```

`write_io`의 `ppu::LCDC | ppu::LY => self.ppu.write_reg(addr, value),`를 아래로 바꾼다:

```rust
            ppu::LCDC..=ppu::LYC | ppu::BGP..=ppu::WX => {
                let irq = self.ppu.write_reg(addr, value);
                self.if_ |= irq;
            }
```

`CpuBus::tick`의 마지막 두 줄(`let vblank_irq = self.ppu.tick(4);`, `self.request(INT_VBLANK, vblank_irq);`)을 아래로 바꾼다:

```rust
        self.if_ |= self.ppu.tick(4);
```

- [ ] **Step 4: 테스트가 통과하는지 확인한다**

Run: `cargo fmt --all && cargo test -p gb-core --lib 2>&1 | grep -E "FAILED|test result" && ARAGORN_REQUIRE_TEST_ROMS=1 cargo test -p gb-core --no-fail-fast --test mooneye --test blargg 2>&1 | grep -E "FAILED|test result" && cargo clippy -p gb-core --all-targets -- -D warnings`
Expected: lib `134 passed`, mooneye `77 passed`, blargg `15 passed`. clippy 출력 없음.

- [ ] **Step 5: 커밋한다**

```bash
git add crates/gb-core
git commit -m "feat(core): add PPU modes, STAT interrupts and LYC comparison

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: 스캔라인 렌더러와 dmg-acid2

**Files:**
- Modify: `crates/gb-core/src/ppu.rs`, `crates/gb-core/src/bus.rs`, `crates/gb-core/src/gameboy.rs`, `crates/gb-core/Cargo.toml`, `scripts/fetch-test-roms.sh`
- Create: `crates/gb-core/tests/acid2.rs`

**Interfaces:**
- Consumes: Task 2 `Ppu`
- Produces:
  - `gb_core::ppu::{SCREEN_WIDTH = 160, SCREEN_HEIGHT = 144, DEFAULT_DMG_PALETTE: [u32; 4]}`
  - `Ppu::framebuffer(&self) -> &[u32]`, `Ppu::set_palette([u32; 4])`
  - `Bus::ppu(&self) -> &Ppu`
  - `GameBoy::framebuffer(&self) -> &[u32]`: 160×144이고, 각 픽셀은 0xRRGGBBAA다.

- [ ] **Step 1: 실패하는 테스트를 작성한다**

`ppu.rs` tests 모듈의 `fn mode(ppu: &Ppu) -> u8 { ... }` 다음에 도우미를 추가한다:

```rust
    /// (x, y) 픽셀의 음영 번호 (0 = 가장 밝음).
    fn shade(ppu: &Ppu, x: usize, y: usize) -> usize {
        let pixel = ppu.framebuffer()[y * SCREEN_WIDTH + x];
        DEFAULT_DMG_PALETTE
            .iter()
            .position(|&c| c == pixel)
            .unwrap()
    }

    /// 타일 `index`(0x8000 기준)의 모든 줄을 색 3으로 채운다.
    fn solid_tile(ppu: &mut Ppu, index: u16) {
        for i in 0..16 {
            ppu.write_vram(0x8000 + index * 16 + i, 0xFF);
        }
    }
```

같은 모듈의 `vram_and_oam_store_bytes` 테스트 위에 추가한다:

```rust
    #[test]
    fn background_tile_uses_bgp() {
        let mut p = ppu();
        solid_tile(&mut p, 1);
        p.write_vram(0x9800, 1);
        ticks(&mut p, 20);
        assert_eq!(
            (shade(&p, 0, 0), shade(&p, 7, 0), shade(&p, 8, 0)),
            (3, 3, 0)
        );
        p.write_reg(BGP, 0x00);
        ticks(&mut p, 114);
        assert_eq!(shade(&p, 0, 1), 0);
    }

    #[test]
    fn signed_tile_addressing_uses_0x9000_base() {
        let mut p = ppu();
        p.write_reg(LCDC, 0x81);
        for i in 0..16 {
            p.write_vram(0x8800 + i, 0xFF);
        }
        p.write_vram(0x9800, 0x80);
        ticks(&mut p, 20);
        assert_eq!(shade(&p, 0, 0), 3);
    }

    #[test]
    fn scx_scrolls_background() {
        let mut p = ppu();
        solid_tile(&mut p, 1);
        p.write_vram(0x9800, 1);
        p.write_reg(SCX, 4);
        ticks(&mut p, 20);
        assert_eq!((shade(&p, 3, 0), shade(&p, 4, 0)), (3, 0));
    }

    #[test]
    fn window_starts_at_wx_minus_7() {
        let mut p = ppu();
        solid_tile(&mut p, 1);
        for i in 0..32 {
            p.write_vram(0x9800 + i, 1);
        }
        // BG는 0x9C00 맵(타일 0), 윈도우는 0x9800 맵(타일 1).
        p.write_reg(LCDC, 0x91 | 0x08 | 0x20);
        p.write_reg(WY, 0);
        p.write_reg(WX, 7 + 80);
        ticks(&mut p, 20);
        assert_eq!((shade(&p, 79, 0), shade(&p, 80, 0)), (0, 3));
    }

    #[test]
    fn sprite_draws_over_background_unless_bg_priority() {
        let mut p = ppu();
        p.write_reg(LCDC, 0x93);
        solid_tile(&mut p, 1);
        p.write_oam(0xFE00, 16);
        p.write_oam(0xFE01, 8);
        p.write_oam(0xFE02, 1);
        ticks(&mut p, 20);
        assert_eq!(shade(&p, 0, 0), 3);
        // BG 우선 속성이면 BG 색 0 위에만 그린다. 여기서 BG는 색 3(BGP로 음영 1)이라 BG가 보인다.
        p.write_oam(0xFE03, 0x80);
        p.write_vram(0x9800, 1);
        p.write_reg(BGP, 0x40);
        ticks(&mut p, 114);
        assert_eq!(shade(&p, 0, 1), 1);
    }

    #[test]
    fn lower_x_sprite_wins_on_dmg() {
        let mut p = ppu();
        p.write_reg(LCDC, 0x93);
        solid_tile(&mut p, 1);
        p.write_reg(OBP0, 0xC0);
        p.write_reg(OBP1, 0x40);
        // OAM 0: x=12, OBP0(색3→음영3). OAM 1: x=8, OBP1(색3→음영1). 겹치는 x=4..8은 x가 작은 OAM 1.
        for (i, (x, attr)) in [(12u8, 0x00u8), (8, 0x10)].into_iter().enumerate() {
            let base = 0xFE00 + 4 * i as u16;
            p.write_oam(base, 16);
            p.write_oam(base + 1, x);
            p.write_oam(base + 2, 1);
            p.write_oam(base + 3, attr);
        }
        ticks(&mut p, 20);
        assert_eq!((shade(&p, 4, 0), shade(&p, 8, 0)), (1, 3));
    }

    #[test]
    fn only_ten_sprites_per_line() {
        let mut p = ppu();
        p.write_reg(LCDC, 0x93);
        solid_tile(&mut p, 1);
        for i in 0..11u16 {
            p.write_oam(0xFE00 + 4 * i, 16);
            p.write_oam(0xFE01 + 4 * i, 8 + 8 * i as u8);
            p.write_oam(0xFE02 + 4 * i, 1);
        }
        ticks(&mut p, 20);
        assert_eq!((shade(&p, 72, 0), shade(&p, 80, 0)), (3, 0));
    }
```

`crates/gb-core/Cargo.toml` 끝에 추가한다:

```toml

[dev-dependencies]
png = "0.17"
```

`crates/gb-core/tests/acid2.rs`:

```rust
//! dmg-acid2 인수 테스트 (스펙 §8-3): 프레임버퍼를 기준 스크린샷과 픽셀 단위로 비교한다.

mod common;

use gb_core::{GameBoy, Model, ppu::DEFAULT_DMG_PALETTE};

const WIDTH: usize = 160;
const HEIGHT: usize = 144;

/// 기준 PNG(2비트 그레이스케일, 3 = 흰색)를 음영 번호(0 = 가장 밝음)로 바꾼다.
fn reference_shades(png_bytes: &[u8]) -> Vec<usize> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder.read_info().expect("PNG 헤더");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("PNG 본문");
    assert_eq!((info.width, info.height), (WIDTH as u32, HEIGHT as u32));
    assert_eq!(
        (info.color_type, info.bit_depth),
        (png::ColorType::Grayscale, png::BitDepth::Two)
    );
    (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .map(|(x, y)| {
            let byte = buf[y * info.line_size + x / 4];
            3 - usize::from((byte >> (6 - 2 * (x % 4))) & 0x03)
        })
        .collect()
}

#[test]
fn dmg_acid2_matches_reference() {
    let (Some(rom), Some(reference)) = (
        common::load_rom("dmg-acid2/dmg-acid2.gb"),
        common::load_rom("dmg-acid2/reference-dmg.png"),
    ) else {
        return;
    };
    let mut gb = GameBoy::new(rom, Model::Dmg).expect("테스트 ROM 로드");
    for _ in 0..60 {
        gb.run_frame();
    }
    let expected = reference_shades(&reference);
    let wrong: Vec<(usize, usize)> = gb
        .framebuffer()
        .iter()
        .zip(&expected)
        .enumerate()
        .filter(|(_, (pixel, shade))| DEFAULT_DMG_PALETTE[**shade] != **pixel)
        .map(|(i, _)| (i % WIDTH, i / WIDTH))
        .collect();
    assert!(
        wrong.is_empty(),
        "{}개 픽셀이 다릅니다. 처음 몇 개: {:?}",
        wrong.len(),
        &wrong[..wrong.len().min(10)]
    );
}
```

`scripts/fetch-test-roms.sh`에서 `echo "Blargg 테스트 ROM 준비 완료: $DEST"` 줄 다음에 추가한다:

```bash
# dmg-acid2 (MIT): ROM과 기준 스크린샷
ACID2_DEST="$ROOT/tests/roms/dmg-acid2"
ACID2_FILES=(
  "https://github.com/mattcurrie/dmg-acid2/releases/download/v1.0/dmg-acid2.gb|dmg-acid2.gb|464e14b7d42e7feea0b7ede42be7071dc88913f75b9ffa444299424b63d1dff1"
  "https://raw.githubusercontent.com/mattcurrie/dmg-acid2/8a98ce731f96dde032ffb22ec36dc985d78fdb18/img/reference-dmg.png|reference-dmg.png|ca966d50895c7efef05838590d148c2cbfd7fba57dab986f25b35b4da71abb57"
)
mkdir -p "$ACID2_DEST"
for entry in "${ACID2_FILES[@]}"; do
  IFS='|' read -r url name expected <<< "$entry"
  out="$ACID2_DEST/$name"
  if [[ -f "$out" && "$(sha256 "$out")" == "$expected" ]]; then
    continue
  fi
  curl -sfL --retry 3 -o "$out.tmp" "$url"
  actual="$(sha256 "$out.tmp")"
  if [[ "$actual" != "$expected" ]]; then
    rm -f "$out.tmp"
    echo "SHA256 불일치: $name ($actual)" >&2
    exit 1
  fi
  mv "$out.tmp" "$out"
  echo "내려받음: dmg-acid2/$name"
done
echo "dmg-acid2 준비 완료: $ACID2_DEST"
```

- [ ] **Step 2: 테스트가 실패하는지 확인한다**

Run: `bash scripts/fetch-test-roms.sh | tail -3 && cargo test -p gb-core --lib ppu:: 2>&1 | grep -E "^error" | head -3`
Expected:
- 스크립트가 `내려받음: dmg-acid2/...` 두 줄과 `dmg-acid2 준비 완료`를 출력한다.
- 테스트는 FAIL(컴파일 오류)이다. `framebuffer`와 `DEFAULT_DMG_PALETTE`가 아직 없기 때문이다.

- [ ] **Step 3: 구현한다**

`ppu.rs`에서 `#[cfg(test)]` 앞까지를 아래로 교체한다:

```rust
//! PPU (Pan Docs "Rendering", "LCD Control", "LCD Status", "Palettes", "OAM").
//!
//! 한 줄은 456 dot이다. 0–143번 줄은 모드 2(0–79) → 모드 3(80–251, 고정) → 모드 0(252–455),
//! 144–153번 줄은 모드 1이다. 모드 3에 들어갈 때 그 줄 전체를 한 번에 그린다(스캔라인 렌더러).

pub const LCDC: u16 = 0xFF40;
pub const STAT: u16 = 0xFF41;
pub const SCY: u16 = 0xFF42;
pub const SCX: u16 = 0xFF43;
pub const LY: u16 = 0xFF44;
pub const LYC: u16 = 0xFF45;
pub const BGP: u16 = 0xFF47;
pub const OBP0: u16 = 0xFF48;
pub const OBP1: u16 = 0xFF49;
pub const WY: u16 = 0xFF4A;
pub const WX: u16 = 0xFF4B;

pub const SCREEN_WIDTH: usize = 160;
pub const SCREEN_HEIGHT: usize = 144;
pub const DOTS_PER_LINE: u32 = 456;
pub const LINES_PER_FRAME: u8 = 154;
pub const DOTS_PER_FRAME: u32 = DOTS_PER_LINE * LINES_PER_FRAME as u32;

/// PPU가 요청하는 인터럽트 (IF 비트와 같은 값).
pub const IRQ_VBLANK: u8 = 0x01;
pub const IRQ_STAT: u8 = 0x02;

const VBLANK_LINE: u8 = 144;
const MODE3_START: u32 = 80;
const MODE0_START: u32 = 252;

/// DMG 기본 팔레트(밝은 색부터). 각 값은 0xRRGGBBAA.
pub const DEFAULT_DMG_PALETTE: [u32; 4] = [0xE0F8D0FF, 0x88C070FF, 0x346856FF, 0x081820FF];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    HBlank = 0,
    VBlank = 1,
    OamScan = 2,
    Drawing = 3,
}

#[derive(Debug, Clone)]
pub struct Ppu {
    vram: Box<[u8; 0x2000]>,
    oam: Box<[u8; 0xA0]>,
    lcdc: u8,
    /// STAT의 쓰기 가능한 비트(3–6). 모드와 LYC 일치 비트는 읽을 때 만든다.
    stat: u8,
    scy: u8,
    scx: u8,
    ly: u8,
    lyc: u8,
    bgp: u8,
    obp0: u8,
    obp1: u8,
    wy: u8,
    wx: u8,
    mode: Mode,
    /// 현재 줄(LCD가 꺼져 있으면 현재 프레임)에서 지난 dot 수.
    dot: u32,
    /// 이번 프레임에서 LY == WY인 줄을 지났는지 (윈도우 표시 조건).
    wy_triggered: bool,
    /// 윈도우 내부 줄 카운터. 윈도우를 그린 줄에서만 증가한다.
    window_line: u8,
    /// LY == LYC 비교 결과(STAT 비트 2). LCD가 켜져 있을 때만 갱신되고, 꺼지면 마지막 값을 유지한다.
    lyc_match: bool,
    /// STAT 인터럽트 신호. 0→1로 바뀔 때만 인터럽트를 요청한다.
    stat_line: bool,
    frame_ready: bool,
    palette: [u32; 4],
    framebuffer: Box<[u32; SCREEN_WIDTH * SCREEN_HEIGHT]>,
}

impl Default for Ppu {
    fn default() -> Self {
        Self {
            vram: Box::new([0; 0x2000]),
            oam: Box::new([0; 0xA0]),
            lcdc: 0x91,
            stat: 0,
            scy: 0,
            scx: 0,
            ly: 0,
            lyc: 0,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wy: 0,
            wx: 0,
            mode: Mode::OamScan,
            dot: 0,
            wy_triggered: false,
            window_line: 0,
            lyc_match: true,
            stat_line: false,
            frame_ready: false,
            palette: DEFAULT_DMG_PALETTE,
            framebuffer: Box::new([DEFAULT_DMG_PALETTE[0]; SCREEN_WIDTH * SCREEN_HEIGHT]),
        }
    }
}

impl Ppu {
    pub fn read_vram(&self, addr: u16) -> u8 {
        self.vram[usize::from(addr & 0x1FFF)]
    }

    pub fn write_vram(&mut self, addr: u16, value: u8) {
        self.vram[usize::from(addr & 0x1FFF)] = value;
    }

    /// `addr`는 0xFE00–0xFE9F (버스가 보장한다).
    pub fn read_oam(&self, addr: u16) -> u8 {
        self.oam[usize::from(addr - 0xFE00)]
    }

    pub fn write_oam(&mut self, addr: u16, value: u8) {
        self.oam[usize::from(addr - 0xFE00)] = value;
    }

    pub fn read_reg(&self, addr: u16) -> u8 {
        match addr {
            LCDC => self.lcdc,
            STAT => {
                let mode = if self.lcd_on() { self.mode as u8 } else { 0 };
                0x80 | self.stat | (u8::from(self.lyc_match) << 2) | mode
            }
            SCY => self.scy,
            SCX => self.scx,
            LY => self.ly,
            LYC => self.lyc,
            BGP => self.bgp,
            OBP0 => self.obp0,
            OBP1 => self.obp1,
            WY => self.wy,
            WX => self.wx,
            _ => 0xFF,
        }
    }

    /// 레지스터 쓰기. STAT 신호가 새로 켜지면 STAT 인터럽트 비트를 반환한다.
    pub fn write_reg(&mut self, addr: u16, value: u8) -> u8 {
        match addr {
            LCDC => {
                let was_on = self.lcd_on();
                self.lcdc = value;
                if was_on && !self.lcd_on() {
                    self.ly = 0;
                    self.dot = 0;
                    self.mode = Mode::HBlank;
                    self.framebuffer.fill(self.palette[0]);
                } else if !was_on && self.lcd_on() {
                    self.ly = 0;
                    self.dot = 0;
                    self.mode = Mode::OamScan;
                    self.wy_triggered = false;
                    self.window_line = 0;
                }
            }
            STAT => self.stat = value & 0x78,
            SCY => self.scy = value,
            SCX => self.scx = value,
            LYC => self.lyc = value,
            BGP => self.bgp = value,
            OBP0 => self.obp0 = value,
            OBP1 => self.obp1 = value,
            WY => self.wy = value,
            WX => self.wx = value,
            // LY는 읽기 전용이다.
            _ => {}
        }
        if self.lcd_on() {
            self.lyc_match = self.ly == self.lyc;
        }
        self.update_stat_line()
    }

    pub fn set_palette(&mut self, palette: [u32; 4]) {
        self.palette = palette;
    }

    /// 160×144, 각 픽셀은 0xRRGGBBAA.
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer[..]
    }

    fn lcd_on(&self) -> bool {
        self.lcdc & 0x80 != 0
    }

    /// STAT 신호를 다시 계산해 0→1로 바뀌었으면 `IRQ_STAT`을 반환한다.
    fn update_stat_line(&mut self) -> u8 {
        // DMG는 144번 줄이 시작될 때 모드 2 조건도 한 번 켠다.
        let oam_condition = self.mode == Mode::OamScan || (self.ly == VBLANK_LINE && self.dot == 0);
        let line = (self.stat & 0x40 != 0 && self.lyc_match)
            || (self.lcd_on()
                && ((self.stat & 0x08 != 0 && self.mode == Mode::HBlank)
                    || (self.stat & 0x10 != 0 && self.mode == Mode::VBlank)
                    || (self.stat & 0x20 != 0 && oam_condition)));
        let rising = line && !self.stat_line;
        self.stat_line = line;
        if rising { IRQ_STAT } else { 0 }
    }

    /// `dots` T-사이클(4의 배수) 진행한다. 요청할 인터럽트 비트(`IRQ_VBLANK`, `IRQ_STAT`)를 반환한다.
    pub fn tick(&mut self, dots: u32) -> u8 {
        if !self.lcd_on() {
            self.dot += dots;
            if self.dot >= DOTS_PER_FRAME {
                self.dot -= DOTS_PER_FRAME;
                self.frame_ready = true;
            }
            return 0;
        }
        let mut irq = 0;
        for _ in 0..dots / 4 {
            irq |= self.step4();
        }
        irq
    }

    /// 4 dot 진행한다. 모드 경계(80, 252, 456)는 모두 4의 배수다.
    fn step4(&mut self) -> u8 {
        self.dot += 4;
        let mut irq = 0;
        if self.ly < VBLANK_LINE {
            match self.dot {
                MODE3_START => {
                    self.mode = Mode::Drawing;
                    self.render_line();
                }
                MODE0_START => self.mode = Mode::HBlank,
                _ => {}
            }
        }
        if self.dot == DOTS_PER_LINE {
            self.dot = 0;
            self.ly = (self.ly + 1) % LINES_PER_FRAME;
            self.lyc_match = self.ly == self.lyc;
            if self.ly == VBLANK_LINE {
                self.mode = Mode::VBlank;
                self.frame_ready = true;
                irq |= IRQ_VBLANK;
            } else if self.ly < VBLANK_LINE {
                if self.ly == 0 {
                    self.wy_triggered = false;
                    self.window_line = 0;
                }
                self.mode = Mode::OamScan;
            }
        }
        irq | self.update_stat_line()
    }

    /// 프레임 경계를 지났으면 `true`. 읽으면 초기화된다.
    pub fn take_frame_ready(&mut self) -> bool {
        std::mem::take(&mut self.frame_ready)
    }

    fn tile_row(&self, tile_addr: usize, row: usize) -> (u8, u8) {
        let i = (tile_addr + row * 2) & 0x1FFF;
        (self.vram[i], self.vram[i + 1])
    }

    /// BG/윈도우 타일 번호가 가리키는 타일 데이터 주소 (VRAM 기준 오프셋).
    fn bg_tile_addr(&self, index: u8) -> usize {
        if self.lcdc & 0x10 != 0 {
            usize::from(index) * 16
        } else {
            (0x1000 + i32::from(index as i8) * 16) as usize
        }
    }

    fn render_line(&mut self) {
        if self.ly == self.wy {
            self.wy_triggered = true;
        }
        let y = usize::from(self.ly);
        // BG/윈도우의 색 번호(0–3). 스프라이트 우선순위 판정에 쓴다.
        let mut bg_color = [0u8; SCREEN_WIDTH];
        let window_visible = self.lcdc & 0x20 != 0 && self.wy_triggered && self.wx <= 166;
        let mut window_drawn = false;
        if self.lcdc & 0x01 != 0 {
            for (x, color) in bg_color.iter_mut().enumerate() {
                let in_window = window_visible && x + 7 >= usize::from(self.wx);
                let (map, px, py) = if in_window {
                    window_drawn = true;
                    let map = if self.lcdc & 0x40 != 0 {
                        0x1C00
                    } else {
                        0x1800
                    };
                    (
                        map,
                        x + 7 - usize::from(self.wx),
                        usize::from(self.window_line),
                    )
                } else {
                    let map = if self.lcdc & 0x08 != 0 {
                        0x1C00
                    } else {
                        0x1800
                    };
                    (
                        map,
                        (x + usize::from(self.scx)) & 0xFF,
                        (y + usize::from(self.scy)) & 0xFF,
                    )
                };
                let index = self.vram[map + (py / 8) * 32 + px / 8];
                let (lo, hi) = self.tile_row(self.bg_tile_addr(index), py % 8);
                let bit = 7 - (px % 8);
                *color = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
            }
        }
        if window_drawn {
            self.window_line = self.window_line.wrapping_add(1);
        }
        let row = &mut self.framebuffer[y * SCREEN_WIDTH..(y + 1) * SCREEN_WIDTH];
        for (pixel, &color) in row.iter_mut().zip(&bg_color) {
            *pixel = self.palette[usize::from((self.bgp >> (color * 2)) & 3)];
        }
        if self.lcdc & 0x02 != 0 {
            self.render_sprites(y, &bg_color);
        }
    }

    fn render_sprites(&mut self, y: usize, bg_color: &[u8; SCREEN_WIDTH]) {
        let height = if self.lcdc & 0x04 != 0 { 16 } else { 8 };
        // 줄당 최대 10개, OAM 순서대로 고른다.
        let mut sprites: Vec<(usize, &[u8])> = self
            .oam
            .chunks_exact(4)
            .enumerate()
            .filter(|(_, s)| {
                let top = i32::from(s[0]) - 16;
                (top..top + height).contains(&(y as i32))
            })
            .take(10)
            .collect();
        // DMG: X 좌표가 작은 것이 우선이고, 같으면 OAM 앞쪽이 우선이다.
        sprites.sort_by_key(|&(i, s)| (s[1], i));
        for (x, &bg) in bg_color.iter().enumerate() {
            for &(_, s) in &sprites {
                let left = i32::from(s[1]) - 8;
                let col = x as i32 - left;
                if !(0..8).contains(&col) {
                    continue;
                }
                let attr = s[3];
                let mut line = y as i32 - (i32::from(s[0]) - 16);
                if attr & 0x40 != 0 {
                    line = height - 1 - line;
                }
                let tile = if height == 16 { s[2] & 0xFE } else { s[2] };
                let (lo, hi) = self.tile_row(usize::from(tile) * 16, line as usize);
                let bit = if attr & 0x20 != 0 { col } else { 7 - col };
                let color = (((hi >> bit) & 1) << 1) | ((lo >> bit) & 1);
                if color == 0 {
                    continue;
                }
                // 우선순위가 가장 높은 불투명 스프라이트 픽셀만 본다. BG 우선이면 BG 색 1–3이 이긴다.
                if attr & 0x80 == 0 || bg == 0 {
                    let palette = if attr & 0x10 != 0 {
                        self.obp1
                    } else {
                        self.obp0
                    };
                    self.framebuffer[y * SCREEN_WIDTH + x] =
                        self.palette[usize::from((palette >> (color * 2)) & 3)];
                }
                break;
            }
        }
    }
}
```

`bus.rs`의 `pub fn take_frame_ready` 위에 추가한다:

```rust
    pub fn ppu(&self) -> &Ppu {
        &self.ppu
    }

```

`gameboy.rs`의 `pub fn debug(&self)` 위에 추가한다:

```rust
    /// 160×144, 각 픽셀은 0xRRGGBBAA.
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.ppu().framebuffer()
    }

```

- [ ] **Step 4: 테스트가 통과하는지 확인한다**

Run: `cargo fmt --all && cargo test -p gb-core --lib 2>&1 | grep -E "FAILED|test result" && ARAGORN_REQUIRE_TEST_ROMS=1 cargo test -p gb-core --no-fail-fast --test acid2 --test mooneye --test blargg 2>&1 | grep -E "FAILED|test result" && cargo clippy -p gb-core --all-targets -- -D warnings`
Expected: lib `141 passed`, acid2 `1 passed`, mooneye `77 passed`, blargg `15 passed`. clippy 출력 없음.

- [ ] **Step 5: 커밋한다**

```bash
git add crates/gb-core scripts/fetch-test-roms.sh
git commit -m "feat(core): render background, window and sprites; pass dmg-acid2

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: 프레임 속도 제어와 세션

**Files:**
- Create: `crates/aragorn-app/src/pacing.rs`, `crates/aragorn-app/src/session.rs`
- Modify: `crates/aragorn-app/src/lib.rs`

**Interfaces:**
- Consumes: Task 3 `GameBoy::framebuffer`, M1 `GameBoy::new`, `run_frame`, `header()`
- Produces:
  - `aragorn_app::pacing::{FRAME_DURATION, MAX_FRAMES_PER_TICK, FramePacer}`
  - `FramePacer::frames_for(&mut self, elapsed: Duration) -> u32`
  - `aragorn_app::session::Session`
    - `load(Vec<u8>) -> Result<Session, CartError>`
    - `title(&self) -> &str`
    - `advance(&mut self, elapsed: Duration) -> u32`
    - `framebuffer(&self) -> &[u32]`

- [ ] **Step 1: 실패하는 테스트를 작성한다**

`crates/aragorn-app/src/lib.rs`의 `pub mod config;` 아래에 `pub mod pacing;`과 `pub mod session;`을 추가한다.

`crates/aragorn-app/src/pacing.rs`:

```rust
//! 실제 경과 시간을 에뮬레이션 프레임 수로 바꾼다 (스펙 §5.1 속도 제어 계산).

use std::time::Duration;

/// DMG 한 프레임: 70224 T-사이클 / 4194304 Hz ≈ 16.74ms (59.73fps).
pub const FRAME_DURATION: Duration = Duration::from_nanos(16_742_706);

/// 한 번에 따라잡는 최대 프레임 수. 창이 멈췄다 돌아와도 한꺼번에 몰아서 돌리지 않는다.
pub const MAX_FRAMES_PER_TICK: u32 = 3;

#[derive(Debug, Default, Clone)]
pub struct FramePacer {
    /// 아직 프레임으로 바꾸지 못한 경과 시간.
    carry: Duration,
}

impl FramePacer {
    /// `elapsed`만큼 시간이 지났을 때 돌릴 프레임 수. 상한을 넘은 시간은 버린다.
    pub fn frames_for(&mut self, elapsed: Duration) -> u32 {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_frame_duration_runs_one_frame() {
        let mut p = FramePacer::default();
        assert_eq!(p.frames_for(FRAME_DURATION), 1);
        assert_eq!(p.frames_for(Duration::ZERO), 0);
    }

    #[test]
    fn leftover_time_carries_over() {
        let mut p = FramePacer::default();
        assert_eq!(p.frames_for(FRAME_DURATION / 2), 0);
        assert_eq!(
            p.frames_for(FRAME_DURATION / 2 + Duration::from_nanos(1)),
            1
        );
    }

    #[test]
    fn long_pause_is_capped_and_forgotten() {
        let mut p = FramePacer::default();
        assert_eq!(p.frames_for(Duration::from_secs(1)), MAX_FRAMES_PER_TICK);
        assert_eq!(p.frames_for(FRAME_DURATION / 2), 0);
    }
}
```

`crates/aragorn-app/src/session.rs`:

```rust
//! 에뮬레이션 세션: 불러온 ROM 하나와 그 실행 속도 (스펙 §5.1).

use crate::pacing::FramePacer;
use gb_core::{CartError, GameBoy, Model};
use std::time::Duration;

pub struct Session {
    gb: GameBoy,
    pacer: FramePacer,
    title: String,
}

impl Session {
    pub fn load(rom: Vec<u8>) -> Result<Session, CartError> {
        todo!()
    }

    pub fn title(&self) -> &str {
        todo!()
    }

    /// 실제로 `elapsed`가 지났을 때 필요한 만큼 프레임을 돌리고, 돌린 프레임 수를 반환한다.
    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        todo!()
    }

    /// 160×144, 각 픽셀은 0xRRGGBBAA.
    pub fn framebuffer(&self) -> &[u32] {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pacing::FRAME_DURATION;

    /// 제목 "ARAGORN TEST", 0x0100에서 제자리 루프(JR -2)를 도는 32KB ROM.
    fn looping_rom() -> Vec<u8> {
        let mut rom = vec![0; 0x8000];
        rom[0x0100..0x0102].copy_from_slice(&[0x18, 0xFE]);
        rom[0x0134..0x0140].copy_from_slice(b"ARAGORN TEST");
        rom
    }

    #[test]
    fn loads_rom_and_exposes_title() {
        let session = Session::load(looping_rom()).unwrap();
        assert_eq!(session.title(), "ARAGORN TEST");
        assert_eq!(session.framebuffer().len(), 160 * 144);
    }

    #[test]
    fn rejects_invalid_rom() {
        assert_eq!(
            Session::load(vec![0; 16]).err(),
            Some(CartError::TooSmall(16))
        );
    }

    #[test]
    fn advance_runs_frames_for_elapsed_time() {
        let mut session = Session::load(looping_rom()).unwrap();
        assert_eq!(session.advance(FRAME_DURATION * 2), 2);
        assert_eq!(session.advance(Duration::ZERO), 0);
    }
}
```

- [ ] **Step 2: 테스트가 실패하는지 확인한다**

Run: `cargo test -p aragorn-app --lib -- pacing:: session:: 2>&1 | grep -E "test result"`
Expected: FAIL. 6개 테스트가 `not yet implemented`로 패닉한다.

- [ ] **Step 3: 구현한다**

`pacing.rs`에서 `#[cfg(test)]` 앞까지를 아래로 교체한다:

```rust
//! 실제 경과 시간을 에뮬레이션 프레임 수로 바꾼다 (스펙 §5.1 속도 제어 계산).

use std::time::Duration;

/// DMG 한 프레임: 70224 T-사이클 / 4194304 Hz ≈ 16.74ms (59.73fps).
pub const FRAME_DURATION: Duration = Duration::from_nanos(16_742_706);

/// 한 번에 따라잡는 최대 프레임 수. 창이 멈췄다 돌아와도 한꺼번에 몰아서 돌리지 않는다.
pub const MAX_FRAMES_PER_TICK: u32 = 3;

#[derive(Debug, Default, Clone)]
pub struct FramePacer {
    /// 아직 프레임으로 바꾸지 못한 경과 시간.
    carry: Duration,
}

impl FramePacer {
    /// `elapsed`만큼 시간이 지났을 때 돌릴 프레임 수. 상한을 넘은 시간은 버린다.
    pub fn frames_for(&mut self, elapsed: Duration) -> u32 {
        let total = self.carry + elapsed;
        let frames = (total.as_nanos() / FRAME_DURATION.as_nanos()) as u32;
        if frames > MAX_FRAMES_PER_TICK {
            self.carry = Duration::ZERO;
            return MAX_FRAMES_PER_TICK;
        }
        self.carry = total - FRAME_DURATION * frames;
        frames
    }
}
```

`session.rs`에서 `#[cfg(test)]` 앞까지를 아래로 교체한다:

```rust
//! 에뮬레이션 세션: 불러온 ROM 하나와 그 실행 속도 (스펙 §5.1).

use crate::pacing::FramePacer;
use gb_core::{CartError, GameBoy, Model};
use std::time::Duration;

pub struct Session {
    gb: GameBoy,
    pacer: FramePacer,
    title: String,
}

impl Session {
    pub fn load(rom: Vec<u8>) -> Result<Session, CartError> {
        let gb = GameBoy::new(rom, Model::Auto)?;
        let title = gb.header().title.trim().to_string();
        Ok(Session {
            gb,
            pacer: FramePacer::default(),
            title,
        })
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// 실제로 `elapsed`가 지났을 때 필요한 만큼 프레임을 돌리고, 돌린 프레임 수를 반환한다.
    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        let frames = self.pacer.frames_for(elapsed);
        for _ in 0..frames {
            self.gb.run_frame();
        }
        frames
    }

    /// 160×144, 각 픽셀은 0xRRGGBBAA.
    pub fn framebuffer(&self) -> &[u32] {
        self.gb.framebuffer()
    }
}
```

- [ ] **Step 4: 테스트가 통과하는지 확인한다**

Run: `cargo fmt --all && cargo test -p aragorn-app 2>&1 | grep -E "FAILED|test result" | head -1 && cargo clippy -p aragorn-app --all-targets -- -D warnings`
Expected: `54 passed`. clippy 출력 없음.

- [ ] **Step 5: 커밋한다**

```bash
git add crates/aragorn-app
git commit -m "feat(app): add frame pacer and emulation session

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: 데스크톱 화면과 ROM 열기

**Files:**
- Create: `crates/aragorn-desktop/src/ui/screen.rs`
- Modify: `crates/aragorn-desktop/Cargo.toml`, `crates/aragorn-desktop/src/ui/mod.rs`, `crates/aragorn-desktop/src/app.rs`, `crates/aragorn-desktop/src/main.rs`

**Interfaces:**
- Consumes: Task 4 `Session`, Task 3 `gb_core::ppu::{SCREEN_WIDTH, SCREEN_HEIGHT}`
- Produces:
  - `ui::screen::{to_rgba, integer_scale, ScreenView}`
  - `app::load_session(&Path) -> Result<Session, String>`
  - `AppDeps.initial_rom: Option<PathBuf>`

- [ ] **Step 1: 실패하는 테스트를 작성한다**

`crates/aragorn-desktop/Cargo.toml`의 `[dependencies]`에서 `aragorn-app = ...` 줄 아래에 추가한다:

```toml
gb-core = { path = "../gb-core" }
rfd = "0.17.2"
```

`ui/mod.rs`의 `pub mod fonts;` 아래에 `pub mod screen;`을 추가한다.

`crates/aragorn-desktop/src/ui/screen.rs`:

```rust
//! 에뮬레이터 화면: 프레임버퍼를 egui 텍스처로 올려 정수 배율로 그린다.

use eframe::egui;
use gb_core::ppu::{SCREEN_HEIGHT, SCREEN_WIDTH};

/// 0xRRGGBBAA 픽셀을 RGBA 바이트로 바꾼다.
pub fn to_rgba(framebuffer: &[u32]) -> Vec<u8> {
    todo!()
}

/// 주어진 영역에 들어가는 가장 큰 정수 배율 (최소 1).
pub fn integer_scale(available: egui::Vec2) -> f32 {
    todo!()
}

#[derive(Default)]
pub struct ScreenView {
    texture: Option<egui::TextureHandle>,
}

impl ScreenView {
    pub fn update(&mut self, ctx: &egui::Context, framebuffer: &[u32]) {
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [SCREEN_WIDTH, SCREEN_HEIGHT],
            &to_rgba(framebuffer),
        );
        match &mut self.texture {
            Some(texture) => texture.set(image, egui::TextureOptions::NEAREST),
            None => {
                self.texture =
                    Some(ctx.load_texture("screen", image, egui::TextureOptions::NEAREST));
            }
        }
    }

    pub fn show(&self, ui: &mut egui::Ui) {
        let Some(texture) = &self.texture else {
            return;
        };
        let scale = integer_scale(ui.available_size());
        let size = egui::vec2(SCREEN_WIDTH as f32, SCREEN_HEIGHT as f32) * scale;
        ui.centered_and_justified(|ui| {
            ui.add(egui::Image::new((texture.id(), size)));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_become_rgba_bytes() {
        assert_eq!(
            to_rgba(&[0x1122_3344, 0xAABB_CCDD]),
            [0x11, 0x22, 0x33, 0x44, 0xAA, 0xBB, 0xCC, 0xDD]
        );
    }

    #[test]
    fn scale_is_largest_integer_that_fits() {
        assert_eq!(integer_scale(egui::vec2(640.0, 576.0)), 4.0);
        assert_eq!(integer_scale(egui::vec2(500.0, 1000.0)), 3.0);
        assert_eq!(integer_scale(egui::vec2(100.0, 100.0)), 1.0);
    }
}
```

`app.rs`의 `pub struct AragornApp {` 위(`AppDeps` 구조체 다음)에 추가한다:

```rust
/// ROM 파일을 읽어 세션을 만든다. 실패하면 상태 표시줄에 보일 문구를 돌려준다.
pub fn load_session(path: &Path) -> Result<Session, String> {
    todo!()
}
```

`app.rs` 파일 끝에 추가한다:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_rom_file_reports_read_error() {
        let err = load_session(Path::new("/없는/경로/pokemon.gb"))
            .err()
            .unwrap();
        assert!(
            err.starts_with("pokemon.gb을(를) 읽을 수 없습니다"),
            "{err}"
        );
    }

    #[test]
    fn invalid_rom_reports_cartridge_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.gb");
        std::fs::write(&path, [0u8; 16]).unwrap();
        let err = load_session(&path).err().unwrap();
        assert_eq!(
            err,
            "broken.gb을(를) 열 수 없습니다: ROM 파일이 너무 작습니다 (16바이트)"
        );
    }
}
```

`app.rs`의 import를 아래처럼 바꾼다. `aragorn_app` 묶음에 `session::Session,`을 추가하고, `std` 묶음에 `path::{Path, PathBuf},`를 추가한다.

```rust
use aragorn_app::{
    config::{Config, ConfigStore, UpdateConfig},
    session::Session,
    update::{UpdateCommand, UpdateEvent, UpdateFlow, UpdateSource, Updater},
};
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
```

- [ ] **Step 2: 테스트가 실패하는지 확인한다**

Run: `cargo test -p aragorn-desktop --lib -- screen:: app:: 2>&1 | grep -E "test result"`
Expected: FAIL. 4개 테스트가 `not yet implemented`로 패닉한다. `PathBuf` 미사용 경고는 Step 3에서 사라진다.

- [ ] **Step 3: 구현한다**

`screen.rs`에서 `#[cfg(test)]` 앞까지를 아래로 교체한다:

```rust
//! 에뮬레이터 화면: 프레임버퍼를 egui 텍스처로 올려 정수 배율로 그린다.

use eframe::egui;
use gb_core::ppu::{SCREEN_HEIGHT, SCREEN_WIDTH};

/// 0xRRGGBBAA 픽셀을 RGBA 바이트로 바꾼다.
pub fn to_rgba(framebuffer: &[u32]) -> Vec<u8> {
    framebuffer.iter().flat_map(|p| p.to_be_bytes()).collect()
}

/// 주어진 영역에 들어가는 가장 큰 정수 배율 (최소 1).
pub fn integer_scale(available: egui::Vec2) -> f32 {
    let sx = (available.x / SCREEN_WIDTH as f32).floor();
    let sy = (available.y / SCREEN_HEIGHT as f32).floor();
    sx.min(sy).max(1.0)
}

#[derive(Default)]
pub struct ScreenView {
    texture: Option<egui::TextureHandle>,
}

impl ScreenView {
    pub fn update(&mut self, ctx: &egui::Context, framebuffer: &[u32]) {
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [SCREEN_WIDTH, SCREEN_HEIGHT],
            &to_rgba(framebuffer),
        );
        match &mut self.texture {
            Some(texture) => texture.set(image, egui::TextureOptions::NEAREST),
            None => {
                self.texture =
                    Some(ctx.load_texture("screen", image, egui::TextureOptions::NEAREST));
            }
        }
    }

    pub fn show(&self, ui: &mut egui::Ui) {
        let Some(texture) = &self.texture else {
            return;
        };
        let scale = integer_scale(ui.available_size());
        let size = egui::vec2(SCREEN_WIDTH as f32, SCREEN_HEIGHT as f32) * scale;
        ui.centered_and_justified(|ui| {
            ui.add(egui::Image::new((texture.id(), size)));
        });
    }
}
```

`app.rs`의 `load_session` 스텁을 아래로 교체한다:

```rust
/// ROM 파일을 읽어 세션을 만든다. 실패하면 상태 표시줄에 보일 문구를 돌려준다.
pub fn load_session(path: &Path) -> Result<Session, String> {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let rom = std::fs::read(path).map_err(|e| format!("{name}을(를) 읽을 수 없습니다: {e}"))?;
    Session::load(rom).map_err(|e| format!("{name}을(를) 열 수 없습니다: {e}"))
}
```
`AppDeps`의 `pub release_page: Option<String>,` 아래에 필드를 추가한다:

```rust
    /// 실행할 때 명령줄로 받은 ROM 경로
    pub initial_rom: Option<PathBuf>,
```

`use crate::{ ... }`의 `ui::update_view::{self, UpdateUiAction},`를 아래로 바꾼다:

```rust
    ui::{
        screen::ScreenView,
        update_view::{self, UpdateUiAction},
    },
```

`AragornApp` 구조체의 `exit_handled: bool,` 아래에 필드를 추가한다:

```rust
    session: Option<Session>,
    screen: ScreenView,
    last_tick: Instant,
    /// ROM 로드 결과 등 상태 표시줄에 보일 문구
    notice: Option<String>,
```

`AragornApp::new`에서 `exit_handled: false,` 아래에 초기값을 추가한다:

```rust
            session: None,
            screen: ScreenView::default(),
            last_tick: Instant::now(),
            notice: None,
```

같은 함수의 `app` 반환 직전(`if app.worker.is_some() { ... }` 다음)에 추가한다:

```rust
        if let Some(path) = deps.initial_rom {
            app.open_rom(ctx, &path);
        }
```

`impl AragornApp`의 `fn dispatch` 위에 추가한다:

```rust
    fn open_rom(&mut self, ctx: &egui::Context, path: &Path) {
        match load_session(path) {
            Ok(session) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                    "Aragorn v{} - {}",
                    self.flow.current(),
                    session.title()
                )));
                self.screen.update(ctx, session.framebuffer());
                self.notice = Some(format!("{} 실행 중", session.title()));
                self.session = Some(session);
                self.last_tick = Instant::now();
            }
            Err(message) => {
                log::warn!("{message}");
                self.notice = Some(message);
            }
        }
    }

    /// 경과 시간만큼 에뮬레이션을 진행하고 화면을 갱신한다. 강제 업데이트 중에는 멈춘다.
    fn run_emulation(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let elapsed = now - self.last_tick;
        self.last_tick = now;
        let Some(session) = &mut self.session else {
            return;
        };
        if self.flow.blocks_emulation() {
            return;
        }
        if session.advance(elapsed) > 0 {
            self.screen.update(ctx, session.framebuffer());
        }
        ctx.request_repaint();
    }
```

`logic()`에서 `for event in events { self.dispatch(event); }` 다음(`// 6시간 재확인 타이머가` 주석 위)에 추가한다:

```rust
        let dropped = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(path) = dropped {
            self.open_rom(ctx, &path);
        }
        self.run_emulation(ctx);
```

`impl eframe::App for AragornApp`의 `fn ui`부터 그 impl 블록을 닫는 `}`까지를 아래로 교체한다(아래 코드의 마지막 `}`가 impl 블록을 닫는다):

```rust
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut events = Vec::new();

        let mut open_requested = false;
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("ROM 열기").clicked() {
                    open_requested = true;
                }
                ui.separator();
                let mut auto_download = self.flow.prefs().auto_download;
                if ui
                    .checkbox(&mut auto_download, "자동으로 업데이트 다운로드")
                    .changed()
                {
                    events.push(UpdateEvent::SetAutoDownload(auto_download));
                }
                if self.worker.is_some() && ui.button("업데이트 확인").clicked() {
                    events.push(UpdateEvent::CheckRequested);
                }
            });
        });

        for action in update_view::show(ui, &self.flow, self.release_page.as_deref()) {
            match action {
                UpdateUiAction::Event(event) => events.push(event),
                UpdateUiAction::Quit => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(notice) = &self.notice {
                    ui.label(notice);
                    ui.separator();
                }
                ui.label(update_view::status_text(
                    self.flow.state(),
                    self.worker.is_some(),
                ));
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            if self.session.is_some() {
                self.screen.show(ui);
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(120.0);
                    ui.heading(format!("Aragorn v{}", self.flow.current()));
                    ui.label(
                        "ROM 열기 버튼을 누르거나 ROM 파일(.gb, .gbc)을 창에 끌어다 놓으세요.",
                    );
                });
            }
        });

        for event in events {
            self.dispatch(event);
        }
        if open_requested
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Game Boy ROM", &["gb", "gbc"])
                .pick_file()
        {
            let ctx = ui.ctx().clone();
            self.open_rom(&ctx, &path);
        }
    }
}
```

`main.rs`의 `let options = eframe::NativeOptions {` 위에 추가한다:

```rust
    // `aragorn <ROM 경로>`로 실행하면 바로 그 ROM을 연다.
    let initial_rom = std::env::args_os().nth(1).map(PathBuf::from);

```

같은 파일의 `AppDeps { ... }` 초기화에서 `release_page,` 다음 줄에 `initial_rom,`을 추가한다.

- [ ] **Step 4: 테스트가 통과하는지 확인한다**

Run: `cargo fmt --all && cargo test -p aragorn-desktop 2>&1 | grep -E "FAILED|test result" | head -1 && cargo clippy --workspace --all-targets -- -D warnings`
Expected: `31 passed`. clippy 출력 없음.

Run: `cargo build -p aragorn-desktop && timeout 8 ./target/debug/aragorn tests/roms/dmg-acid2/dmg-acid2.gb; echo "exit=$?"`
Expected: `exit=124`(시간 초과로 정상 종료). 패닉 메시지가 없어야 한다.
- WSLg에서는 Wayland 연결 끊김으로 `exit=1`이 가끔 나온다(계획 전 검증 참고).
- 그런 경우 한 번 더 실행해 `124`를 확인한다.

- [ ] **Step 5: 커밋한다**

```bash
git add crates/aragorn-desktop Cargo.lock
git commit -m "feat(desktop): open ROMs and show the emulator screen

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: 스펙 수정, 전체 확인, PR

**Files:**
- Modify: `docs/superpowers/specs/2026-10-07-gameboy-emulator-design.md`

- [ ] **Step 1: 스펙을 고친다**

14번째 줄의 표 행을 아래로 바꾼다:

```markdown
| 포켓몬 레드/블루/그린 | DMG | MBC3 (북미판 레드/블루 기준, 판본에 따라 MBC1) | 배터리 세이브 |
```

`   - mooneye: 종료 시 B,C,D,E,H,L = 3,5,8,13,21,34 확인`을 아래로 바꾼다:

```markdown
   - mooneye: 종료 시 시리얼로 보내는 3,5,8,13,21,34(통과) / 0x42×6(실패) 확인 (레지스터 B,C,D,E,H,L과 같은 값)
```

마일스톤 2·3·4 줄을 아래로 바꾼다:

```markdown
2. 타이머, 인터럽트, HALT, OAM DMA → `instr_timing`, `mem_timing`, mooneye 타이머·인터럽트·DMA 테스트
3. PPU + 화면 출력 + MBC1/3/5 ROM·RAM 뱅크 전환 → `dmg-acid2`, 레드/블루 화면
4. 입력, 배터리 세이브 → 레드/블루/피카츄 플레이
```

- [ ] **Step 2: 전체 검사를 실행한다**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace --no-fail-fast 2>&1 | grep "test result" | grep -v " 0 passed"`
Expected: 모든 결과가 `ok`다.
- gb-core lib: 141
- acid2: 1
- blargg: 15
- mooneye: 77
- app: 54
- desktop: 31
- xtask: 14

- [ ] **Step 3: 커밋한다**

```bash
git add docs/superpowers/specs/2026-10-07-gameboy-emulator-design.md
git commit -m "docs: update spec for MBC types, mooneye judging and milestone scope

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 4: 화면 확인을 요청한다** (사용자 작업)

사용자에게 아래 확인을 요청한다.
- 네이티브 OS에서 `cargo run --release -p aragorn-desktop -- <ROM>`으로 실행하거나, PR 병합 후 릴리스판으로 실행한다.
- dmg-acid2가 웃는 얼굴로 나오는지 확인한다.
- 포켓몬 레드/블루의 인트로와 타이틀이 나오는지 확인한다. 입력은 M4라서 조작은 아직 되지 않는다.
- ROM 열기 버튼, 끌어다 놓기, 잘못된 파일을 열었을 때의 상태 표시줄 문구를 확인한다.

- [ ] **Step 5: PR을 만들고 CI를 확인한다** (외부 공개 작업이므로 사용자 확인 후)

```bash
git push -u origin feat/m3-ppu-display
gh pr create --repo dongjay00/aragorn --base main --title "M3: PPU, 화면 출력, MBC 뱅크 전환" --body-file <작성한 본문>
gh pr checks --watch
```

Expected: 3개 OS의 `test` 작업이 모두 통과한다. Linux CI에서 `rfd`가 시스템 라이브러리 없이 빌드되는지 확인한다(xdg-portal 백엔드).
