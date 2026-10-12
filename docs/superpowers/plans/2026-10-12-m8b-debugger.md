# 마일스톤 8b: 디버거 (VRAM·I/O·채널 음소거) 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:**
- 스펙 §6.3의 나머지 디버거 창을 넣는다.
  - VRAM 창(탭 4개):
    - 타일: 뱅크 0/1, 384개씩
    - 배경·창 맵: 지금 화면 테두리 표시
    - OAM 목록: 스프라이트 미리보기 포함
    - CGB/DMG 팔레트
  - I/O 창:
    - LCD·타이머·인터럽트·사운드 레지스터와 값 풀이
    - 채널 1–4 음소거
- 툴바 "디버거"를 메뉴로 바꿔 CPU·메모리·VRAM·I/O 창을 하나씩 켜고 끈다.
  - M8a에서 미룬 "CPU·메모리 창 열림 상태 공유" 문제가 함께 없어진다.

**Architecture:**
- `gb-core`:
  - `ppu::PaletteKind { Bg, Obj }` (`gb_core::PaletteKind`로 다시 내보냄)
  - `Ppu`에 디버거용 읽기를 더한다.
    - `is_cgb`, `vram_bank_data(bank)`, `oam()`, `palette_ram(kind)`
    - `palette_color(kind, palette, color)`: 화면에 그릴 때와 같은 색. CGB는 팔레트 RAM, DMG는 BGP/OBP0/OBP1과 사용자 DMG 팔레트를 쓴다.
  - `DebugView`가 이것을 `cgb`, `vram`, `oam`, `palette_ram`, `palette_color`로 내보내고 `channel_muted`를 더한다.
    - 지금 `peek`은 CPU가 고른 VRAM 뱅크만 읽고, OAM DMA 중에는 0xFF를 돌려준다. 그래서 따로 둔다.
  - `Apu`:
    - `#[serde(skip)] muted: u8` 비트마스크를 둔다. `output()`에서 음소거한 채널만 합산에서 뺀다. 채널 동작, 레지스터, PCM12/34는 그대로다.
    - `adopt_host`가 음소거를 옮겨서, 스테이트를 불러와도 유지된다.
  - `GameBoy::set_channel_muted(channel, muted)`를 더한다.
- `aragorn-app`:
  - 새 `debug::vram`(순수 함수, 0xRRGGBBAA 픽셀):
    - `tile_row`, `tile_sheet`(16×24 타일 = 128×192)
    - `MapSource::from_lcdc`, `tile_index`(부호 있는 0x8800 방식), `tile_map`(256×256, CGB 속성의 팔레트·뱅크·뒤집기)
    - `outline_viewport`(SCX/SCY 테두리, 감싸 돌기)
    - `Sprite`/`sprites`, `sprite_image`(8×8/8×16, 뒤집기, 색 0 투명)
    - `palette_word`
  - 새 `debug::io`: `GROUPS`(LCD 11개, 타이머·인터럽트 6개, 사운드 21개)와 `describe(addr, value)`
    - 풀이하는 레지스터: LCDC, STAT, BGP/OBP, TAC, IF/IE, NR50, NR51, NR52
  - `Session::set_channel_muted`를 더한다.
- `aragorn-desktop`:
  - `ui::vram_view::VramView`: 탭별로 텍스처를 갱신한다. 이미지는 스크롤 영역 안에 그린다.
    - 테스트하는 순수 함수: `tile_at`(마우스 위치 → 타일 번호), `attributes`(OAM 속성 풀이)
  - `ui::io_view::IoView`: 레지스터 표와 음소거 체크박스. 바꾸면 `DebugCommand::SetChannelMuted`를 돌려준다.
  - `DebuggerView.open`을 `cpu_open`·`memory_open`으로 나눈다.
  - `app.rs`:
    - 툴바 "디버거" 메뉴를 둔다.
    - 음소거 명령은 종료·강제 업데이트 중에도 처리한다. 게임을 진행하지 않고, 체크 상태와 어긋나지 않게 하려는 것이다.
    - ROM을 바꾸면 음소거를 새 세션에 다시 건다.
    - 브레이크포인트에 걸리면 CPU 창을 연다.

**Tech Stack:** Rust 1.99 (edition 2024), 새 의존성 없음

**Spec:** `docs/superpowers/specs/2026-10-07-gameboy-emulator-design.md` (§6.3 디버거 UI, §9의 마일스톤 8)

**작업 브랜치:** `feat/m8b-debugger`

## Global Constraints

- 게임 데이터 때문에 패닉하지 않는다. 디버거가 VRAM·OAM·레지스터를 읽어도 게임 상태는 바뀌지 않는다.
- 음소거는 게임 상태가 아니다. 세이브 스테이트 형식(v1)은 바뀌지 않는다.
- 사용자에게 보이는 문구, 문서, 커밋 메시지는 한국어로 쓴다.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace --no-fail-fast`가 모든 커밋에서 통과해야 한다. 로컬과 CI 모두 Rust 1.99 stable이다.
- 리눅스 로컬에서 데스크톱을 빌드할 때 linuxbrew pkg-config를 쓴다면 `PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig`가 필요하다(`libudev-dev`도 필요하다).
- 커밋 메시지 끝에는 `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`를 붙인다.
- 릴리스(v0.10.0)는 병합 뒤 사용자 확인을 받고 한다.

## 패치 적용 방법

각 태스크의 코드는 diff 블록이다. 블록 내용을 **한 글자도 바꾸지 말고**(빈 컨텍스트 줄의 앞 공백 포함) 파일로 저장한 뒤 `git apply <파일>`로 적용한다.

이 패치들은 재현 브랜치에서 main 위에 Task 1→3 순서로 적용해 확인했다.
- 적용 결과가 스파이크와 바이트 단위로 같다.
- 태스크마다 테스트 패치만 넣으면 컴파일에 실패하고, 구현 패치까지 넣으면 통과한다.
- clippy 경고는 0이다.
- 새 파일은 테스트 패치가 모듈 주석과 테스트만 만들고, 구현 패치가 그 위에 구현을 더한다.
  - 테스트가 없는 새 파일(`io_view.rs`)은 구현 패치가 만든다.
  - 모듈 선언 파일(`lib.rs`, `debug/mod.rs`, `ui/mod.rs`)은 테스트 패치에 통째로 들어 있다.
- 패치를 같은 초 안에 연달아 적용하면 cargo가 바뀐 파일을 놓칠 수 있다. 구현 패치를 적용한 뒤에도 예전 코드 기준의 컴파일 오류가 나면 바뀐 파일을 `touch`한다.

## 계획 전 검증 (스파이크)

- 테스트 결과:

  | 범위 | 결과 |
  |---|---|
  | 단위 테스트 | core 264 (+5), app 120 (+15), desktop 79 (+2), xtask 14 |
  | 인수 테스트 | dmg/cgb-acid2, Blargg 15, dmg_sound 12, cgb_sound 12, mooneye 77, save_state 14 모두 그대로 통과 |

- **실제 게임 확인 (WSLg, 릴리스 빌드):**
  - 포켓몬 금(CGB) 인트로에서 창마다 스크린숏을 찍어 확인했다.
  - 타일 탭: 라프라스·잉어킹·GAME FREAK 타일이 보인다.
  - 배경 맵: 물 장면이 CGB 색으로 보이고, 화면 테두리가 맵 끝에서 감싸 돈다.
  - OAM: 잉어킹 스프라이트 조각 12개가 OBJ 팔레트 1과 가로 뒤집기로 보인다.
  - 팔레트: BG·OBJ 색이 보인다.
  - I/O: LCDC/STAT 풀이가 보인다.
  - 이미지가 창보다 커서 잘리던 것을 스크롤 영역으로 고쳤다.
- WSLg Wayland 연결이 가끔 끊겨(Broken pipe) 앱이 종료되는 일이 있었다. 이 마일스톤과 무관한 환경 문제다.

## 스펙과 다른 결정 (리뷰어 확인용)

1. **음소거는 이번 실행 동안만 유지한다** (사용자 합의).
   - 설정에도 스테이트에도 넣지 않는다.
   - ROM을 바꾸거나 스테이트를 불러와도 유지한다.
2. **타일 탭은 회색 4단계로 그린다** (사용자 합의).
   - 타일마다 정해진 팔레트가 없기 때문이다.
   - 배경 맵과 OAM은 실제 팔레트 색으로 그린다.
3. **I/O 창의 사운드 레지스터는 CPU가 읽는 값(쓰기 전용 비트는 1)을 보인다.**
   - 디버거만을 위한 내부 값 노출은 하지 않는다.
4. **창 맵에는 화면 테두리를 그리지 않는다.** 창은 WX/WY로 화면에 고정되기 때문이다.

## Review Focus

1. **음소거:**
   - 믹서 합산에서만 빠지는지: 채널 상태, NR52, PCM12/34는 그대로인지
   - 스테이트를 불러와도, ROM을 바꿔도 유지되는지
   - 종료·강제 업데이트 중에도 체크 상태와 실제 상태가 어긋나지 않는지
   - → `muted_channel_is_left_out_of_the_mix_only`, `channel_mutes_survive_loading_a_state`, `channel_mutes_reach_the_core`
2. **VRAM 해석 정확성:**
   - 0x8800 방식의 부호 있는 타일 번호
   - CGB 맵 속성(뱅크 1, 뒤집기, 팔레트)
   - 8×16 스프라이트(번호 비트 0 무시, 세로 뒤집기는 16줄 전체)
   - DMG OBP0/OBP1 선택
   - → `signed_tile_numbers_start_at_0x9000`, `cgb_map_attributes_pick_palette_bank_and_flip`, `sprite_images_flip_and_stack_tall_tiles`, `debug_palette_colors_match_rendering`
3. **화면 테두리:** SCX/SCY가 맵 끝을 넘을 때 감싸 도는지 → `viewport_outline_wraps_around_the_map`
4. **성능:** 창이 열려 있는 동안 화면 갱신마다 맵(65536픽셀)과 OAM 텍스처 40개를 다시 만든다. 창이 닫혀 있으면 아무것도 하지 않는지 본다.
5. **창 분리:** CPU와 메모리 창을 따로 닫을 수 있는지, 브레이크포인트에 걸리면 CPU 창만 열리는지 본다.

---
### Task 1: 코어: VRAM·OAM·팔레트 읽기와 채널 음소거

**Files:** `crates/gb-core/src/ppu.rs`, `crates/gb-core/src/apu/mod.rs`, `crates/gb-core/src/bus.rs`, `crates/gb-core/src/gameboy.rs`, `crates/gb-core/src/lib.rs`

**Interfaces:**
- Produces:
  - `gb_core::PaletteKind`
  - `Ppu::{is_cgb, vram_bank_data, oam, palette_ram, palette_color}`
  - `Apu::{set_channel_muted, channel_muted}`, `Bus::apu`
  - `GameBoy::set_channel_muted`
  - `DebugView::{cgb, vram, oam, palette_ram, palette_color, channel_muted}`

- [ ] **Step 1: 테스트 패치를 적용한다**

```bash
git checkout main && git pull --ff-only && git checkout -b feat/m8b-debugger
```

```diff
--- a/crates/gb-core/src/ppu.rs
+++ b/crates/gb-core/src/ppu.rs
@@ -854,6 +854,39 @@
     }
 
     #[test]
+    fn debug_access_reads_both_vram_banks_and_oam() {
+        let mut p = Ppu::new(true);
+        p.write_vram(0x8000, 1);
+        p.write_reg(VBK, 0x01);
+        p.write_vram(0x9FFF, 2);
+        p.write_oam(0xFE9F, 3);
+        assert_eq!(p.vram_bank_data(0).len(), 0x2000);
+        assert_eq!(p.vram_bank_data(0)[0], 1);
+        assert_eq!(p.vram_bank_data(1)[0x1FFF], 2);
+        assert_eq!(p.oam()[0x9F], 3);
+        assert!(p.is_cgb() && !Ppu::new(false).is_cgb());
+    }
+
+    #[test]
+    fn debug_palette_colors_match_rendering() {
+        let mut p = cgb_ppu();
+        set_cgb_color(&mut p, BCPS, BCPD, 7, 3, RED);
+        set_cgb_color(&mut p, OCPS, OCPD, 2, 1, GREEN);
+        assert_eq!(p.palette_color(PaletteKind::Bg, 7, 3), 0xFF00_00FF);
+        assert_eq!(p.palette_color(PaletteKind::Obj, 2, 1), 0x00FF_00FF);
+        assert_eq!(p.palette_ram(PaletteKind::Bg)[7 * 8 + 6], 0x1F);
+
+        let mut d = Ppu::default();
+        d.write_reg(BGP, 0b11_10_01_00);
+        d.write_reg(OBP0, 0b00_00_00_11);
+        d.write_reg(OBP1, 0b00_00_10_00);
+        let shade = DEFAULT_DMG_PALETTE;
+        assert_eq!(d.palette_color(PaletteKind::Bg, 0, 2), shade[2]);
+        assert_eq!(d.palette_color(PaletteKind::Obj, 0, 0), shade[3]);
+        assert_eq!(d.palette_color(PaletteKind::Obj, 1, 1), shade[2]);
+    }
+
+    #[test]
     fn cgb_bg_uses_attribute_palette_bank_and_flip() {
         let mut p = cgb_ppu();
         set_cgb_color(&mut p, BCPS, BCPD, 1, 0, BLACK);
--- a/crates/gb-core/src/apu/mod.rs
+++ b/crates/gb-core/src/apu/mod.rs
@@ -411,6 +411,27 @@
     }
 
     #[test]
+    fn muted_channel_is_left_out_of_the_mix_only() {
+        let mut apu = Apu::new(false);
+        apu.write(0xFF25, 0x22); // 채널 2 → 양쪽
+        apu.write(0xFF16, 0xC0);
+        apu.write(0xFF17, 0xF0);
+        apu.write(0xFF18, 0xFF);
+        apu.write(0xFF19, 0x87);
+        for _ in 0..100 {
+            apu.tick(4);
+        }
+        assert!(apu.output().0 > 0.0);
+        apu.set_channel_muted(1, true);
+        apu.set_channel_muted(7, true); // 범위 밖은 무시
+        assert!(apu.channel_muted(1) && !apu.channel_muted(0) && !apu.channel_muted(7));
+        assert_eq!(apu.output(), (0.0, 0.0));
+        assert_eq!(apu.read(NR52) & 0x02, 0x02, "채널은 계속 켜져 있다");
+        apu.set_channel_muted(1, false);
+        assert!(apu.output().0 > 0.0);
+    }
+
+    #[test]
     fn square_wave_pitch_matches_frequency_register() {
         // 주기 값 1750: 131072 / (2048 - 1750) ≈ 439.8 Hz
         let mut apu = Apu::new(false);
--- a/crates/gb-core/src/gameboy.rs
+++ b/crates/gb-core/src/gameboy.rs
@@ -597,6 +597,37 @@
     }
 
     #[test]
+    fn channel_mutes_survive_loading_a_state() {
+        let mut gb = gb_with_program(&[0x18, 0xFE]);
+        let state = gb.save_state(0);
+        gb.set_channel_muted(2, true);
+        gb.load_state(&state, 0).unwrap();
+        assert!(gb.debug().channel_muted(2));
+        assert!(!gb.debug().channel_muted(0));
+        assert!(!gb_with_program(&[0x18, 0xFE]).debug().channel_muted(2));
+    }
+
+    #[test]
+    fn debug_view_exposes_vram_banks_oam_and_palettes() {
+        let mut rom = test_rom(0x00, 0x00, 0x80);
+        // LD A,1 ; LDH (VBK),A ; LD A,$5A ; LD ($8010),A ; LD A,$77 ; LD ($FE00),A ; JR -2
+        let program = [
+            0x3E, 0x01, 0xE0, 0x4F, 0x3E, 0x5A, 0xEA, 0x10, 0x80, 0x3E, 0x77, 0xEA, 0x00, 0xFE,
+            0x18, 0xFE,
+        ];
+        rom[0x0100..0x0100 + program.len()].copy_from_slice(&program);
+        let mut gb = GameBoy::new(rom, Model::Cgb).unwrap();
+        gb.run_frame();
+        let debug = gb.debug();
+        assert!(debug.cgb());
+        assert_eq!(debug.vram(1)[0x10], 0x5A);
+        assert_eq!(debug.vram(0)[0x10], 0x00);
+        assert_eq!(debug.oam()[0], 0x77);
+        assert_eq!(debug.palette_ram(PaletteKind::Bg).len(), 64);
+        assert_eq!(debug.palette_color(PaletteKind::Bg, 0, 0), 0xFFFF_FFFF);
+    }
+
+    #[test]
     fn pressed_button_is_visible_through_p1() {
         // LD A,0x10 ; LDH (0x00),A (버튼 줄 선택) ; JR -2
         let mut gb = gb_with_program(&[0x3E, 0x10, 0xE0, 0x00, 0x18, 0xFE]);
--- a/crates/gb-core/src/lib.rs
+++ b/crates/gb-core/src/lib.rs
@@ -22,4 +22,5 @@
 pub use gameboy::{DebugView, GameBoy};
 pub use joypad::Button;
 pub use model::Model;
+pub use ppu::PaletteKind;
 pub use state::{RomId, StateError};
```

- [ ] **Step 2: 실패하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace`
Expected: 컴파일 실패, ``unresolved import `ppu::PaletteKind` ``

- [ ] **Step 3: 구현 패치를 적용한다**

```diff
--- a/crates/gb-core/src/ppu.rs
+++ b/crates/gb-core/src/ppu.rs
@@ -248,6 +248,46 @@
 
     pub fn set_palette(&mut self, palette: [u32; 4]) {
         self.palette = palette;
+    }
+
+    /// CGB 모드(색 팔레트, VRAM 뱅크 2개)로 그리는지.
+    pub fn is_cgb(&self) -> bool {
+        self.cgb
+    }
+
+    /// 디버거용: VRAM 뱅크 하나(0x8000–0x9FFF, 0x2000바이트). DMG의 뱅크 1은 쓰이지 않아 0이다.
+    pub fn vram_bank_data(&self, bank: u8) -> &[u8] {
+        let start = usize::from(bank & 1) * 0x2000;
+        &self.vram[start..start + 0x2000]
+    }
+
+    /// 디버거용: OAM 전체. OAM DMA 중에도 실제 내용을 보인다.
+    pub fn oam(&self) -> &[u8; 0xA0] {
+        &self.oam
+    }
+
+    /// 디버거용: CGB 팔레트 RAM (팔레트 8개 × 색 4개 × 2바이트).
+    pub fn palette_ram(&self, kind: PaletteKind) -> &[u8; 64] {
+        match kind {
+            PaletteKind::Bg => &self.bg_palettes,
+            PaletteKind::Obj => &self.obj_palettes,
+        }
+    }
+
+    /// 화면에 그릴 때와 같은 0xRRGGBBAA 색.
+    /// - CGB: 팔레트 RAM의 `palette`(0–7)번 팔레트
+    /// - DMG: BG는 BGP, OBJ는 `palette`가 0이면 OBP0, 1이면 OBP1
+    pub fn palette_color(&self, kind: PaletteKind, palette: u8, color: u8) -> u32 {
+        let color = color & 3;
+        if self.cgb {
+            return cgb_color(self.palette_ram(kind), palette & 7, color);
+        }
+        let reg = match kind {
+            PaletteKind::Bg => self.bgp,
+            PaletteKind::Obj if palette & 1 == 0 => self.obp0,
+            PaletteKind::Obj => self.obp1,
+        };
+        self.palette[usize::from((reg >> (color * 2)) & 3)]
     }
 
     /// 160×144, 각 픽셀은 0xRRGGBBAA.
@@ -506,6 +546,13 @@
     }
 }
 
+/// 배경·창 팔레트인지 스프라이트 팔레트인지.
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub enum PaletteKind {
+    Bg,
+    Obj,
+}
+
 /// CGB 팔레트 RAM의 15비트 색을 0xRRGGBBAA로 바꾼다. 5비트 채널은 (c << 3) | (c >> 2)로 늘린다.
 fn cgb_color(ram: &[u8; 64], palette: u8, color: u8) -> u32 {
     let i = usize::from(palette) * 8 + usize::from(color) * 2;
--- a/crates/gb-core/src/apu/mod.rs
+++ b/crates/gb-core/src/apu/mod.rs
@@ -46,6 +46,9 @@
     nr50: u8,
     nr51: u8,
     mixer: Mixer,
+    /// 디버거의 채널 음소거 (비트 0–3 = 채널 1–4). 게임 상태가 아니라서 스테이트에 넣지 않는다.
+    #[serde(skip)]
+    muted: u8,
 }
 
 impl Default for Apu {
@@ -69,6 +72,7 @@
             nr50: 0x77,
             nr51: 0xF3,
             mixer: Mixer::default(),
+            muted: 0,
         };
         apu.write(0xFF11, 0xBF);
         apu.write(0xFF12, 0xF3);
@@ -296,6 +300,9 @@
         let (mut left, mut right) = (0.0, 0.0);
         for (i, digital) in channels.into_iter().enumerate() {
             let Some(digital) = digital else { continue };
+            if self.muted & (1 << i) != 0 {
+                continue;
+            }
             let analog = f32::from(digital) / 15.0;
             if self.nr51 & (0x10 << i) != 0 {
                 left += analog;
@@ -316,12 +323,29 @@
     /// 스테이트에서 읽은 APU에 호스트 설정을 옮기고, 레지스터 비트 폭을 넘는 값을 감싼다.
     pub(crate) fn adopt_host(&mut self, host: &Apu) {
         self.mixer.adopt_host(&host.mixer);
+        self.muted = host.muted;
         self.fs_step &= 7;
         self.sweep.sanitize();
         self.ch1.sanitize();
         self.ch2.sanitize();
         self.ch3.sanitize();
         self.ch4.sanitize();
+    }
+
+    /// `channel`(0–3 = 채널 1–4)을 믹서에서 뺀다. 채널 자체와 레지스터는 그대로 돈다.
+    pub fn set_channel_muted(&mut self, channel: usize, muted: bool) {
+        if channel < 4 {
+            let bit = 1 << channel;
+            self.muted = if muted {
+                self.muted | bit
+            } else {
+                self.muted & !bit
+            };
+        }
+    }
+
+    pub fn channel_muted(&self, channel: usize) -> bool {
+        channel < 4 && self.muted & (1 << channel) != 0
     }
 
     /// 쌓인 인터리브 스테레오 샘플을 `out` 뒤에 붙인다.
--- a/crates/gb-core/src/bus.rs
+++ b/crates/gb-core/src/bus.rs
@@ -179,6 +179,10 @@
         self.cycles
     }
 
+    pub fn apu(&self) -> &Apu {
+        &self.apu
+    }
+
     pub fn apu_mut(&mut self) -> &mut Apu {
         &mut self.apu
     }
--- a/crates/gb-core/src/gameboy.rs
+++ b/crates/gb-core/src/gameboy.rs
@@ -6,7 +6,7 @@
 use crate::debugger::Debugger;
 use crate::joypad::Button;
 use crate::model::Model;
-use crate::ppu;
+use crate::ppu::{self, PaletteKind};
 use crate::state::{self, RomId, StateError};
 
 #[derive(serde::Serialize, serde::Deserialize)]
@@ -145,6 +145,11 @@
         self.bus.apu_mut().set_sample_rate(rate);
     }
 
+    /// 디버거의 채널 음소거. `channel`은 0–3(채널 1–4)이다. 스테이트를 불러와도 유지한다.
+    pub fn set_channel_muted(&mut self, channel: usize, muted: bool) {
+        self.bus.apu_mut().set_channel_muted(channel, muted);
+    }
+
     /// 쌓인 소리를 인터리브 스테레오 f32(-1.0–1.0)로 `out` 뒤에 붙인다.
     /// 꺼내 가지 않으면 오래된 소리는 버린다(약 1초 분량까지만 남긴다).
     pub fn drain_audio(&mut self, out: &mut Vec<f32>) {
@@ -233,6 +238,35 @@
     /// `addr`의 명령을 해석한다. 메모리를 읽어도 게임 상태는 바뀌지 않는다.
     pub fn disassemble(&self, addr: u16) -> disasm::Instruction {
         disasm::disassemble(addr, |a| self.peek(a))
+    }
+
+    /// CGB 모드로 그리는지 (VRAM 뱅크 2개와 색 팔레트).
+    pub fn cgb(&self) -> bool {
+        self.gb.bus.ppu().is_cgb()
+    }
+
+    /// VRAM 뱅크 하나 (0x8000–0x9FFF에 해당하는 0x2000바이트).
+    pub fn vram(&self, bank: u8) -> &'a [u8] {
+        self.gb.bus.ppu().vram_bank_data(bank)
+    }
+
+    /// OAM 전체 (스프라이트 40개 × 4바이트).
+    pub fn oam(&self) -> &'a [u8; 0xA0] {
+        self.gb.bus.ppu().oam()
+    }
+
+    /// CGB 팔레트 RAM.
+    pub fn palette_ram(&self, kind: PaletteKind) -> &'a [u8; 64] {
+        self.gb.bus.ppu().palette_ram(kind)
+    }
+
+    /// 화면에 그릴 때와 같은 0xRRGGBBAA 색 (`ppu::Ppu::palette_color`).
+    pub fn palette_color(&self, kind: PaletteKind, palette: u8, color: u8) -> u32 {
+        self.gb.bus.ppu().palette_color(kind, palette, color)
+    }
+
+    pub fn channel_muted(&self, channel: usize) -> bool {
+        self.gb.bus.apu().channel_muted(channel)
     }
 }
 
```

- [ ] **Step 4: 통과하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: core lib 264 passed (기존 259 + ppu 2, apu 1, gameboy 2), clippy 경고 0

- [ ] **Step 5: 커밋한다**

```bash
cargo fmt --all --check
git add -A crates
git commit -m "feat(core): 디버거용 VRAM·OAM·팔레트 읽기와 채널 음소거

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---
### Task 2: 앱: VRAM 해석과 I/O 풀이

**Files:** `crates/aragorn-app/src/lib.rs`, `crates/aragorn-app/src/debug/mod.rs`, `crates/aragorn-app/src/debug/vram.rs`, `crates/aragorn-app/src/debug/io.rs`, `crates/aragorn-app/src/session.rs`

**Interfaces:**
- Consumes: Task 1 `GameBoy::set_channel_muted`, `DebugView::channel_muted`
- Produces:
  - `aragorn_app::debug::vram::{tile_row, tile_sheet, MapSource, tile_index, tile_map, outline_viewport, Sprite, sprites, sprite_image, palette_word, GRAY, TRANSPARENT, SHEET_WIDTH, SHEET_HEIGHT, SHEET_COLUMNS, MAP_SIZE}`
  - `aragorn_app::debug::io::{GROUPS, describe}`
  - `Session::set_channel_muted`

- [ ] **Step 1: 테스트 패치를 적용한다**

```diff
--- a/crates/aragorn-app/src/lib.rs
+++ b/crates/aragorn-app/src/lib.rs
@@ -2,6 +2,7 @@
 
 pub mod audio;
 pub mod config;
+pub mod debug;
 pub mod input;
 pub mod pacing;
 pub mod session;
--- /dev/null
+++ b/crates/aragorn-app/src/debug/mod.rs
@@ -0,0 +1,4 @@
+//! 디버거 창이 그릴 내용을 계산하는 순수 함수들 (스펙 §6.3).
+
+pub mod io;
+pub mod vram;
--- /dev/null
+++ b/crates/aragorn-app/src/debug/vram.rs
@@ -0,0 +1,221 @@
+//! VRAM 뷰어: 타일 시트, 배경 맵, 스프라이트(OAM), 팔레트 값. 결과는 0xRRGGBBAA 픽셀이다.
+//!
+//! 입력은 VRAM 뱅크(0x8000–0x9FFF에 해당하는 0x2000바이트)와 색을 정하는 함수라서
+//! 코어 없이 테스트할 수 있다.
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    /// 뱅크 하나 크기의 빈 VRAM에 타일 `index`를 `rows`(2바이트씩 8줄)로 채운다.
+    fn vram_with_tile(index: usize, rows: [[u8; 2]; 8]) -> Vec<u8> {
+        let mut vram = vec![0; 0x2000];
+        for (y, [lo, hi]) in rows.into_iter().enumerate() {
+            vram[index * 16 + y * 2] = lo;
+            vram[index * 16 + y * 2 + 1] = hi;
+        }
+        vram
+    }
+
+    /// 첫 줄만 색 0,1,2,3,0,0,0,0인 타일
+    const STRIPE: [[u8; 2]; 8] = [
+        [0b0101_0000, 0b0011_0000],
+        [0; 2],
+        [0; 2],
+        [0; 2],
+        [0; 2],
+        [0; 2],
+        [0; 2],
+        [0; 2],
+    ];
+
+    #[test]
+    fn tile_rows_combine_low_and_high_bit_planes() {
+        assert_eq!(tile_row(0b0101_0000, 0b0011_0000), [0, 1, 2, 3, 0, 0, 0, 0]);
+        assert_eq!(tile_row(0xFF, 0xFF), [3; 8]);
+    }
+
+    #[test]
+    fn tile_sheet_lays_out_16_tiles_per_row() {
+        let vram = vram_with_tile(17, STRIPE);
+        let sheet = tile_sheet(&vram, GRAY);
+        assert_eq!((SHEET_WIDTH, SHEET_HEIGHT), (128, 192));
+        assert_eq!(sheet.len(), 128 * 192);
+        // 타일 17은 둘째 줄 둘째 칸: (8, 8)
+        let at = |x: usize, y: usize| sheet[y * SHEET_WIDTH + x];
+        assert_eq!([at(8, 8), at(9, 8), at(10, 8), at(11, 8)], GRAY);
+        assert_eq!(at(9, 9), GRAY[0]);
+        assert_eq!(at(1, 0), GRAY[0]);
+    }
+
+    #[test]
+    fn signed_tile_numbers_start_at_0x9000() {
+        assert_eq!(tile_index(0x00, true), 0);
+        assert_eq!(tile_index(0xFF, true), 255);
+        assert_eq!(tile_index(0x00, false), 256);
+        assert_eq!(tile_index(0x7F, false), 383);
+        assert_eq!(tile_index(0x80, false), 128);
+    }
+
+    #[test]
+    fn map_source_follows_lcdc() {
+        let both_high = MapSource {
+            high_map: true,
+            unsigned_tiles: true,
+        };
+        assert_eq!(MapSource::from_lcdc(0x08 | 0x10, false), both_high);
+        assert_eq!(
+            MapSource::from_lcdc(0x08 | 0x10, true),
+            MapSource {
+                high_map: false,
+                ..both_high
+            }
+        );
+        assert_eq!(
+            MapSource::from_lcdc(0x40, true),
+            MapSource {
+                high_map: true,
+                unsigned_tiles: false
+            }
+        );
+    }
+
+    #[test]
+    fn map_follows_tile_numbers_and_addressing_mode() {
+        let mut vram = vram_with_tile(256, STRIPE);
+        vram[0x1800 + 33] = 0x00; // 0x9800 맵 (1, 1) 칸
+        let source = MapSource {
+            high_map: false,
+            unsigned_tiles: false,
+        };
+        let map = tile_map(&vram, None, source, |p, c| u32::from(p) << 8 | u32::from(c));
+        assert_eq!(map.len(), 256 * 256);
+        let row = &map[8 * MAP_SIZE + 8..8 * MAP_SIZE + 12];
+        assert_eq!(row, [0, 1, 2, 3]);
+        let unsigned = tile_map(
+            &vram,
+            None,
+            MapSource {
+                unsigned_tiles: true,
+                ..source
+            },
+            |_, c| u32::from(c),
+        );
+        assert_eq!(unsigned[8 * MAP_SIZE + 9], 0, "타일 0은 비어 있다");
+
+        let mut high = vram.clone();
+        high[0x1C00] = 0x00;
+        high[0x1800] = 0x01; // 0x9800 맵은 빈 타일 257
+        let map = tile_map(
+            &high,
+            None,
+            MapSource {
+                high_map: true,
+                ..source
+            },
+            |_, c| u32::from(c),
+        );
+        assert_eq!(map[1], 1, "0x9C00 맵의 첫 칸");
+    }
+
+    #[test]
+    fn cgb_map_attributes_pick_palette_bank_and_flip() {
+        let bank0 = vec![0; 0x2000];
+        let bank1 = {
+            let mut b = vram_with_tile(0, STRIPE);
+            b[0x1800] = 0x08 | 0x20 | 0x05; // 뱅크 1, 가로 뒤집기, 팔레트 5
+            b[0x1800 + 1] = 0x08 | 0x40; // 뱅크 1, 세로 뒤집기
+            b
+        };
+        let source = MapSource {
+            high_map: false,
+            unsigned_tiles: true,
+        };
+        let map = tile_map(&bank0, Some(&bank1), source, |p, c| {
+            u32::from(p) << 8 | u32::from(c)
+        });
+        assert_eq!(&map[4..8], [0x503, 0x502, 0x501, 0x500]);
+        assert_eq!(
+            map[7 * MAP_SIZE + 8 + 1],
+            1,
+            "세로로 뒤집혀 첫 줄이 맨 아래로 간다"
+        );
+        assert_eq!(map[8 + 1], 0);
+    }
+
+    #[test]
+    fn viewport_outline_wraps_around_the_map() {
+        let mut map = vec![0; MAP_SIZE * MAP_SIZE];
+        outline_viewport(&mut map, 200, 250, 9);
+        let at = |x: usize, y: usize| map[y * MAP_SIZE + x];
+        assert_eq!(at(200, 250), 9);
+        assert_eq!(at((200 + 159) % 256, (250 + 143) % 256), 9);
+        assert_eq!(at(0, 250), 9, "위쪽 테두리가 오른쪽 끝을 넘어 이어진다");
+        assert_eq!(at(201, 251), 0, "안쪽은 그대로다");
+        assert_eq!(
+            map.iter().filter(|&&p| p == 9).count(),
+            2 * 160 + 2 * 144 - 4
+        );
+    }
+
+    #[test]
+    fn sprites_decode_oam_entries() {
+        let mut oam = [0u8; 0xA0];
+        oam[4..8].copy_from_slice(&[16, 8, 0x42, 0x80 | 0x40 | 0x20 | 0x10 | 0x08 | 0x03]);
+        let list = sprites(&oam);
+        assert_eq!(list.len(), 40);
+        let s = list[1];
+        assert_eq!((s.index, s.y, s.x, s.tile), (1, 16, 8, 0x42));
+        assert!(s.flip_x() && s.flip_y() && s.behind_bg());
+        assert_eq!((s.palette(false), s.palette(true)), (1, 3));
+        assert_eq!((s.bank(false), s.bank(true)), (0, 1));
+        assert!(s.on_screen(false));
+        assert!(!list[0].on_screen(false), "좌표 0은 화면 밖이다");
+        let tall_top = Sprite { y: 2, x: 8, ..s };
+        assert!(!tall_top.on_screen(false) && tall_top.on_screen(true));
+    }
+
+    #[test]
+    fn sprite_images_flip_and_stack_tall_tiles() {
+        let mut tiles = vram_with_tile(4, STRIPE);
+        tiles[5 * 16] = 0xFF; // 타일 5 첫 줄: 색 1
+        let color = |c: u8| u32::from(c);
+        let plain = Sprite {
+            index: 0,
+            y: 16,
+            x: 8,
+            tile: 4,
+            attr: 0,
+        };
+        let img = sprite_image(&plain, &tiles, false, color);
+        assert_eq!(img.len(), 64);
+        assert_eq!(&img[..4], [TRANSPARENT, 1, 2, 3]);
+
+        let flipped = Sprite {
+            attr: 0x20,
+            ..plain
+        };
+        assert_eq!(
+            &sprite_image(&flipped, &tiles, false, color)[4..8],
+            [3, 2, 1, TRANSPARENT]
+        );
+
+        let tall = Sprite { tile: 5, ..plain }; // 8×16에서는 번호의 비트 0을 무시한다
+        let img = sprite_image(&tall, &tiles, true, color);
+        assert_eq!(img.len(), 128);
+        assert_eq!(img[1], 1, "위 타일 4");
+        assert_eq!(img[8 * 8], 1, "아래 타일 5");
+        let tall_flipped = Sprite { attr: 0x40, ..tall };
+        let img = sprite_image(&tall_flipped, &tiles, true, color);
+        assert_eq!(img[15 * 8 + 1], 1, "세로 뒤집기는 8×16 전체를 뒤집는다");
+        assert_eq!(img[7 * 8], 1);
+    }
+
+    #[test]
+    fn palette_words_are_little_endian_bgr555() {
+        let mut ram = [0u8; 64];
+        ram[7 * 8 + 3 * 2] = 0x1F;
+        ram[7 * 8 + 3 * 2 + 1] = 0x7C;
+        assert_eq!(palette_word(&ram, 7, 3), 0x7C1F);
+        assert_eq!(palette_word(&ram, 0, 0), 0);
+    }
+}
--- /dev/null
+++ b/crates/aragorn-app/src/debug/io.rs
@@ -0,0 +1,57 @@
+//! I/O 창: 레지스터 목록과 값 풀이.
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn groups_cover_lcd_timer_and_sound_registers() {
+        let all: Vec<u16> = GROUPS
+            .iter()
+            .flat_map(|(_, regs)| regs.iter().map(|r| r.0))
+            .collect();
+        for addr in [0xFF40, 0xFF41, 0xFF04, 0xFF07, 0xFFFF, 0xFF10, 0xFF26] {
+            assert!(all.contains(&addr), "{addr:04X}");
+        }
+        let mut sorted = all.clone();
+        sorted.sort_unstable();
+        sorted.dedup();
+        assert_eq!(sorted.len(), all.len(), "중복이 없다");
+    }
+
+    #[test]
+    fn lcdc_and_stat_are_spelled_out() {
+        assert_eq!(
+            describe(0xFF40, 0x91),
+            "LCD 켜짐, 창 꺼짐 (맵 $9800), 타일 $8000, BG 맵 $9800, OBJ 꺼짐 8×8, BG 켜짐"
+        );
+        assert_eq!(
+            describe(0xFF40, 0x6E),
+            "LCD 꺼짐, 창 켜짐 (맵 $9C00), 타일 $8800, BG 맵 $9C00, OBJ 켜짐 8×16, BG 꺼짐"
+        );
+        assert_eq!(
+            describe(0xFF41, 0xC5),
+            "모드 1 VBlank, LYC=LY 같음, 인터럽트 LYC"
+        );
+        assert_eq!(
+            describe(0xFF41, 0x80),
+            "모드 0 HBlank, LYC=LY 다름, 인터럽트 -"
+        );
+    }
+
+    #[test]
+    fn timer_interrupt_and_palette_registers_are_spelled_out() {
+        assert_eq!(describe(0xFF07, 0x05), "켜짐, 262144 Hz");
+        assert_eq!(describe(0xFF07, 0xF8), "꺼짐, 4096 Hz");
+        assert_eq!(describe(0xFFFF, 0x05), "VBlank 타이머");
+        assert_eq!(describe(0xFF0F, 0xE0), "-");
+        assert_eq!(describe(0xFF47, 0xE4), "색 0–3 → 명암 0 1 2 3");
+    }
+
+    #[test]
+    fn sound_control_registers_are_spelled_out() {
+        assert_eq!(describe(0xFF24, 0x73), "볼륨 왼쪽 7 오른쪽 3");
+        assert_eq!(describe(0xFF25, 0xF3), "왼쪽 1 2 3 4 / 오른쪽 1 2");
+        assert_eq!(describe(0xFF26, 0xF5), "전원 켜짐, 소리 나는 채널 1 3");
+        assert_eq!(describe(0xFF10, 0x80), "");
+    }
+}
--- a/crates/aragorn-app/src/session.rs
+++ b/crates/aragorn-app/src/session.rs
@@ -957,6 +957,15 @@
         assert_eq!(session.breakpoint_hit(), Some(0x6000));
     }
 
+    #[test]
+    fn channel_mutes_reach_the_core() {
+        let (mut session, _) = session_with(nop_rom(), MemoryStore::default());
+        session.set_channel_muted(3, true);
+        assert!(session.debug().channel_muted(3));
+        session.set_channel_muted(3, false);
+        assert!(!session.debug().channel_muted(3));
+    }
+
     /// MBC3+TIMER+RAM+BATTERY(0x10) 판 `saving_rom(true)`.
     fn rtc_saving_rom() -> Vec<u8> {
         let mut rom = saving_rom(true);
```

- [ ] **Step 2: 실패하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace`
Expected: 컴파일 실패, ``cannot find function `describe` in this scope``, ``cannot find struct, variant or union type `MapSource` in this scope``

- [ ] **Step 3: 구현 패치를 적용한다**

```diff
--- a/crates/aragorn-app/src/debug/vram.rs
+++ b/crates/aragorn-app/src/debug/vram.rs
@@ -2,6 +2,224 @@
 //!
 //! 입력은 VRAM 뱅크(0x8000–0x9FFF에 해당하는 0x2000바이트)와 색을 정하는 함수라서
 //! 코어 없이 테스트할 수 있다.
+
+/// 타일 시트는 16×24 타일(뱅크 하나의 384개)이다.
+pub const SHEET_COLUMNS: usize = 16;
+pub const SHEET_WIDTH: usize = SHEET_COLUMNS * 8;
+pub const SHEET_HEIGHT: usize = 384 / SHEET_COLUMNS * 8;
+/// 배경 맵은 32×32 타일이다.
+pub const MAP_SIZE: usize = 256;
+pub const SCREEN_WIDTH: usize = 160;
+pub const SCREEN_HEIGHT: usize = 144;
+/// 스프라이트 미리보기에서 색 0(투명)을 칠할 값.
+pub const TRANSPARENT: u32 = 0x0000_0000;
+/// 타일 시트용 회색 4단계 (색 번호 0이 가장 밝다).
+pub const GRAY: [u32; 4] = [0xFFFF_FFFF, 0xAAAA_AAFF, 0x5555_55FF, 0x0000_00FF];
+
+/// 타일 한 줄(2바이트)의 색 번호 8개, 왼쪽부터.
+pub fn tile_row(lo: u8, hi: u8) -> [u8; 8] {
+    std::array::from_fn(|x| {
+        let bit = 7 - x;
+        ((hi >> bit) & 1) << 1 | ((lo >> bit) & 1)
+    })
+}
+
+/// 타일 `index`(0–383)의 `y`번째 줄. `vram`은 뱅크 하나다.
+fn row_of(vram: &[u8], index: usize, y: usize) -> [u8; 8] {
+    let at = index * 16 + y * 2;
+    tile_row(vram[at], vram[at + 1])
+}
+
+/// 뱅크 하나의 타일 384개를 16×24 격자로 그린다 (`SHEET_WIDTH`×`SHEET_HEIGHT`).
+pub fn tile_sheet(vram: &[u8], shades: [u32; 4]) -> Vec<u32> {
+    let mut out = vec![0; SHEET_WIDTH * SHEET_HEIGHT];
+    for index in 0..384 {
+        let (tx, ty) = (index % SHEET_COLUMNS * 8, index / SHEET_COLUMNS * 8);
+        for y in 0..8 {
+            for (x, color) in row_of(vram, index, y).into_iter().enumerate() {
+                out[(ty + y) * SHEET_WIDTH + tx + x] = shades[usize::from(color)];
+            }
+        }
+    }
+    out
+}
+
+/// 배경 맵을 어디서 어떻게 읽을지 (LCDC 비트에서 정한다).
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub struct MapSource {
+    /// `true`면 0x9C00, 아니면 0x9800 맵
+    pub high_map: bool,
+    /// `true`면 0x8000부터 부호 없는 번호, 아니면 0x9000 기준 부호 있는 번호 (LCDC.4)
+    pub unsigned_tiles: bool,
+}
+
+impl MapSource {
+    /// LCDC가 고른 배경 맵(`window`면 창 맵)과 타일 주소 방식.
+    pub fn from_lcdc(lcdc: u8, window: bool) -> Self {
+        let map_bit = if window { 0x40 } else { 0x08 };
+        Self {
+            high_map: lcdc & map_bit != 0,
+            unsigned_tiles: lcdc & 0x10 != 0,
+        }
+    }
+}
+
+/// 맵 타일 번호가 가리키는 타일(뱅크 안 0–383).
+pub fn tile_index(number: u8, unsigned_tiles: bool) -> usize {
+    if unsigned_tiles {
+        usize::from(number)
+    } else {
+        (256 + i16::from(number as i8)) as usize
+    }
+}
+
+/// 32×32 배경 맵 전체를 256×256으로 그린다.
+/// - `bank1`이 있으면(CGB) 맵 속성의 팔레트·뱅크·뒤집기를 따른다.
+/// - `color(palette, color)`는 BG 팔레트 색이다. DMG에서는 팔레트가 늘 0이다.
+pub fn tile_map(
+    bank0: &[u8],
+    bank1: Option<&[u8]>,
+    source: MapSource,
+    color: impl Fn(u8, u8) -> u32,
+) -> Vec<u32> {
+    let base = if source.high_map { 0x1C00 } else { 0x1800 };
+    let mut out = vec![0; MAP_SIZE * MAP_SIZE];
+    for cell in 0..32 * 32 {
+        let number = bank0[base + cell];
+        let attr = bank1.map_or(0, |b| b[base + cell]);
+        let tiles = if attr & 0x08 != 0 {
+            bank1.unwrap_or(bank0)
+        } else {
+            bank0
+        };
+        let index = tile_index(number, source.unsigned_tiles);
+        let (cx, cy) = (cell % 32 * 8, cell / 32 * 8);
+        for y in 0..8 {
+            let src_y = if attr & 0x40 != 0 { 7 - y } else { y };
+            let row = row_of(tiles, index, src_y);
+            for x in 0..8 {
+                let src_x = if attr & 0x20 != 0 { 7 - x } else { x };
+                out[(cy + y) * MAP_SIZE + cx + x] = color(attr & 0x07, row[src_x]);
+            }
+        }
+    }
+    out
+}
+
+/// 맵 위에 지금 화면(160×144, SCX/SCY에서 시작) 테두리를 그린다. 맵 끝에서 감싸 돈다.
+pub fn outline_viewport(map: &mut [u32], scx: u8, scy: u8, color: u32) {
+    let mut put = |x: usize, y: usize| {
+        map[(usize::from(scy) + y) % MAP_SIZE * MAP_SIZE + (usize::from(scx) + x) % MAP_SIZE] =
+            color;
+    };
+    for x in 0..SCREEN_WIDTH {
+        put(x, 0);
+        put(x, SCREEN_HEIGHT - 1);
+    }
+    for y in 0..SCREEN_HEIGHT {
+        put(0, y);
+        put(SCREEN_WIDTH - 1, y);
+    }
+}
+
+/// OAM 항목 하나.
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub struct Sprite {
+    pub index: u8,
+    /// OAM에 적힌 값 그대로 (화면 좌표 + 16, + 8)
+    pub y: u8,
+    pub x: u8,
+    pub tile: u8,
+    pub attr: u8,
+}
+
+impl Sprite {
+    pub fn flip_x(&self) -> bool {
+        self.attr & 0x20 != 0
+    }
+
+    pub fn flip_y(&self) -> bool {
+        self.attr & 0x40 != 0
+    }
+
+    /// BG 색 1–3 뒤에 그려지는지 (속성 비트 7).
+    pub fn behind_bg(&self) -> bool {
+        self.attr & 0x80 != 0
+    }
+
+    /// 쓰는 팔레트: CGB는 0–7, DMG는 OBP0/OBP1의 0/1.
+    pub fn palette(&self, cgb: bool) -> u8 {
+        if cgb {
+            self.attr & 0x07
+        } else {
+            (self.attr >> 4) & 1
+        }
+    }
+
+    /// CGB에서 타일을 읽을 VRAM 뱅크.
+    pub fn bank(&self, cgb: bool) -> u8 {
+        u8::from(cgb && self.attr & 0x08 != 0)
+    }
+
+    /// 화면 안에 조금이라도 보이는지.
+    pub fn on_screen(&self, tall: bool) -> bool {
+        let height = if tall { 16 } else { 8 };
+        self.x > 0 && self.x < 168 && self.y + height > 16 && self.y < 160
+    }
+}
+
+/// OAM 160바이트를 스프라이트 40개로 나눈다.
+pub fn sprites(oam: &[u8; 0xA0]) -> Vec<Sprite> {
+    let (entries, _) = oam.as_chunks::<4>();
+    entries
+        .iter()
+        .enumerate()
+        .map(|(i, &[y, x, tile, attr])| Sprite {
+            index: i as u8,
+            y,
+            x,
+            tile,
+            attr,
+        })
+        .collect()
+}
+
+/// 스프라이트 하나를 8×8(또는 8×16)로 그린다. 색 0은 `TRANSPARENT`다.
+/// `tiles`는 스프라이트가 쓰는 뱅크, `color(color)`는 그 스프라이트 팔레트 색이다.
+pub fn sprite_image(
+    sprite: &Sprite,
+    tiles: &[u8],
+    tall: bool,
+    color: impl Fn(u8) -> u32,
+) -> Vec<u32> {
+    let height = if tall { 16 } else { 8 };
+    let first = if tall {
+        sprite.tile & 0xFE
+    } else {
+        sprite.tile
+    };
+    let mut out = vec![TRANSPARENT; 8 * height];
+    for y in 0..height {
+        let src_y = if sprite.flip_y() { height - 1 - y } else { y };
+        let index = usize::from(first) + src_y / 8;
+        let row = row_of(tiles, index, src_y % 8);
+        for x in 0..8 {
+            let src_x = if sprite.flip_x() { 7 - x } else { x };
+            let c = row[src_x];
+            if c != 0 {
+                out[y * 8 + x] = color(c);
+            }
+        }
+    }
+    out
+}
+
+/// CGB 팔레트 RAM에 적힌 15비트 BGR 값.
+pub fn palette_word(ram: &[u8; 64], palette: u8, color: u8) -> u16 {
+    let i = usize::from(palette & 7) * 8 + usize::from(color & 3) * 2;
+    u16::from_le_bytes([ram[i], ram[i + 1]])
+}
+
 #[cfg(test)]
 mod tests {
     use super::*;
--- a/crates/aragorn-app/src/debug/io.rs
+++ b/crates/aragorn-app/src/debug/io.rs
@@ -1,4 +1,159 @@
 //! I/O 창: 레지스터 목록과 값 풀이.
+
+/// 창에 보일 레지스터 묶음: (묶음 이름, [(주소, 이름)]).
+pub const GROUPS: &[(&str, &[(u16, &str)])] = &[
+    (
+        "LCD",
+        &[
+            (0xFF40, "LCDC"),
+            (0xFF41, "STAT"),
+            (0xFF42, "SCY"),
+            (0xFF43, "SCX"),
+            (0xFF44, "LY"),
+            (0xFF45, "LYC"),
+            (0xFF47, "BGP"),
+            (0xFF48, "OBP0"),
+            (0xFF49, "OBP1"),
+            (0xFF4A, "WY"),
+            (0xFF4B, "WX"),
+        ],
+    ),
+    (
+        "타이머·인터럽트",
+        &[
+            (0xFF04, "DIV"),
+            (0xFF05, "TIMA"),
+            (0xFF06, "TMA"),
+            (0xFF07, "TAC"),
+            (0xFF0F, "IF"),
+            (0xFFFF, "IE"),
+        ],
+    ),
+    (
+        "사운드",
+        &[
+            (0xFF10, "NR10"),
+            (0xFF11, "NR11"),
+            (0xFF12, "NR12"),
+            (0xFF13, "NR13"),
+            (0xFF14, "NR14"),
+            (0xFF16, "NR21"),
+            (0xFF17, "NR22"),
+            (0xFF18, "NR23"),
+            (0xFF19, "NR24"),
+            (0xFF1A, "NR30"),
+            (0xFF1B, "NR31"),
+            (0xFF1C, "NR32"),
+            (0xFF1D, "NR33"),
+            (0xFF1E, "NR34"),
+            (0xFF20, "NR41"),
+            (0xFF21, "NR42"),
+            (0xFF22, "NR43"),
+            (0xFF23, "NR44"),
+            (0xFF24, "NR50"),
+            (0xFF25, "NR51"),
+            (0xFF26, "NR52"),
+        ],
+    ),
+];
+
+fn on_off(value: u8, bit: u8) -> &'static str {
+    if value & (1 << bit) != 0 {
+        "켜짐"
+    } else {
+        "꺼짐"
+    }
+}
+
+/// 켜진 비트의 이름을 공백으로 잇는다. 없으면 `-`.
+fn bit_names(value: u8, names: &[(u8, &str)]) -> String {
+    let set: Vec<&str> = names
+        .iter()
+        .filter(|&&(bit, _)| value & (1 << bit) != 0)
+        .map(|&(_, name)| name)
+        .collect();
+    if set.is_empty() {
+        "-".into()
+    } else {
+        set.join(" ")
+    }
+}
+
+/// 채널 번호 1–4 중 `value`의 비트 `shift`..`shift+4`가 켜진 것.
+fn channels(value: u8, shift: u8) -> String {
+    bit_names(value >> shift, &[(0, "1"), (1, "2"), (2, "3"), (3, "4")])
+}
+
+/// 레지스터 값의 뜻. 풀이할 것이 없는 레지스터는 빈 문자열이다.
+/// 사운드 레지스터는 읽은 값(쓰기 전용 비트는 1)을 풀이한다.
+pub fn describe(addr: u16, value: u8) -> String {
+    match addr {
+        0xFF40 => format!(
+            "LCD {}, 창 {} (맵 {}), 타일 {}, BG 맵 {}, OBJ {} {}, BG {}",
+            on_off(value, 7),
+            on_off(value, 5),
+            if value & 0x40 != 0 { "$9C00" } else { "$9800" },
+            if value & 0x10 != 0 { "$8000" } else { "$8800" },
+            if value & 0x08 != 0 { "$9C00" } else { "$9800" },
+            on_off(value, 1),
+            if value & 0x04 != 0 { "8×16" } else { "8×8" },
+            on_off(value, 0),
+        ),
+        0xFF41 => format!(
+            "모드 {} {}, LYC=LY {}, 인터럽트 {}",
+            value & 3,
+            ["HBlank", "VBlank", "OAM 검색", "그리기"][usize::from(value & 3)],
+            if value & 0x04 != 0 {
+                "같음"
+            } else {
+                "다름"
+            },
+            bit_names(
+                value,
+                &[(6, "LYC"), (5, "OAM"), (4, "VBlank"), (3, "HBlank")]
+            ),
+        ),
+        0xFF47..=0xFF49 => format!(
+            "색 0–3 → 명암 {} {} {} {}",
+            value & 3,
+            (value >> 2) & 3,
+            (value >> 4) & 3,
+            (value >> 6) & 3
+        ),
+        0xFF07 => format!(
+            "{}, {} Hz",
+            if value & 0x04 != 0 {
+                "켜짐"
+            } else {
+                "꺼짐"
+            },
+            ["4096", "262144", "65536", "16384"][usize::from(value & 3)],
+        ),
+        0xFF0F | 0xFFFF => bit_names(
+            value,
+            &[
+                (0, "VBlank"),
+                (1, "STAT"),
+                (2, "타이머"),
+                (3, "시리얼"),
+                (4, "조이패드"),
+            ],
+        ),
+        0xFF24 => format!("볼륨 왼쪽 {} 오른쪽 {}", (value >> 4) & 7, value & 7),
+        0xFF25 => format!(
+            "왼쪽 {} / 오른쪽 {}",
+            channels(value, 4),
+            channels(value, 0)
+        ),
+        0xFF26 => format!(
+            "전원 {}, 소리 나는 채널 {}",
+            on_off(value, 7),
+            channels(value, 0)
+        ),
+        _ => String::new(),
+    }
+}
+
 #[cfg(test)]
 mod tests {
     use super::*;
--- a/crates/aragorn-app/src/session.rs
+++ b/crates/aragorn-app/src/session.rs
@@ -233,6 +233,11 @@
         self.gb.debug().breakpoint_hit()
     }
 
+    /// 디버거: 채널(0–3 = 채널 1–4)을 음소거한다. 스테이트를 불러와도 유지한다.
+    pub fn set_channel_muted(&mut self, channel: usize, muted: bool) {
+        self.gb.set_channel_muted(channel, muted);
+    }
+
     /// 디버거가 읽는 코어 상태.
     pub fn debug(&self) -> DebugView<'_> {
         self.gb.debug()
```

- [ ] **Step 4: 통과하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: app 120 passed (기존 105 + vram 10, io 4, session 1), clippy 경고 0

- [ ] **Step 5: 커밋한다**

```bash
cargo fmt --all --check
git add -A crates
git commit -m "feat(app): VRAM 타일·맵·스프라이트 해석과 I/O 레지스터 풀이

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---
### Task 3: 데스크톱: VRAM·I/O 창과 디버거 메뉴

**Files:** `crates/aragorn-desktop/src/ui/mod.rs`, `crates/aragorn-desktop/src/ui/debugger.rs`, `crates/aragorn-desktop/src/ui/vram_view.rs`, `crates/aragorn-desktop/src/ui/io_view.rs`, `crates/aragorn-desktop/src/app.rs`

**Interfaces:**
- Consumes:
  - Task 1 `DebugView::{cgb, vram, oam, palette_ram, palette_color}`, `PaletteKind`
  - Task 2 `debug::{vram, io}`, `Session::set_channel_muted`
- Produces:
  - `ui::vram_view::{VramView, tile_at}`, `ui::io_view::IoView`
  - `DebugCommand::SetChannelMuted`, `DebuggerView::{cpu_open, memory_open}`
  - 툴바 "디버거" 메뉴

- [ ] **Step 1: 테스트 패치를 적용한다**

```diff
--- a/crates/aragorn-desktop/src/ui/mod.rs
+++ b/crates/aragorn-desktop/src/ui/mod.rs
@@ -1,7 +1,9 @@
 pub mod debugger;
 pub mod fonts;
 pub mod input;
+pub mod io_view;
 pub mod screen;
 pub mod settings;
 pub mod state_slots;
 pub mod update_view;
+pub mod vram_view;
--- /dev/null
+++ b/crates/aragorn-desktop/src/ui/vram_view.rs
@@ -0,0 +1,30 @@
+//! VRAM 창: 타일, 배경·창 맵, OAM, 팔레트 (스펙 §6.3).
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn hovered_tile_follows_zoomed_grid() {
+        assert_eq!(tile_at(egui::vec2(0.0, 0.0), 2.0), Some(0));
+        assert_eq!(tile_at(egui::vec2(15.9, 15.9), 2.0), Some(0));
+        assert_eq!(tile_at(egui::vec2(16.0, 16.0), 2.0), Some(17));
+        assert_eq!(tile_at(egui::vec2(255.0, 383.0), 2.0), Some(383));
+        assert_eq!(tile_at(egui::vec2(256.0, 0.0), 2.0), None);
+        assert_eq!(tile_at(egui::vec2(0.0, 384.0), 2.0), None);
+        assert_eq!(tile_at(egui::vec2(-1.0, 0.0), 2.0), None);
+    }
+
+    #[test]
+    fn sprite_attributes_are_spelled_out() {
+        let s = Sprite {
+            index: 0,
+            y: 16,
+            x: 8,
+            tile: 0,
+            attr: 0x80 | 0x20 | 0x10 | 0x0B,
+        };
+        assert_eq!(attributes(&s, true), "팔레트 3 뱅크 1 ↔ BG뒤");
+        assert_eq!(attributes(&s, false), "OBP1 ↔ BG뒤");
+        assert_eq!(attributes(&Sprite { attr: 0x40, ..s }, false), "OBP0 ↕");
+    }
+}
```

- [ ] **Step 2: 실패하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace`
Expected: 컴파일 실패, ``cannot find function `tile_at` in this scope``, ``cannot find function `attributes` in this scope``

- [ ] **Step 3: 구현 패치를 적용한다**

```diff
--- a/crates/aragorn-desktop/src/ui/debugger.rs
+++ b/crates/aragorn-desktop/src/ui/debugger.rs
@@ -17,6 +17,8 @@
     StepInstruction,
     StepFrame,
     SetBreakpoints(Vec<u16>),
+    /// 채널(0–3)의 음소거를 켜거나 끈다.
+    SetChannelMuted(usize, bool),
 }
 
 /// `0150`, `$0150`, `0x0150`, `c000` 같은 16진수 주소. 공백은 무시한다.
@@ -69,7 +71,8 @@
 
 #[derive(Default)]
 pub struct DebuggerView {
-    pub open: bool,
+    pub cpu_open: bool,
+    pub memory_open: bool,
     breakpoints: Vec<u16>,
     new_breakpoint: String,
     memory_address: String,
@@ -90,20 +93,19 @@
         paused: bool,
     ) -> Vec<DebugCommand> {
         let mut commands = Vec::new();
-        if !self.open {
-            return commands;
-        }
         let debug = session.debug();
-        let mut open = self.open;
+        let mut open = self.cpu_open;
         egui::Window::new("CPU")
             .open(&mut open)
             .default_width(360.0)
             .show(ctx, |ui| self.cpu_window(ui, &debug, paused, &mut commands));
+        self.cpu_open = open;
+        let mut open = self.memory_open;
         egui::Window::new("메모리")
             .open(&mut open)
             .default_width(480.0)
             .show(ctx, |ui| self.memory_window(ui, &debug));
-        self.open = open;
+        self.memory_open = open;
         commands
     }
 
--- a/crates/aragorn-desktop/src/ui/vram_view.rs
+++ b/crates/aragorn-desktop/src/ui/vram_view.rs
@@ -1,4 +1,263 @@
 //! VRAM 창: 타일, 배경·창 맵, OAM, 팔레트 (스펙 §6.3).
+
+use std::collections::HashMap;
+
+use aragorn_app::debug::vram::{self, MapSource, Sprite};
+use eframe::egui;
+use gb_core::{DebugView, PaletteKind};
+
+use super::screen::to_rgba;
+
+/// 타일 시트와 맵을 그릴 배율.
+const ZOOM: f32 = 2.0;
+/// 스프라이트 미리보기 배율.
+const SPRITE_ZOOM: f32 = 3.0;
+/// 맵 위 화면 테두리 색.
+const VIEWPORT: u32 = 0xFF30_30FF;
+
+#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
+enum Tab {
+    #[default]
+    Tiles,
+    Map,
+    Oam,
+    Palettes,
+}
+
+/// 배율 `zoom`으로 그린 타일 시트에서 `pos`(시트 왼쪽 위 기준)에 있는 타일 번호(0–383).
+pub fn tile_at(pos: egui::Vec2, zoom: f32) -> Option<usize> {
+    if pos.x < 0.0 || pos.y < 0.0 {
+        return None;
+    }
+    let (x, y) = ((pos.x / zoom) as usize / 8, (pos.y / zoom) as usize / 8);
+    (x < vram::SHEET_COLUMNS && y < vram::SHEET_HEIGHT / 8).then_some(y * vram::SHEET_COLUMNS + x)
+}
+
+#[derive(Default)]
+pub struct VramView {
+    pub open: bool,
+    tab: Tab,
+    bank: u8,
+    /// 창 맵을 볼지 (아니면 배경 맵)
+    window_map: bool,
+    textures: HashMap<String, egui::TextureHandle>,
+}
+
+impl VramView {
+    pub fn show(&mut self, ctx: &egui::Context, debug: &DebugView<'_>) {
+        let mut open = self.open;
+        egui::Window::new("VRAM")
+            .open(&mut open)
+            .default_width(540.0)
+            .show(ctx, |ui| {
+                ui.horizontal(|ui| {
+                    ui.selectable_value(&mut self.tab, Tab::Tiles, "타일");
+                    ui.selectable_value(&mut self.tab, Tab::Map, "배경 맵");
+                    ui.selectable_value(&mut self.tab, Tab::Oam, "OAM");
+                    ui.selectable_value(&mut self.tab, Tab::Palettes, "팔레트");
+                });
+                ui.separator();
+                // 창이 이미지보다 작아도 끝까지 볼 수 있게 한다.
+                egui::ScrollArea::both().show(ui, |ui| match self.tab {
+                    Tab::Tiles => self.tiles(ui, debug),
+                    Tab::Map => self.map(ui, debug),
+                    Tab::Oam => self.oam(ui, debug),
+                    Tab::Palettes => palettes(ui, debug),
+                });
+            });
+        self.open = open;
+    }
+
+    /// 텍스처를 `pixels`로 갱신하고 그 id를 돌려준다.
+    fn texture(
+        &mut self,
+        ctx: &egui::Context,
+        name: &str,
+        size: [usize; 2],
+        pixels: &[u32],
+    ) -> egui::TextureId {
+        let image = egui::ColorImage::from_rgba_unmultiplied(size, &to_rgba(pixels));
+        let options = egui::TextureOptions::NEAREST;
+        match self.textures.get_mut(name) {
+            Some(texture) => {
+                texture.set(image, options);
+                texture.id()
+            }
+            None => {
+                let texture = ctx.load_texture(name, image, options);
+                let id = texture.id();
+                self.textures.insert(name.to_string(), texture);
+                id
+            }
+        }
+    }
+
+    fn tiles(&mut self, ui: &mut egui::Ui, debug: &DebugView<'_>) {
+        if debug.cgb() {
+            ui.horizontal(|ui| {
+                ui.selectable_value(&mut self.bank, 0, "뱅크 0");
+                ui.selectable_value(&mut self.bank, 1, "뱅크 1");
+            });
+        } else {
+            self.bank = 0;
+        }
+        let sheet = vram::tile_sheet(debug.vram(self.bank), vram::GRAY);
+        let id = self.texture(
+            ui.ctx(),
+            "vram-tiles",
+            [vram::SHEET_WIDTH, vram::SHEET_HEIGHT],
+            &sheet,
+        );
+        let size = egui::vec2(vram::SHEET_WIDTH as f32, vram::SHEET_HEIGHT as f32) * ZOOM;
+        let response = ui.add(egui::Image::new((id, size)).sense(egui::Sense::hover()));
+        if let Some(pos) = response.hover_pos()
+            && let Some(tile) = tile_at(pos - response.rect.min, ZOOM)
+        {
+            let addr = 0x8000 + tile * 16;
+            response.on_hover_text(format!("뱅크 {} 타일 {tile} (${addr:04X})", self.bank));
+        }
+    }
+
+    fn map(&mut self, ui: &mut egui::Ui, debug: &DebugView<'_>) {
+        ui.horizontal(|ui| {
+            ui.selectable_value(&mut self.window_map, false, "배경");
+            ui.selectable_value(&mut self.window_map, true, "창");
+        });
+        let lcdc = debug.peek(0xFF40);
+        let source = MapSource::from_lcdc(lcdc, self.window_map);
+        let bank1 = debug.cgb().then(|| debug.vram(1));
+        let mut map = vram::tile_map(debug.vram(0), bank1, source, |p, c| {
+            debug.palette_color(PaletteKind::Bg, p, c)
+        });
+        if !self.window_map {
+            vram::outline_viewport(&mut map, debug.peek(0xFF43), debug.peek(0xFF42), VIEWPORT);
+        }
+        ui.label(format!(
+            "맵 {}, 타일 {}{}",
+            if source.high_map { "$9C00" } else { "$9800" },
+            if source.unsigned_tiles {
+                "$8000"
+            } else {
+                "$8800"
+            },
+            if self.window_map {
+                ""
+            } else {
+                " · 빨간 테두리는 지금 화면"
+            }
+        ));
+        let id = self.texture(ui.ctx(), "vram-map", [vram::MAP_SIZE, vram::MAP_SIZE], &map);
+        let size = egui::Vec2::splat(vram::MAP_SIZE as f32 * ZOOM);
+        ui.add(egui::Image::new((id, size)));
+    }
+
+    fn oam(&mut self, ui: &mut egui::Ui, debug: &DebugView<'_>) {
+        let cgb = debug.cgb();
+        let tall = debug.peek(0xFF40) & 0x04 != 0;
+        let height = if tall { 16 } else { 8 };
+        let sprites = vram::sprites(debug.oam());
+        egui::Grid::new("oam").striped(true).show(ui, |ui| {
+            for title in ["#", "", "X", "Y", "타일", "속성"] {
+                ui.strong(title);
+            }
+            ui.end_row();
+            for sprite in &sprites {
+                let image = vram::sprite_image(sprite, debug.vram(sprite.bank(cgb)), tall, |c| {
+                    debug.palette_color(PaletteKind::Obj, sprite.palette(cgb), c)
+                });
+                let id = self.texture(
+                    ui.ctx(),
+                    &format!("oam-{}", sprite.index),
+                    [8, height],
+                    &image,
+                );
+                let mono = |text: String| {
+                    let text = egui::RichText::new(text).monospace();
+                    if sprite.on_screen(tall) {
+                        text
+                    } else {
+                        text.weak()
+                    }
+                };
+                ui.label(mono(sprite.index.to_string()));
+                ui.add(egui::Image::new((
+                    id,
+                    egui::vec2(8.0, height as f32) * SPRITE_ZOOM,
+                )));
+                ui.label(mono(format!("{:3}", i16::from(sprite.x) - 8)));
+                ui.label(mono(format!("{:3}", i16::from(sprite.y) - 16)));
+                ui.label(mono(format!("${:02X}", sprite.tile)));
+                ui.label(mono(attributes(sprite, cgb)));
+                ui.end_row();
+            }
+        });
+    }
+}
+
+/// 속성 바이트 풀이: `팔레트 3 뱅크 1 ↔ ↕ BG뒤`
+fn attributes(sprite: &Sprite, cgb: bool) -> String {
+    let mut parts = vec![if cgb {
+        format!("팔레트 {} 뱅크 {}", sprite.palette(true), sprite.bank(true))
+    } else {
+        format!("OBP{}", sprite.palette(false))
+    }];
+    if sprite.flip_x() {
+        parts.push("↔".into());
+    }
+    if sprite.flip_y() {
+        parts.push("↕".into());
+    }
+    if sprite.behind_bg() {
+        parts.push("BG뒤".into());
+    }
+    parts.join(" ")
+}
+
+fn swatch(ui: &mut egui::Ui, rgba: u32, hover: String) {
+    let [r, g, b, _] = rgba.to_be_bytes();
+    let (rect, response) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
+    ui.painter()
+        .rect_filled(rect, 2.0, egui::Color32::from_rgb(r, g, b));
+    response.on_hover_text(hover);
+}
+
+fn palettes(ui: &mut egui::Ui, debug: &DebugView<'_>) {
+    if debug.cgb() {
+        for (kind, title) in [(PaletteKind::Bg, "BG"), (PaletteKind::Obj, "OBJ")] {
+            ui.strong(title);
+            egui::Grid::new(title).show(ui, |ui| {
+                for palette in 0..8u8 {
+                    ui.label(format!("{palette}"));
+                    for color in 0..4u8 {
+                        let word = vram::palette_word(debug.palette_ram(kind), palette, color);
+                        let rgba = debug.palette_color(kind, palette, color);
+                        swatch(ui, rgba, format!("색 {color}: ${word:04X}"));
+                    }
+                    ui.end_row();
+                }
+            });
+        }
+    } else {
+        egui::Grid::new("dmg-palettes").show(ui, |ui| {
+            for (kind, palette, name, addr) in [
+                (PaletteKind::Bg, 0, "BGP", 0xFF47),
+                (PaletteKind::Obj, 0, "OBP0", 0xFF48),
+                (PaletteKind::Obj, 1, "OBP1", 0xFF49),
+            ] {
+                ui.label(format!("{name} ${:02X}", debug.peek(addr)));
+                for color in 0..4u8 {
+                    swatch(
+                        ui,
+                        debug.palette_color(kind, palette, color),
+                        format!("색 {color}"),
+                    );
+                }
+                ui.end_row();
+            }
+        });
+    }
+}
+
 #[cfg(test)]
 mod tests {
     use super::*;
--- /dev/null
+++ b/crates/aragorn-desktop/src/ui/io_view.rs
@@ -0,0 +1,63 @@
+//! I/O 창: LCD·타이머·인터럽트·사운드 레지스터와 채널별 음소거 (스펙 §6.3).
+
+use aragorn_app::debug::io;
+use eframe::egui;
+use gb_core::DebugView;
+
+use super::debugger::DebugCommand;
+
+#[derive(Default)]
+pub struct IoView {
+    pub open: bool,
+    muted: [bool; 4],
+}
+
+impl IoView {
+    /// 채널 1–4의 음소거. ROM을 바꾸면 새 세션에 다시 건다.
+    pub fn muted(&self) -> [bool; 4] {
+        self.muted
+    }
+
+    pub fn show(&mut self, ctx: &egui::Context, debug: &DebugView<'_>) -> Vec<DebugCommand> {
+        let mut commands = Vec::new();
+        let mut open = self.open;
+        egui::Window::new("I/O")
+            .open(&mut open)
+            .default_width(520.0)
+            .show(ctx, |ui| {
+                ui.horizontal(|ui| {
+                    ui.label("음소거");
+                    for (channel, muted) in self.muted.iter_mut().enumerate() {
+                        if ui
+                            .checkbox(muted, format!("채널 {}", channel + 1))
+                            .changed()
+                        {
+                            commands.push(DebugCommand::SetChannelMuted(channel, *muted));
+                        }
+                    }
+                });
+                ui.separator();
+                egui::ScrollArea::vertical().show(ui, |ui| {
+                    for (group, registers) in io::GROUPS {
+                        egui::CollapsingHeader::new(*group)
+                            .default_open(true)
+                            .show(ui, |ui| {
+                                egui::Grid::new(*group).striped(true).show(ui, |ui| {
+                                    for &(addr, name) in *registers {
+                                        let value = debug.peek(addr);
+                                        let mono = |t: String| egui::RichText::new(t).monospace();
+                                        ui.label(mono(format!("{name:<5}")));
+                                        ui.label(mono(format!("${addr:04X}")));
+                                        ui.label(mono(format!("{value:02X}")));
+                                        ui.label(io::describe(addr, value));
+                                        ui.end_row();
+                                    }
+                                });
+                            });
+                    }
+                });
+            });
+        self.open = open;
+        commands
+    }
+}
--- a/crates/aragorn-desktop/src/app.rs
+++ b/crates/aragorn-desktop/src/app.rs
@@ -4,10 +4,12 @@
     ui::{
         debugger::{DebugCommand, DebuggerView},
         input::{self, InputFrame, Keymap, TypingGuard},
+        io_view::IoView,
         screen::ScreenView,
         settings::SettingsView,
         state_slots::{self, SlotCommand, SlotsView},
         update_view::{self, UpdateUiAction},
+        vram_view::VramView,
     },
     update_worker::UpdateWorker,
 };
@@ -114,6 +116,8 @@
     settings: SettingsView,
     slots: SlotsView,
     debugger: DebuggerView,
+    vram: VramView,
+    io: IoView,
     typing_guard: TypingGuard,
 }
 
@@ -158,6 +162,8 @@
             settings: SettingsView::default(),
             slots: SlotsView::default(),
             debugger: DebuggerView::default(),
+            vram: VramView::default(),
+            io: IoView::default(),
             typing_guard: TypingGuard::default(),
         };
         if app.worker.is_some() {
@@ -187,6 +193,9 @@
                 self.slots.invalidate();
                 let mut session = session;
                 session.set_breakpoints(self.debugger.breakpoints());
+                for (channel, muted) in self.io.muted().into_iter().enumerate() {
+                    session.set_channel_muted(channel, muted);
+                }
                 self.session = Some(session);
                 self.last_tick = Instant::now();
                 self.collect_session_errors();
@@ -274,12 +283,17 @@
     /// 디버거 창의 요청을 처리한다. 단계 실행은 일시정지한 상태에서 한다. 종료 중이거나 강제 업데이트
     /// 중에는 하지 않는다.
     fn run_debug_command(&mut self, ctx: &egui::Context, command: DebugCommand) {
+        let Some(session) = &mut self.session else {
+            return;
+        };
+        // 음소거는 게임을 진행하지 않으니 언제든 바꾼다. 창의 체크 상태와 어긋나지 않게 한다.
+        if let DebugCommand::SetChannelMuted(channel, muted) = command {
+            session.set_channel_muted(channel, muted);
+            return;
+        }
         if self.exit_handled || self.flow.blocks_emulation() {
             return;
         }
-        let Some(session) = &mut self.session else {
-            return;
-        };
         match command {
             DebugCommand::TogglePause => self.speed.toggle_pause(),
             DebugCommand::StepInstruction | DebugCommand::StepFrame => {
@@ -292,6 +306,7 @@
                 self.screen.update(ctx, session.framebuffer());
             }
             DebugCommand::SetBreakpoints(pcs) => session.set_breakpoints(&pcs),
+            DebugCommand::SetChannelMuted(..) => unreachable!("위에서 처리한다"),
         }
         self.collect_session_errors();
     }
@@ -332,7 +347,7 @@
         if let Some(pc) = session.take_breakpoint_stop() {
             self.speed.pause();
             self.mode = RunMode::Paused;
-            self.debugger.open = true;
+            self.debugger.cpu_open = true;
             self.notice = Some(format!("브레이크포인트 ${pc:04X}에서 멈췄습니다"));
         }
         if let Some(sink) = self.audio.sink() {
@@ -455,9 +470,12 @@
                 if ui.button("ROM 열기").clicked() {
                     open_requested = true;
                 }
-                if ui.button("디버거").clicked() {
-                    self.debugger.open = !self.debugger.open;
-                }
+                ui.menu_button("디버거", |ui| {
+                    ui.checkbox(&mut self.debugger.cpu_open, "CPU");
+                    ui.checkbox(&mut self.debugger.memory_open, "메모리");
+                    ui.checkbox(&mut self.vram.open, "VRAM");
+                    ui.checkbox(&mut self.io.open, "I/O");
+                });
                 if ui.button("설정").clicked() {
                     self.settings.open = !self.settings.open;
                 }
@@ -522,9 +540,14 @@
         });
 
         let debug_commands = match &self.session {
-            Some(session) => self
-                .debugger
-                .show(ui.ctx(), session, self.speed.is_paused()),
+            Some(session) => {
+                let mut commands = self
+                    .debugger
+                    .show(ui.ctx(), session, self.speed.is_paused());
+                self.vram.show(ui.ctx(), &session.debug());
+                commands.extend(self.io.show(ui.ctx(), &session.debug()));
+                commands
+            }
             None => Vec::new(),
         };
         for command in debug_commands {
```

- [ ] **Step 4: 통과하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: desktop 79 passed (기존 77 + vram_view 2), clippy 경고 0

- [ ] **Step 5: 커밋한다**

```bash
cargo fmt --all --check
git add -A crates
git commit -m "feat(desktop): VRAM·I/O 디버거 창과 채널 음소거

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---
### Task 4: 전체 확인, PR

- [ ] **Step 1: 전체 검사를 실행한다**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace --no-fail-fast 2>&1 | grep "test result" | grep -v " 0 passed"`
Expected: 모든 결과가 `ok`다.

| 테스트 | 개수 |
|---|---|
| gb-core lib | 264 |
| acid2 (DMG) | 1 |
| blargg | 15 |
| cgb_acid2 | 1 |
| cgb_sound | 12 |
| dmg_sound | 12 |
| mooneye | 77 |
| save_state | 14 |
| app | 120 |
| desktop | 79 |
| xtask | 14 |

- [ ] **Step 2: PR을 만들고 CI를 확인한다** (외부 공개 작업이므로 사용자 확인 후)

```bash
git push -u origin feat/m8b-debugger
gh pr create --repo dongjay00/aragorn --base main --title "M8b: 디버거 (VRAM·I/O·채널 음소거)" --body-file <작성한 본문>
gh pr checks --watch
```

- [ ] **Step 3: 사용 확인을 요청한다** (사용자 작업, 릴리스 뒤)

사용자에게 아래 확인을 요청한다.
- 툴바 "디버거" 메뉴에서 CPU·메모리·VRAM·I/O 창을 하나씩 켜고 끌 수 있다.
- VRAM 창:
  - 타일 탭에 게임 타일이 보인다.
  - 배경 맵 탭에 빨간 테두리(지금 화면)가 스크롤에 맞춰 움직인다.
  - OAM 탭에 스프라이트가 보인다.
  - 팔레트 탭에 색이 보인다.
- I/O 창에서 채널 음소거를 켜면 그 채널 소리만 사라지고, 끄면 돌아온다.
