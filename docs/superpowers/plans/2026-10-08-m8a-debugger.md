# 마일스톤 8a: 디버거 (CPU·실행 제어·메모리) 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:**
- 디버거 창 두 개를 넣는다(스펙 §6.3의 앞부분).
  - CPU 창: 레지스터, 플래그, IME, HALT, PC 주변 디스어셈블리, 실행 제어(일시정지, 명령 1개, 프레임 1개), PC 브레이크포인트
  - 메모리 창: 16진수 뷰어와 주소 이동
- VRAM, I/O, 채널 음소거는 M8b에서 넣는다.

**Architecture:**
- `gb-core`:
  - `cpu::disasm::disassemble(addr, read) -> Instruction { len, text }`
    - SM83의 일반 옵코드 256개와 CB 옵코드 256개를 x/y/z/p/q 비트 분해로 해석한다.
    - 니모닉은 Pan Docs 표기다. JR은 도착 주소로, 정의되지 않은 옵코드는 `DB $D3`으로 보인다.
  - 새 `debugger::Debugger`(스테이트에 넣지 않음):
    - 64K 비트맵 브레이크포인트
    - 정지 PC
    - 최근 실행한 PC 16개 링 버퍼
  - `GameBoy`:
    - `run_frame`은 브레이크포인트 주소의 명령을 실행하기 직전에 멈춘다(스펙 §4.3). 멈춘 PC에서 다시 부르면 그 명령은 한 번 통과한다.
    - `step_instruction()`은 브레이크포인트를 보지 않고, HALT 중이면 최대 한 프레임 분량까지 기다린다.
    - `set_breakpoints(&[u16])`(스펙 §4.1)를 둔다. 스테이트를 불러와도 브레이크포인트는 유지한다.
  - `DebugView`에 `ime`, `halted`, `breakpoint_hit`, `trace`, `disassemble`을 더한다.
- `aragorn-app`:
  - `Session::run`은 브레이크포인트에 걸리면 남은 프레임을 돌리지 않는다.
  - `step_instruction`, `step_frame`(소리 끔), `set_breakpoints`, `breakpoint_hit`, `debug`를 둔다.
  - `SpeedControl::pause()`를 더한다.
- `aragorn-desktop`:
  - `ui::debugger::DebuggerView`가 CPU 창과 메모리 창을 그리고 `DebugCommand`를 돌려준다.
    - 테스트하는 순수 함수: `parse_address`, `hex_row`, `flags_text`, `toggle_breakpoint`
  - `app.rs`:
    - 툴바에 "디버거" 버튼을 둔다.
    - 브레이크포인트에 걸리면 일시정지하고 디버거를 연 뒤 상태 표시줄에 알린다.
    - 단계 실행 명령은 일시정지한 상태에서 실행한다.
    - ROM을 바꾸면 브레이크포인트 목록을 새 세션에 다시 건다.
  - 글자 입력칸(주소 입력)에 포커스가 있는 동안:
    - `release_widget_focus`가 포커스를 남긴다.
    - 키보드는 게임과 스테이트 단축키에 넘기지 않는다.

**Tech Stack:** Rust 1.99 (edition 2024), 새 의존성 없음

**Spec:** `docs/superpowers/specs/2026-10-07-gameboy-emulator-design.md` (§4.1 `set_breakpoints`·`debug`, §4.3 브레이크포인트 정지, §6.3 디버거 UI, §9의 마일스톤 8)

**작업 브랜치:** `feat/m8a-debugger`

## Global Constraints

- 게임 데이터 때문에 패닉하지 않는다. 디버거가 메모리를 읽어도 게임 상태는 바뀌지 않는다(`peek`).
- 사용자에게 보이는 문구, 문서, 커밋 메시지는 한국어로 쓴다.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace --no-fail-fast`가 모든 커밋에서 통과해야 한다. 로컬과 CI 모두 Rust 1.99 stable이다.
- 리눅스 로컬에서 데스크톱을 빌드할 때 linuxbrew pkg-config를 쓴다면 `PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig`가 필요하다(`libudev-dev`도 필요하다).
- 커밋 메시지 끝에는 `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`를 붙인다.
- 릴리스(v0.9.0)는 병합 뒤 사용자 확인을 받고 한다.

## 패치 적용 방법

각 태스크의 코드는 diff 블록이다. 블록 내용을 **한 글자도 바꾸지 말고**(빈 컨텍스트 줄의 앞 공백 포함) 파일로 저장한 뒤 `git apply <파일>`로 적용한다.

이 패치들은 재현 브랜치에서 main 위에 Task 1→4 순서로 적용해 확인했다. 결과는 아래와 같다.
- 적용 결과가 스파이크와 바이트 단위로 같다.
- 태스크마다 테스트 패치만 넣으면 실패하고, 구현 패치까지 넣으면 통과한다.
- clippy 경고는 0이다.
- 새 파일은 테스트 패치가 모듈 주석과 테스트만 만들고, 구현 패치가 그 위에 구현을 더한다.
- 패치를 같은 초 안에 연달아 적용하면 cargo가 바뀐 파일을 놓칠 수 있다. 구현 패치를 적용한 뒤에도 예전 코드 기준의 컴파일 오류가 나면 바뀐 파일을 `touch`한다.

## 계획 전 검증 (스파이크)

- 테스트 결과:

  | 범위 | 결과 |
  |---|---|
  | 단위 테스트 | core 258 (+13), app 104 (+2), desktop 76 (+5), xtask 14 |
  | 인수 테스트 | dmg/cgb-acid2, Blargg 15, dmg_sound 12, cgb_sound 12, mooneye 77, save_state 14 모두 그대로 통과 |

- **디스어셈블러 길이 검증 (`lengths_match_cpu_execution`):**
  - 정의된 옵코드 전부를 평면 메모리 CPU로 실제 실행하고, F=0과 F=0xF0 두 가지에서 PC 증가량을 길이와 대조한다.
  - 분기해서 0번지 근처로 간 경우는 뺀다.
  - 돌연변이 검사: LD rp,nn / CALL cc / STOP / ALU n의 길이를 일부러 틀리게 바꾸면 모두 실패한다.
- **입력칸 포커스 테스트 (`text_field_keeps_focus_while_typing`):** `release_widget_focus`의 입력칸 예외를 빼면 실패하는 것을 확인했다.
- WSLg에서 릴리스 빌드에 dmg-acid2를 실어 띄웠고, 경고나 패닉이 없었다.

## 스펙과 다른 결정 (리뷰어 확인용)

1. **"명령 1개 실행"은 브레이크포인트를 보지 않는다.**
   - 스펙에는 따로 정해진 것이 없다.
   - HALT 중이면 깨어나 다음 명령을 실행할 때까지, 최대 한 프레임 분량만 진행한다.
2. **디버거 단축키는 없다.** F1–F10은 스테이트가 쓰므로 버튼으로만 조작한다.
3. **브레이크포인트는 이번 실행 동안만 유지한다.** 설정 파일에는 저장하지 않는다.
4. **실행 흔적(최근 PC 16개)을 더한다.**
   - 스펙 §6.3의 "PC 주변 디스어셈블리"에서 PC 앞쪽을 거꾸로 해석하면 모호하다.
   - 그래서 실제로 실행한 명령을 보여 준다.
5. **글자 입력칸에 입력하는 동안 키보드는 게임에 가지 않는다.** 게임패드는 그대로 받는다.

## Review Focus

1. **브레이크포인트 의미:**
   - 실행 직전에 멈추는지, 다시 실행하면 한 번 통과하는지, 루프에서는 매번 걸리는지
   - HALT·정지(lock) 중에는 걸리지 않는지
   - 인터럽트 디스패치와의 관계
   - → `breakpoint_stops_before_the_instruction_mid_frame`, `resuming_passes_the_breakpoint_once`, `loop_hits_the_breakpoint_every_iteration`
2. **앱 흐름:**
   - 브레이크포인트에 걸린 프레임 뒤에 일시정지가 되는지
   - 일시정지 중의 "계속"이 그 자리를 통과하는지
   - 단계 실행이 소리를 쌓지 않는지
   - 배속(무제한) 중에 걸려도 바로 멈추는지
   - → `breakpoint_stops_running_further_frames`, `stepping_runs_one_instruction_or_frame_silently`
3. **성능:** 명령마다 브레이크포인트 비트맵 확인과 실행 흔적 기록이 늘었다. 인수 테스트 시간이 크게 달라지지 않았는지 본다.
4. **디스어셈블러 정확성:** 니모닉 표기와 접두 CB, 부호 있는 오프셋, `$FF00+C`, 주소 감싸기를 본다.
5. **입력 충돌:**
   - 주소 입력 중의 키보드(Enter=Start, Backspace=Select 등)가 게임으로 새지 않는지
   - 입력이 끝나면 다시 게임으로 가는지
   - 디버거 버튼이 Tab/Enter로 눌리지 않는지(M7a 포커스 정리)

---
### Task 1: 디스어셈블러

**Files:** `crates/gb-core/src/cpu/mod.rs`, `crates/gb-core/src/cpu/disasm.rs`, `crates/gb-core/src/lib.rs`

**Interfaces:**
- Produces:
  - `gb_core::cpu::disasm::{disassemble, Instruction}`
  - `gb_core::Instruction` (다시 내보냄)

- [ ] **Step 1: 테스트 패치를 적용한다**

```bash
git checkout main && git pull --ff-only && git checkout -b feat/m8a-debugger
```

```diff
--- a/crates/gb-core/src/cpu/mod.rs
+++ b/crates/gb-core/src/cpu/mod.rs
@@ -1,6 +1,7 @@
 //! SM83 CPU. 모든 메모리 접근과 내부 지연마다 `CpuBus::tick`을 호출한다 (M-사이클 tick 모델).
 
 pub mod alu;
+pub mod disasm;
 pub mod registers;
 
 #[cfg(test)]
--- /dev/null
+++ b/crates/gb-core/src/cpu/disasm.rs
@@ -0,0 +1,110 @@
+//! SM83 디스어셈블러 (디버거용). 니모닉은 Pan Docs "CPU Instruction Set" 표기를 따른다.
+//!
+//! 옵코드를 x(7–6)·y(5–3)·z(2–0) 비트와 p = y >> 1, q = y & 1로 나눠 해석한다.
+#[cfg(test)]
+mod tests {
+    use super::super::testing::FlatBus;
+    use super::super::{Cpu, Registers};
+    use super::*;
+
+    fn text(bytes: &[u8], at: u16) -> Instruction {
+        disassemble(at, |a| {
+            bytes
+                .get(usize::from(a.wrapping_sub(at)))
+                .copied()
+                .unwrap_or(0)
+        })
+    }
+
+    #[test]
+    fn mnemonics_follow_pan_docs() {
+        let cases: [(&[u8], &str); 24] = [
+            (&[0x00], "NOP"),
+            (&[0x08, 0x34, 0x12], "LD ($1234),SP"),
+            (&[0x01, 0xCD, 0xAB], "LD BC,$ABCD"),
+            (&[0x22], "LD (HL+),A"),
+            (&[0x3A], "LD A,(HL-)"),
+            (&[0x36, 0x7F], "LD (HL),$7F"),
+            (&[0x76], "HALT"),
+            (&[0x7E], "LD A,(HL)"),
+            (&[0x8E], "ADC A,(HL)"),
+            (&[0x97], "SUB A"),
+            (&[0xFE, 0x90], "CP $90"),
+            (&[0xE0, 0x44], "LDH ($FF44),A"),
+            (&[0xF0, 0x40], "LDH A,($FF40)"),
+            (&[0xE2], "LD ($FF00+C),A"),
+            (&[0xEA, 0x00, 0xC0], "LD ($C000),A"),
+            (&[0xE8, 0xFE], "ADD SP,-$02"),
+            (&[0xF8, 0x05], "LD HL,SP+$05"),
+            (&[0xC2, 0x50, 0x01], "JP NZ,$0150"),
+            (&[0xCD, 0x00, 0x40], "CALL $4000"),
+            (&[0xD8], "RET C"),
+            (&[0xFF], "RST $38"),
+            (&[0xCB, 0x37], "SWAP A"),
+            (&[0xCB, 0x7E], "BIT 7,(HL)"),
+            (&[0xD3], "DB $D3"),
+        ];
+        for (bytes, expected) in cases {
+            assert_eq!(text(bytes, 0x0100).text, expected, "{bytes:02X?}");
+        }
+    }
+
+    #[test]
+    fn relative_jumps_show_absolute_target() {
+        assert_eq!(text(&[0x18, 0xFE], 0x0150).text, "JR $0150");
+        assert_eq!(text(&[0x20, 0x05], 0x0150).text, "JR NZ,$0157");
+        assert_eq!(
+            text(&[0x38, 0x80], 0x0000).text,
+            "JR C,$FF82",
+            "주소가 감싼다"
+        );
+    }
+
+    #[test]
+    fn every_cb_opcode_is_two_bytes_and_distinct() {
+        let mut seen = std::collections::HashSet::new();
+        for op in 0..=255u8 {
+            let i = text(&[0xCB, op], 0);
+            assert_eq!(i.len, 2);
+            assert!(seen.insert(i.text.clone()), "{}", i.text);
+        }
+    }
+
+    /// 실제 CPU가 명령 하나를 실행하고 PC가 얼마나 나아가는지와 길이를 대조한다. 피연산자는 0이라
+    /// 분기 명령은 주소 0으로 가거나(점프·호출·복귀) 다음 명령으로 간다(JR 0). 0으로 간 경우는 빼고 본다.
+    #[test]
+    fn lengths_match_cpu_execution() {
+        const ILLEGAL: [u8; 11] = [
+            0xD3, 0xDB, 0xDD, 0xE3, 0xE4, 0xEB, 0xEC, 0xED, 0xF4, 0xFC, 0xFD,
+        ];
+        for op in 0..=255u8 {
+            if ILLEGAL.contains(&op) || op == 0x76 {
+                continue;
+            }
+            let program: &[u8] = if op == 0xCB { &[0xCB, 0x00] } else { &[op] };
+            let expected = text(program, 0x0100).len;
+            let mut lengths = Vec::new();
+            for f in [0x00, 0xF0] {
+                let mut bus = FlatBus::with_program(program);
+                let mut cpu = Cpu::new(Registers {
+                    f,
+                    sp: 0xFFF0,
+                    pc: 0x0100,
+                    ..Registers::default()
+                });
+                cpu.step(&mut bus);
+                let pc = cpu.regs.pc;
+                // RST·JP HL 같은 무조건 분기는 0번지 근처로 간다
+                if !(0x0000..0x0100).contains(&pc) {
+                    lengths.push(pc - 0x0100);
+                }
+            }
+            if lengths.is_empty() {
+                continue;
+            }
+            for len in lengths {
+                assert_eq!(u16::from(expected), len, "{op:#04X}");
+            }
+        }
+    }
+}
--- a/crates/gb-core/src/lib.rs
+++ b/crates/gb-core/src/lib.rs
@@ -17,7 +17,7 @@
 pub mod timer;
 
 pub use cartridge::{CartError, Header};
-pub use cpu::{IllegalOpcode, Registers};
+pub use cpu::{IllegalOpcode, Registers, disasm::Instruction};
 pub use gameboy::{DebugView, GameBoy};
 pub use joypad::Button;
 pub use model::Model;
```

- [ ] **Step 2: 실패하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace`
Expected: 컴파일 실패, ``unresolved import `cpu::disasm::Instruction` ``

- [ ] **Step 3: 구현 패치를 적용한다**

```diff
--- a/crates/gb-core/src/cpu/disasm.rs
+++ b/crates/gb-core/src/cpu/disasm.rs
@@ -1,6 +1,119 @@
 //! SM83 디스어셈블러 (디버거용). 니모닉은 Pan Docs "CPU Instruction Set" 표기를 따른다.
 //!
 //! 옵코드를 x(7–6)·y(5–3)·z(2–0) 비트와 p = y >> 1, q = y & 1로 나눠 해석한다.
+
+/// 명령 하나의 길이와 문자열.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub struct Instruction {
+    /// 바이트 수 (1–3)
+    pub len: u8,
+    pub text: String,
+}
+
+const R: [&str; 8] = ["B", "C", "D", "E", "H", "L", "(HL)", "A"];
+const RP: [&str; 4] = ["BC", "DE", "HL", "SP"];
+const RP2: [&str; 4] = ["BC", "DE", "HL", "AF"];
+const CC: [&str; 4] = ["NZ", "Z", "NC", "C"];
+const ALU: [&str; 8] = [
+    "ADD A,", "ADC A,", "SUB ", "SBC A,", "AND ", "XOR ", "OR ", "CP ",
+];
+const ROT: [&str; 8] = ["RLC", "RRC", "RL", "RR", "SLA", "SRA", "SWAP", "SRL"];
+const ACC: [&str; 8] = ["RLCA", "RRCA", "RLA", "RRA", "DAA", "CPL", "SCF", "CCF"];
+
+/// 부호 있는 8비트 값을 `+$02` / `-$02`로 쓴다.
+fn signed(d: u8) -> String {
+    let d = d as i8;
+    if d < 0 {
+        format!("-${:02X}", d.unsigned_abs())
+    } else {
+        format!("+${d:02X}")
+    }
+}
+
+/// `addr`의 명령을 해석한다. `read`는 부작용 없이 메모리를 읽어야 한다.
+pub fn disassemble(addr: u16, read: impl Fn(u16) -> u8) -> Instruction {
+    let op = read(addr);
+    let n = read(addr.wrapping_add(1));
+    let nn = u16::from_le_bytes([n, read(addr.wrapping_add(2))]);
+    let (x, y, z) = (op >> 6, usize::from((op >> 3) & 7), usize::from(op & 7));
+    let (p, q) = (y >> 1, y & 1);
+    let one = |text: String| Instruction { len: 1, text };
+    let two = |text: String| Instruction { len: 2, text };
+    let three = |text: String| Instruction { len: 3, text };
+    // JR의 도착 주소: 다음 명령 위치 + 부호 있는 오프셋
+    let jr_target = addr.wrapping_add(2).wrapping_add_signed(i16::from(n as i8));
+    let illegal = || one(format!("DB ${op:02X}"));
+    match x {
+        0 => match z {
+            0 => match y {
+                0 => one("NOP".into()),
+                1 => three(format!("LD (${nn:04X}),SP")),
+                2 => two("STOP".into()),
+                3 => two(format!("JR ${jr_target:04X}")),
+                _ => two(format!("JR {},${jr_target:04X}", CC[y - 4])),
+            },
+            1 if q == 0 => three(format!("LD {},${nn:04X}", RP[p])),
+            1 => one(format!("ADD HL,{}", RP[p])),
+            2 => {
+                let mem = ["(BC)", "(DE)", "(HL+)", "(HL-)"][p];
+                one(if q == 0 {
+                    format!("LD {mem},A")
+                } else {
+                    format!("LD A,{mem}")
+                })
+            }
+            3 => one(format!("{} {}", ["INC", "DEC"][q], RP[p])),
+            4 => one(format!("INC {}", R[y])),
+            5 => one(format!("DEC {}", R[y])),
+            6 => two(format!("LD {},${n:02X}", R[y])),
+            _ => one(ACC[y].into()),
+        },
+        1 if y == 6 && z == 6 => one("HALT".into()),
+        1 => one(format!("LD {},{}", R[y], R[z])),
+        2 => one(format!("{}{}", ALU[y], R[z])),
+        _ => match z {
+            0 => match y {
+                0..=3 => one(format!("RET {}", CC[y])),
+                4 => two(format!("LDH ($FF{n:02X}),A")),
+                5 => two(format!("ADD SP,{}", signed(n))),
+                6 => two(format!("LDH A,($FF{n:02X})")),
+                _ => two(format!("LD HL,SP{}", signed(n))),
+            },
+            1 if q == 0 => one(format!("POP {}", RP2[p])),
+            1 => one(["RET", "RETI", "JP HL", "LD SP,HL"][p].into()),
+            2 => match y {
+                0..=3 => three(format!("JP {},${nn:04X}", CC[y])),
+                4 => one("LD ($FF00+C),A".into()),
+                5 => three(format!("LD (${nn:04X}),A")),
+                6 => one("LD A,($FF00+C)".into()),
+                _ => three(format!("LD A,(${nn:04X})")),
+            },
+            3 => match y {
+                0 => three(format!("JP ${nn:04X}")),
+                1 => {
+                    let (x, y, z) = (n >> 6, usize::from((n >> 3) & 7), usize::from(n & 7));
+                    two(match x {
+                        0 => format!("{} {}", ROT[y], R[z]),
+                        1 => format!("BIT {y},{}", R[z]),
+                        2 => format!("RES {y},{}", R[z]),
+                        _ => format!("SET {y},{}", R[z]),
+                    })
+                }
+                6 => one("DI".into()),
+                7 => one("EI".into()),
+                _ => illegal(),
+            },
+            4 if y <= 3 => three(format!("CALL {},${nn:04X}", CC[y])),
+            4 => illegal(),
+            5 if q == 0 => one(format!("PUSH {}", RP2[p])),
+            5 if p == 0 => three(format!("CALL ${nn:04X}")),
+            5 => illegal(),
+            6 => two(format!("{}${n:02X}", ALU[y])),
+            _ => one(format!("RST ${:02X}", y * 8)),
+        },
+    }
+}
+
 #[cfg(test)]
 mod tests {
     use super::super::testing::FlatBus;
```

- [ ] **Step 4: 통과하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: core lib 249 passed (기존 245 + disasm 4), clippy 경고 0

- [ ] **Step 5: 커밋한다**

```bash
cargo fmt --all --check
git add -A crates
git commit -m "feat(core): SM83 디스어셈블러

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---
### Task 2: 브레이크포인트와 실행 흔적

**Files:** `crates/gb-core/src/lib.rs`, `crates/gb-core/src/debugger.rs`, `crates/gb-core/src/gameboy.rs`

**Interfaces:**
- Consumes: Task 1 `disasm::disassemble`
- Produces:
  - `gb_core::debugger::{Debugger, TRACE_LEN}`
  - `GameBoy::{set_breakpoints, step_instruction}`. `run_frame`이 브레이크포인트에서 멈춘다.
  - `DebugView::{ime, halted, breakpoint_hit, trace, disassemble}`

- [ ] **Step 1: 테스트 패치를 적용한다**

```diff
--- a/crates/gb-core/src/lib.rs
+++ b/crates/gb-core/src/lib.rs
@@ -7,6 +7,7 @@
 pub mod bus;
 pub mod cartridge;
 pub mod cpu;
+pub mod debugger;
 mod gameboy;
 pub mod joypad;
 mod model;
--- /dev/null
+++ b/crates/gb-core/src/debugger.rs
@@ -0,0 +1,36 @@
+//! 디버거 상태: PC 브레이크포인트, 정지 위치, 최근 실행한 PC (스펙 §4.3, §6.3).
+//! 세이브 스테이트에는 넣지 않는다.
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn breakpoints_are_replaced_as_a_set() {
+        let mut d = Debugger::default();
+        assert!(!d.is_breakpoint(0x0150));
+        d.set_breakpoints(&[0x0150, 0xFFFF, 0x0000, 0x0150]);
+        assert!(d.is_breakpoint(0x0150));
+        assert!(d.is_breakpoint(0xFFFF));
+        assert!(d.is_breakpoint(0x0000));
+        assert!(!d.is_breakpoint(0x0151));
+        d.set_breakpoints(&[]);
+        assert!(!d.is_breakpoint(0x0150));
+    }
+
+    #[test]
+    fn trace_keeps_last_entries_oldest_first() {
+        let mut d = Debugger::default();
+        assert!(d.trace().is_empty());
+        for pc in 0..3u16 {
+            d.record(pc);
+        }
+        assert_eq!(d.trace(), [0, 1, 2]);
+        for pc in 3..40u16 {
+            d.record(pc);
+        }
+        let expected: Vec<u16> = (40 - TRACE_LEN as u16..40).collect();
+        assert_eq!(d.trace(), expected);
+        d.reset_run_state();
+        assert!(d.trace().is_empty());
+    }
+}
--- a/crates/gb-core/src/gameboy.rs
+++ b/crates/gb-core/src/gameboy.rs
@@ -406,6 +406,95 @@
     }
 
     #[test]
+    fn breakpoint_stops_before_the_instruction_mid_frame() {
+        // 0x0100부터 NOP만 있다 (ROM의 나머지는 0).
+        let mut gb = gb_with_program(&[]);
+        gb.set_breakpoints(&[0x0200]);
+        gb.run_frame();
+        assert_eq!(gb.debug().registers().pc, 0x0200);
+        assert_eq!(gb.debug().breakpoint_hit(), Some(0x0200));
+        assert_eq!(gb.debug().trace().last(), Some(&0x01FF));
+    }
+
+    #[test]
+    fn resuming_passes_the_breakpoint_once() {
+        let mut gb = gb_with_program(&[]);
+        gb.set_breakpoints(&[0x0200]);
+        gb.run_frame();
+        gb.run_frame();
+        assert_eq!(
+            gb.debug().breakpoint_hit(),
+            None,
+            "직선 코드는 다시 오지 않는다"
+        );
+        assert!(gb.debug().registers().pc > 0x0200);
+    }
+
+    #[test]
+    fn loop_hits_the_breakpoint_every_iteration() {
+        let mut gb = gb_with_program(&[0x18, 0xFE]); // JR -2
+        gb.set_breakpoints(&[0x0100]);
+        gb.run_frame();
+        assert_eq!(gb.debug().breakpoint_hit(), Some(0x0100));
+        assert!(gb.debug().trace().is_empty(), "시작 위치에서 바로 멈춘다");
+        gb.run_frame();
+        assert_eq!(gb.debug().breakpoint_hit(), Some(0x0100));
+        assert_eq!(gb.debug().trace(), [0x0100]);
+    }
+
+    #[test]
+    fn cleared_breakpoints_run_whole_frames() {
+        let mut gb = gb_with_program(&[0x18, 0xFE]);
+        gb.set_breakpoints(&[0x0100]);
+        gb.run_frame();
+        gb.set_breakpoints(&[]);
+        gb.run_frame();
+        assert_eq!(gb.debug().breakpoint_hit(), None);
+        assert_eq!(gb.debug().trace().len(), crate::debugger::TRACE_LEN);
+    }
+
+    #[test]
+    fn step_instruction_runs_one_instruction_ignoring_breakpoints() {
+        let mut gb = gb_with_program(&[0x01, 0x34, 0x12, 0x00]); // LD BC,$1234 ; NOP
+        gb.set_breakpoints(&[0x0103]);
+        gb.step_instruction();
+        assert_eq!(gb.debug().registers().pc, 0x0103);
+        assert_eq!(gb.debug().registers().c, 0x34);
+        gb.step_instruction();
+        assert_eq!(gb.debug().registers().pc, 0x0104);
+        assert_eq!(
+            gb.debug().disassemble(0x0100).text,
+            "LD BC,$1234",
+            "디스어셈블은 게임 메모리를 그대로 읽는다"
+        );
+    }
+
+    #[test]
+    fn step_instruction_while_halted_forever_returns() {
+        // DI ; XOR A ; LDH (IE),A ; HALT — 깨울 인터럽트가 없다
+        let mut gb = gb_with_program(&[0xF3, 0xAF, 0xE0, 0xFF, 0x76]);
+        for _ in 0..4 {
+            gb.step_instruction();
+        }
+        assert!(gb.debug().halted());
+        gb.step_instruction();
+        assert!(gb.debug().halted(), "한 프레임 분량 뒤 돌아온다");
+    }
+
+    #[test]
+    fn breakpoints_survive_loading_a_state() {
+        let mut gb = gb_with_program(&[0x18, 0xFE]);
+        let state = gb.save_state(0);
+        gb.set_breakpoints(&[0x0100]);
+        gb.run_frame();
+        gb.load_state(&state, 0).unwrap();
+        assert_eq!(gb.debug().breakpoint_hit(), None);
+        assert!(gb.debug().trace().is_empty());
+        gb.run_frame();
+        assert_eq!(gb.debug().breakpoint_hit(), Some(0x0100));
+    }
+
+    #[test]
     fn pressed_button_is_visible_through_p1() {
         // LD A,0x10 ; LDH (0x00),A (버튼 줄 선택) ; JR -2
         let mut gb = gb_with_program(&[0x3E, 0x10, 0xE0, 0x00, 0x18, 0xFE]);
```

- [ ] **Step 2: 실패하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace`
Expected: 컴파일 실패, ``cannot find type `Debugger` in this scope``, ``no method named `breakpoint_hit` found for struct `DebugView<'a>` ``

- [ ] **Step 3: 구현 패치를 적용한다**

```diff
--- a/crates/gb-core/src/debugger.rs
+++ b/crates/gb-core/src/debugger.rs
@@ -1,5 +1,81 @@
 //! 디버거 상태: PC 브레이크포인트, 정지 위치, 최근 실행한 PC (스펙 §4.3, §6.3).
 //! 세이브 스테이트에는 넣지 않는다.
+
+/// 최근 실행한 명령을 이만큼 기억한다.
+pub const TRACE_LEN: usize = 16;
+
+#[derive(Debug, Clone)]
+pub struct Debugger {
+    /// 주소마다 1비트. 명령마다 확인하므로 집합 대신 비트맵을 쓴다.
+    breakpoints: Box<[u64; 1024]>,
+    count: usize,
+    /// 브레이크포인트에 걸려 멈춘 PC. 다음 실행은 이 PC의 명령을 한 번 통과시킨다.
+    hit: Option<u16>,
+    trace: [u16; TRACE_LEN],
+    /// 다음에 쓸 자리와 채워진 개수
+    trace_next: usize,
+    trace_len: usize,
+}
+
+impl Default for Debugger {
+    fn default() -> Self {
+        Self {
+            breakpoints: Box::new([0; 1024]),
+            count: 0,
+            hit: None,
+            trace: [0; TRACE_LEN],
+            trace_next: 0,
+            trace_len: 0,
+        }
+    }
+}
+
+impl Debugger {
+    pub fn set_breakpoints(&mut self, pcs: &[u16]) {
+        self.breakpoints.fill(0);
+        for &pc in pcs {
+            self.breakpoints[usize::from(pc >> 6)] |= 1 << (pc & 63);
+        }
+        self.count = self
+            .breakpoints
+            .iter()
+            .map(|w| w.count_ones() as usize)
+            .sum();
+    }
+
+    pub fn is_breakpoint(&self, pc: u16) -> bool {
+        self.count != 0 && self.breakpoints[usize::from(pc >> 6)] & (1 << (pc & 63)) != 0
+    }
+
+    pub fn hit(&self) -> Option<u16> {
+        self.hit
+    }
+
+    pub fn set_hit(&mut self, pc: Option<u16>) {
+        self.hit = pc;
+    }
+
+    pub fn record(&mut self, pc: u16) {
+        self.trace[self.trace_next] = pc;
+        self.trace_next = (self.trace_next + 1) % TRACE_LEN;
+        self.trace_len = (self.trace_len + 1).min(TRACE_LEN);
+    }
+
+    /// 최근 실행한 PC, 오래된 것부터.
+    pub fn trace(&self) -> Vec<u16> {
+        (0..self.trace_len)
+            .map(|i| self.trace[(self.trace_next + TRACE_LEN - self.trace_len + i) % TRACE_LEN])
+            .collect()
+    }
+
+    /// 스테이트를 불러온 뒤: 브레이크포인트는 두고 정지 위치와 실행 흔적은 지운다.
+    pub fn reset_run_state(&mut self) {
+        self.hit = None;
+        self.trace_next = 0;
+        self.trace_len = 0;
+    }
+}
+
 #[cfg(test)]
 mod tests {
     use super::*;
--- a/crates/gb-core/src/gameboy.rs
+++ b/crates/gb-core/src/gameboy.rs
@@ -2,7 +2,8 @@
 
 use crate::bus::Bus;
 use crate::cartridge::{CartError, Cartridge, Header};
-use crate::cpu::{Cpu, IllegalOpcode, Registers};
+use crate::cpu::{Cpu, IllegalOpcode, Registers, disasm};
+use crate::debugger::Debugger;
 use crate::joypad::Button;
 use crate::model::Model;
 use crate::ppu;
@@ -13,6 +14,9 @@
     cpu: Cpu,
     bus: Bus,
     model: Model,
+    /// 브레이크포인트와 실행 흔적. 스테이트에 넣지 않는다.
+    #[serde(skip)]
+    debugger: Debugger,
 }
 
 impl GameBoy {
@@ -24,6 +28,7 @@
             cpu: Cpu::new(Registers::post_boot(model)),
             bus: Bus::new(cart, model),
             model,
+            debugger: Debugger::default(),
         })
     }
 
@@ -37,19 +42,66 @@
 
     /// 명령 하나(또는 인터럽트 디스패치, HALT 대기 1 M-사이클)를 실행한다.
     pub fn step(&mut self) {
+        self.traced_step();
+    }
+
+    /// CPU를 한 번 진행한다. 명령을 실행하는 경우 그 PC를 실행 흔적에 남긴다.
+    fn traced_step(&mut self) {
+        if !self.cpu.halted() && self.cpu.lock().is_none() {
+            self.debugger.record(self.cpu.regs.pc);
+        }
         self.cpu.step(&mut self.bus);
+    }
+
+    /// 다음에 실행할 명령이 브레이크포인트인지. HALT·정지 중에는 명령을 실행하지 않으므로 보지 않는다.
+    fn at_breakpoint(&self) -> bool {
+        !self.cpu.halted()
+            && self.cpu.lock().is_none()
+            && self.debugger.is_breakpoint(self.cpu.regs.pc)
     }
 
     /// 다음 VBlank 진입까지 실행한다. 프레임 경계가 오지 않아도(LCD가 꺼져 있거나 게임이
     /// LCD를 계속 껐다 켜는 경우) 한 프레임 분량(70224 T-사이클)이 지나면 반환한다.
+    ///
+    /// 브레이크포인트 주소의 명령을 실행하기 직전에 멈추고 돌아온다(스펙 §4.3). 그 자리에서 다시
+    /// 부르면 그 명령은 한 번 통과한다.
     pub fn run_frame(&mut self) {
+        let resume = self.debugger.hit();
+        self.debugger.set_hit(None);
         let deadline = self.bus.dots() + u64::from(ppu::DOTS_PER_FRAME);
+        let mut first = true;
         loop {
-            self.cpu.step(&mut self.bus);
+            if self.at_breakpoint() && !(first && resume == Some(self.cpu.regs.pc)) {
+                self.debugger.set_hit(Some(self.cpu.regs.pc));
+                return;
+            }
+            first = false;
+            self.traced_step();
             if self.bus.take_frame_ready() || self.bus.dots() >= deadline {
                 return;
             }
         }
+    }
+
+    /// 디버거의 "명령 1개 실행". 브레이크포인트는 보지 않는다. HALT 중이면 깨어나 다음 명령을 실행할
+    /// 때까지(최대 한 프레임 분량) 진행한다.
+    pub fn step_instruction(&mut self) {
+        self.debugger.set_hit(None);
+        if self.cpu.halted() {
+            let deadline = self.bus.dots() + u64::from(ppu::DOTS_PER_FRAME);
+            while self.cpu.halted() && self.bus.dots() < deadline {
+                self.cpu.step(&mut self.bus);
+            }
+        } else {
+            self.traced_step();
+        }
+        // 단계 실행 중에 지난 VBlank 표시는 버린다. 남겨 두면 다음 `run_frame`이 바로 끝난다.
+        self.bus.take_frame_ready();
+    }
+
+    /// PC 브레이크포인트를 `pcs`로 바꾼다. 빈 목록이면 모두 지운다.
+    pub fn set_breakpoints(&mut self, pcs: &[u16]) {
+        self.debugger.set_breakpoints(pcs);
     }
 
     /// 160×144, 각 픽셀은 0xRRGGBBAA.
@@ -127,6 +179,8 @@
             .bus
             .cartridge_mut()
             .advance_rtc(now_unix.saturating_sub(saved_at));
+        loaded.debugger = std::mem::take(&mut self.debugger);
+        loaded.debugger.reset_run_state();
         *self = loaded;
         Ok(())
     }
@@ -156,6 +210,29 @@
 
     pub fn peek(&self, addr: u16) -> u8 {
         self.gb.bus.peek(addr)
+    }
+
+    pub fn ime(&self) -> bool {
+        self.gb.cpu.ime()
+    }
+
+    pub fn halted(&self) -> bool {
+        self.gb.cpu.halted()
+    }
+
+    /// 마지막 `run_frame`이 브레이크포인트에 걸려 멈췄으면 그 PC.
+    pub fn breakpoint_hit(&self) -> Option<u16> {
+        self.gb.debugger.hit()
+    }
+
+    /// 최근 실행한 명령의 PC, 오래된 것부터 (최대 `debugger::TRACE_LEN`개).
+    pub fn trace(&self) -> Vec<u16> {
+        self.gb.debugger.trace()
+    }
+
+    /// `addr`의 명령을 해석한다. 메모리를 읽어도 게임 상태는 바뀌지 않는다.
+    pub fn disassemble(&self, addr: u16) -> disasm::Instruction {
+        disasm::disassemble(addr, |a| self.peek(a))
     }
 }
 
```

- [ ] **Step 4: 통과하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: core lib 258 passed (+ debugger 2, gameboy 7), clippy 경고 0

- [ ] **Step 5: 커밋한다**

```bash
cargo fmt --all --check
git add -A crates
git commit -m "feat(core): PC 브레이크포인트와 실행 흔적, 명령 단위 실행

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---
### Task 3: 세션 단계 실행

**Files:** `crates/aragorn-app/src/pacing.rs`, `crates/aragorn-app/src/session.rs`

**Interfaces:**
- Consumes: Task 2 `GameBoy::{set_breakpoints, step_instruction}`, `DebugView::breakpoint_hit`
- Produces:
  - `SpeedControl::pause()`
  - `Session::{step_instruction, step_frame, set_breakpoints, breakpoint_hit, debug}`
  - `Session::run`이 브레이크포인트에서 멈춘다.

- [ ] **Step 1: 테스트 패치를 적용한다**

```diff
--- a/crates/aragorn-app/src/pacing.rs
+++ b/crates/aragorn-app/src/pacing.rs
@@ -202,5 +202,8 @@
         assert_eq!(c.mode(true, FastForward::X2), RunMode::Paused);
         c.toggle_pause();
         assert_eq!(c.mode(false, FastForward::X2), RunMode::Normal);
-    }
-}
+        c.pause();
+        c.pause();
+        assert!(c.is_paused(), "pause는 토글이 아니다");
+    }
+}
--- a/crates/aragorn-app/src/session.rs
+++ b/crates/aragorn-app/src/session.rs
@@ -852,6 +852,46 @@
         assert_eq!((rtc[0], rtc[4]), (0, 3), "3분 진행");
     }
 
+    /// 0x0100부터 NOP만 있는 ROM. 한 프레임에 PC가 0x0100에서 약 0x4A00까지 간다.
+    fn nop_rom() -> Vec<u8> {
+        rom_with(0x00, 0x00, &[])
+    }
+
+    #[test]
+    fn breakpoint_stops_running_further_frames() {
+        let (mut session, _) = session_with(nop_rom(), MemoryStore::default());
+        session.set_breakpoints(&[0x0200]);
+        assert_eq!(
+            session.advance(FRAME_DURATION * 3),
+            1,
+            "첫 프레임에서 멈춘다"
+        );
+        assert_eq!(session.breakpoint_hit(), Some(0x0200));
+        assert_eq!(session.debug().registers().pc, 0x0200);
+        let unlimited = RunMode::Fast(FastForward::Unlimited);
+        let (mut session, _) = session_with(nop_rom(), MemoryStore::default());
+        session.set_breakpoints(&[0x0200]);
+        assert_eq!(session.run(Duration::ZERO, unlimited, &mut || true), 1);
+    }
+
+    #[test]
+    fn stepping_runs_one_instruction_or_frame_silently() {
+        let (mut session, _) = session_with(nop_rom(), MemoryStore::default());
+        let mut sink = sink();
+        session.step_instruction();
+        assert_eq!(session.debug().registers().pc, 0x0101);
+        session.step_frame();
+        assert!(session.debug().registers().pc > 0x1000);
+        session.pump_audio(&mut sink);
+        assert!(
+            sink.received.is_empty(),
+            "단계 실행 중에는 소리를 내지 않는다"
+        );
+        session.set_breakpoints(&[0x6000]);
+        session.step_frame();
+        assert_eq!(session.breakpoint_hit(), Some(0x6000));
+    }
+
     /// MBC3+TIMER+RAM+BATTERY(0x10) 판 `saving_rom(true)`.
     fn rtc_saving_rom() -> Vec<u8> {
         let mut rom = saving_rom(true);
```

- [ ] **Step 2: 실패하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace`
Expected: 컴파일 실패, ``no method named `set_breakpoints` found for struct `session::Session` ``, ``no method named `pause` found for struct `pacing::SpeedControl` ``

- [ ] **Step 3: 구현 패치를 적용한다**

```diff
--- a/crates/aragorn-app/src/pacing.rs
+++ b/crates/aragorn-app/src/pacing.rs
@@ -75,6 +75,11 @@
         self.paused = !self.paused;
     }
 
+    /// 일시정지한다 (디버거의 브레이크포인트·단계 실행).
+    pub fn pause(&mut self) {
+        self.paused = true;
+    }
+
     pub fn is_paused(&self) -> bool {
         self.paused
     }
--- a/crates/aragorn-app/src/session.rs
+++ b/crates/aragorn-app/src/session.rs
@@ -2,7 +2,7 @@
 
 use crate::audio::{self, AudioSink};
 use crate::pacing::{FramePacer, RunMode, UNLIMITED_MAX_FRAMES};
-use gb_core::{Button, CartError, GameBoy, Model};
+use gb_core::{Button, CartError, DebugView, GameBoy, Model};
 use std::{io, time::Duration};
 
 /// 게임이 외부 RAM을 켠 채로 두어도, 마지막 쓰기 뒤 이만큼(약 1초) 쓰기가 없으면 저장한다.
@@ -174,22 +174,55 @@
                     self.pacer.reset();
                     let mut frames = 0;
                     while frames == 0 || (frames < UNLIMITED_MAX_FRAMES && keep_going()) {
-                        self.run_one_frame();
                         frames += 1;
+                        if !self.run_one_frame() {
+                            break;
+                        }
                     }
                     return frames;
                 }
             },
         };
-        for _ in 0..frames {
-            self.run_one_frame();
+        for done in 1..=frames {
+            if !self.run_one_frame() {
+                return done;
+            }
         }
         frames
     }
 
-    fn run_one_frame(&mut self) {
+    /// 한 프레임을 돌린다. 브레이크포인트에 걸려 멈췄으면 `false`.
+    fn run_one_frame(&mut self) -> bool {
         self.gb.run_frame();
         self.track_battery();
+        self.gb.debug().breakpoint_hit().is_none()
+    }
+
+    /// 디버거: 명령 하나를 실행한다(브레이크포인트는 보지 않는다). 소리는 내지 않는다.
+    pub fn step_instruction(&mut self) {
+        self.muted = true;
+        self.gb.step_instruction();
+    }
+
+    /// 디버거: 프레임 하나를 실행한다. 브레이크포인트에 걸리면 그 자리에서 멈춘다. 소리는 내지 않는다.
+    pub fn step_frame(&mut self) {
+        self.muted = true;
+        self.run_one_frame();
+    }
+
+    /// PC 브레이크포인트를 바꾼다.
+    pub fn set_breakpoints(&mut self, pcs: &[u16]) {
+        self.gb.set_breakpoints(pcs);
+    }
+
+    /// 마지막 실행이 브레이크포인트에 걸려 멈췄으면 그 PC.
+    pub fn breakpoint_hit(&self) -> Option<u16> {
+        self.gb.debug().breakpoint_hit()
+    }
+
+    /// 디버거가 읽는 코어 상태.
+    pub fn debug(&self) -> DebugView<'_> {
+        self.gb.debug()
     }
 
     pub fn set_button(&mut self, button: Button, pressed: bool) {
```

- [ ] **Step 4: 통과하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: app 104 passed (+ session 2; pacing 테스트는 단언만 늘어남), clippy 경고 0

- [ ] **Step 5: 커밋한다**

```bash
cargo fmt --all --check
git add -A crates
git commit -m "feat(app): 세션 단계 실행과 브레이크포인트

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---
### Task 4: 디버거 창

**Files:** `crates/aragorn-desktop/src/ui/mod.rs`, `crates/aragorn-desktop/src/ui/debugger.rs`, `crates/aragorn-desktop/src/app.rs`

**Interfaces:**
- Consumes: Task 3 `Session` 디버거 API, `SpeedControl::pause`
- Produces:
  - `ui::debugger::{DebuggerView, DebugCommand, parse_address, hex_row, flags_text, toggle_breakpoint}`
  - 툴바 "디버거"와 CPU·메모리 창
  - 브레이크포인트에서 자동 일시정지

- [ ] **Step 1: 테스트 패치를 적용한다**

```diff
--- a/crates/aragorn-desktop/src/ui/mod.rs
+++ b/crates/aragorn-desktop/src/ui/mod.rs
@@ -1,3 +1,4 @@
+pub mod debugger;
 pub mod fonts;
 pub mod input;
 pub mod screen;
--- /dev/null
+++ b/crates/aragorn-desktop/src/ui/debugger.rs
@@ -0,0 +1,42 @@
+//! 디버거 창: CPU(레지스터·플래그·디스어셈블리·실행 제어·브레이크포인트)와 메모리 뷰어 (스펙 §6.3).
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn addresses_accept_common_hex_forms() {
+        assert_eq!(parse_address("0150"), Some(0x0150));
+        assert_eq!(parse_address(" $c000 "), Some(0xC000));
+        assert_eq!(parse_address("0xFF44"), Some(0xFF44));
+        assert_eq!(parse_address("ff"), Some(0x00FF));
+        assert_eq!(parse_address(""), None);
+        assert_eq!(parse_address("10000"), None);
+        assert_eq!(parse_address("zz"), None);
+    }
+
+    #[test]
+    fn hex_row_shows_bytes_and_printable_ascii() {
+        let bytes: Vec<u8> = (0x40..0x50).collect();
+        assert_eq!(
+            hex_row(0xC000, &bytes),
+            "C000  40 41 42 43 44 45 46 47 48 49 4A 4B 4C 4D 4E 4F  @ABCDEFGHIJKLMNO"
+        );
+        assert!(hex_row(0, &[0x00, 0x7F, 0xFF]).ends_with("..."));
+    }
+
+    #[test]
+    fn flags_show_set_bits() {
+        assert_eq!(flags_text(0xB0), "Z-HC");
+        assert_eq!(flags_text(0x00), "----");
+        assert_eq!(flags_text(0xF0), "ZNHC");
+    }
+
+    #[test]
+    fn toggling_keeps_breakpoints_sorted_and_unique() {
+        let mut list = Vec::new();
+        for pc in [0x0200, 0x0100, 0x0300, 0x0100] {
+            toggle_breakpoint(&mut list, pc);
+        }
+        assert_eq!(list, [0x0200, 0x0300]);
+    }
+}
--- a/crates/aragorn-desktop/src/app.rs
+++ b/crates/aragorn-desktop/src/app.rs
@@ -520,6 +520,25 @@
     }
 
     #[test]
+    fn text_field_keeps_focus_while_typing() {
+        let ctx = egui::Context::default();
+        let mut text = String::new();
+        let id = egui::Id::new("address");
+        // 첫 화면 갱신에서 입력칸을 누른 것처럼 포커스를 주고, 이후 갱신마다 앱처럼 포커스를 정리한다.
+        for frame in 0..3 {
+            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
+                release_widget_focus(ui.ctx());
+                let response = ui.add(egui::TextEdit::singleline(&mut text).id(id));
+                if frame == 0 {
+                    response.request_focus();
+                }
+            });
+            output.textures_delta.clear();
+        }
+        assert!(ctx.memory(|m| m.has_focus(id)), "입력칸 포커스는 남는다");
+    }
+
+    #[test]
     fn tab_then_enter_does_not_press_toolbar_button() {
         // 막지 않으면 Tab(배속)이 버튼에 포커스를 주고 Enter(Start)가 버튼을 누른다.
         let ctx = egui::Context::default();
```

- [ ] **Step 2: 실패하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace`
Expected: 컴파일 실패, ``cannot find function `parse_address` in this scope``

- [ ] **Step 3: 구현 패치를 적용한다**

```diff
--- a/crates/aragorn-desktop/src/ui/debugger.rs
+++ b/crates/aragorn-desktop/src/ui/debugger.rs
@@ -1,4 +1,288 @@
 //! 디버거 창: CPU(레지스터·플래그·디스어셈블리·실행 제어·브레이크포인트)와 메모리 뷰어 (스펙 §6.3).
+
+use aragorn_app::session::Session;
+use eframe::egui;
+use gb_core::DebugView;
+
+/// 디스어셈블리에서 PC 앞에 보일 최근 실행 명령 수.
+const TRACE_SHOWN: usize = 4;
+/// PC부터 보일 명령 수.
+const AHEAD_SHOWN: usize = 20;
+const MEMORY_ROWS: usize = 0x10000 / 16;
+
+/// 디버거 창이 앱에 요청하는 일.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub enum DebugCommand {
+    TogglePause,
+    StepInstruction,
+    StepFrame,
+    SetBreakpoints(Vec<u16>),
+}
+
+/// `0150`, `$0150`, `0x0150`, `c000` 같은 16진수 주소. 공백은 무시한다.
+pub fn parse_address(text: &str) -> Option<u16> {
+    let t = text.trim();
+    let t = t
+        .strip_prefix('$')
+        .or_else(|| t.strip_prefix("0x"))
+        .or_else(|| t.strip_prefix("0X"))
+        .unwrap_or(t);
+    if t.is_empty() || t.len() > 4 {
+        return None;
+    }
+    u16::from_str_radix(t, 16).ok()
+}
+
+/// 메모리 한 줄: `C000  00 11 22 … FF  ..."...`
+pub fn hex_row(addr: u16, bytes: &[u8]) -> String {
+    let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02X}")).collect();
+    let ascii: String = bytes
+        .iter()
+        .map(|&b| {
+            if (0x20..0x7F).contains(&b) {
+                b as char
+            } else {
+                '.'
+            }
+        })
+        .collect();
+    format!("{addr:04X}  {}  {ascii}", hex.join(" "))
+}
+
+/// F 레지스터의 플래그: 켜진 것은 글자, 꺼진 것은 `-` (예: `Z-H-`).
+pub fn flags_text(f: u8) -> String {
+    [(0x80, 'Z'), (0x40, 'N'), (0x20, 'H'), (0x10, 'C')]
+        .iter()
+        .map(|&(bit, c)| if f & bit != 0 { c } else { '-' })
+        .collect()
+}
+
+/// 목록에 있으면 빼고 없으면 넣는다. 목록은 정렬해 둔다.
+pub fn toggle_breakpoint(list: &mut Vec<u16>, pc: u16) {
+    match list.binary_search(&pc) {
+        Ok(i) => {
+            list.remove(i);
+        }
+        Err(i) => list.insert(i, pc),
+    }
+}
+
+#[derive(Default)]
+pub struct DebuggerView {
+    pub open: bool,
+    breakpoints: Vec<u16>,
+    new_breakpoint: String,
+    memory_address: String,
+    /// 다음에 메모리 창을 그릴 때 이 줄로 스크롤한다.
+    memory_jump: Option<usize>,
+}
+
+impl DebuggerView {
+    pub fn breakpoints(&self) -> &[u16] {
+        &self.breakpoints
+    }
+
+    /// 창들을 그리고, 사용자가 요청한 일을 돌려준다.
+    pub fn show(
+        &mut self,
+        ctx: &egui::Context,
+        session: &Session,
+        paused: bool,
+    ) -> Vec<DebugCommand> {
+        let mut commands = Vec::new();
+        if !self.open {
+            return commands;
+        }
+        let debug = session.debug();
+        let mut open = self.open;
+        egui::Window::new("CPU")
+            .open(&mut open)
+            .default_width(360.0)
+            .show(ctx, |ui| self.cpu_window(ui, &debug, paused, &mut commands));
+        egui::Window::new("메모리")
+            .open(&mut open)
+            .default_width(480.0)
+            .show(ctx, |ui| self.memory_window(ui, &debug));
+        self.open = open;
+        commands
+    }
+
+    fn cpu_window(
+        &mut self,
+        ui: &mut egui::Ui,
+        debug: &DebugView<'_>,
+        paused: bool,
+        commands: &mut Vec<DebugCommand>,
+    ) {
+        ui.horizontal(|ui| {
+            if ui
+                .button(if paused { "계속" } else { "일시정지" })
+                .clicked()
+            {
+                commands.push(DebugCommand::TogglePause);
+            }
+            if ui.button("명령 1개").clicked() {
+                commands.push(DebugCommand::StepInstruction);
+            }
+            if ui.button("프레임 1개").clicked() {
+                commands.push(DebugCommand::StepFrame);
+            }
+        });
+        let r = debug.registers();
+        if let Some(pc) = debug.breakpoint_hit() {
+            ui.colored_label(
+                ui.visuals().warn_fg_color,
+                format!("브레이크포인트 ${pc:04X}에서 멈춤"),
+            );
+        }
+        if let Some(lock) = debug.illegal_opcode() {
+            ui.colored_label(
+                ui.visuals().error_fg_color,
+                format!(
+                    "잘못된 옵코드 ${:02X} (${:04X})에서 CPU 정지",
+                    lock.opcode, lock.pc
+                ),
+            );
+        }
+        ui.separator();
+        let mono = |ui: &mut egui::Ui, text: String| {
+            ui.label(egui::RichText::new(text).monospace());
+        };
+        mono(
+            ui,
+            format!(
+                "AF={:02X}{:02X} BC={:02X}{:02X} DE={:02X}{:02X} HL={:02X}{:02X}",
+                r.a, r.f, r.b, r.c, r.d, r.e, r.h, r.l
+            ),
+        );
+        mono(
+            ui,
+            format!(
+                "SP={:04X} PC={:04X} 플래그 {} IME {} HALT {}",
+                r.sp,
+                r.pc,
+                flags_text(r.f),
+                u8::from(debug.ime()),
+                u8::from(debug.halted())
+            ),
+        );
+        ui.separator();
+        // 최근 실행한 명령(흐리게) + PC부터의 명령
+        let trace = debug.trace();
+        let recent: Vec<u16> = trace
+            .iter()
+            .rev()
+            .skip_while(|&&pc| pc == r.pc)
+            .take(TRACE_SHOWN)
+            .copied()
+            .collect();
+        let mut rows: Vec<(u16, bool)> = recent.into_iter().rev().map(|pc| (pc, false)).collect();
+        let mut addr = r.pc;
+        for _ in 0..AHEAD_SHOWN {
+            rows.push((addr, true));
+            addr = addr.wrapping_add(u16::from(debug.disassemble(addr).len));
+        }
+        let mut changed = false;
+        egui::Grid::new("disassembly")
+            .spacing([6.0, 2.0])
+            .show(ui, |ui| {
+                for (pc, ahead) in rows {
+                    let is_bp = self.breakpoints.binary_search(&pc).is_ok();
+                    if ui
+                        .selectable_label(is_bp, if is_bp { "●" } else { "○" })
+                        .on_hover_text("브레이크포인트 켜기/끄기")
+                        .clicked()
+                    {
+                        toggle_breakpoint(&mut self.breakpoints, pc);
+                        changed = true;
+                    }
+                    let instruction = debug.disassemble(pc);
+                    let bytes: Vec<String> = (0..instruction.len)
+                        .map(|i| format!("{:02X}", debug.peek(pc.wrapping_add(u16::from(i)))))
+                        .collect();
+                    let marker = if pc == r.pc { "▶" } else { " " };
+                    let text = egui::RichText::new(format!(
+                        "{marker} {pc:04X}  {:<8}  {}",
+                        bytes.join(" "),
+                        instruction.text
+                    ))
+                    .monospace();
+                    if ahead {
+                        ui.label(text);
+                    } else {
+                        ui.label(text.weak());
+                    }
+                    ui.end_row();
+                }
+            });
+        ui.separator();
+        ui.horizontal(|ui| {
+            ui.label("브레이크포인트");
+            let edit = ui.add(
+                egui::TextEdit::singleline(&mut self.new_breakpoint)
+                    .hint_text("0150")
+                    .desired_width(60.0),
+            );
+            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
+            if (ui.button("추가").clicked() || enter)
+                && let Some(pc) = parse_address(&self.new_breakpoint)
+            {
+                if self.breakpoints.binary_search(&pc).is_err() {
+                    toggle_breakpoint(&mut self.breakpoints, pc);
+                    changed = true;
+                }
+                self.new_breakpoint.clear();
+            }
+        });
+        let mut remove = None;
+        for &pc in &self.breakpoints {
+            ui.horizontal(|ui| {
+                ui.label(egui::RichText::new(format!("${pc:04X}")).monospace());
+                if ui.small_button("✕").clicked() {
+                    remove = Some(pc);
+                }
+            });
+        }
+        if let Some(pc) = remove {
+            toggle_breakpoint(&mut self.breakpoints, pc);
+            changed = true;
+        }
+        if changed {
+            commands.push(DebugCommand::SetBreakpoints(self.breakpoints.clone()));
+        }
+    }
+
+    fn memory_window(&mut self, ui: &mut egui::Ui, debug: &DebugView<'_>) {
+        ui.horizontal(|ui| {
+            ui.label("주소");
+            let edit = ui.add(
+                egui::TextEdit::singleline(&mut self.memory_address)
+                    .hint_text("C000")
+                    .desired_width(60.0),
+            );
+            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
+            if (ui.button("이동").clicked() || enter)
+                && let Some(addr) = parse_address(&self.memory_address)
+            {
+                self.memory_jump = Some(usize::from(addr) / 16);
+            }
+        });
+        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
+        let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
+        if let Some(row) = self.memory_jump.take() {
+            let spacing = ui.spacing().item_spacing.y;
+            area = area.vertical_scroll_offset(row as f32 * (row_height + spacing));
+        }
+        area.show_rows(ui, row_height, MEMORY_ROWS, |ui, range| {
+            for row in range {
+                let addr = (row * 16) as u16;
+                let bytes: Vec<u8> = (0..16).map(|i| debug.peek(addr + i)).collect();
+                ui.label(egui::RichText::new(hex_row(addr, &bytes)).monospace());
+            }
+        });
+    }
+}
+
 #[cfg(test)]
 mod tests {
     use super::*;
--- a/crates/aragorn-desktop/src/app.rs
+++ b/crates/aragorn-desktop/src/app.rs
@@ -2,6 +2,7 @@
     adapters::{CpalAudio, FsSaveStore, Gamepads, SystemClock, UnavailableUpdater},
     build_info,
     ui::{
+        debugger::{DebugCommand, DebuggerView},
         input::{self, InputFrame, Keymap},
         screen::ScreenView,
         settings::SettingsView,
@@ -51,7 +52,11 @@
 
 /// 위젯의 키보드 포커스를 없앤다. 게임 키(Tab=배속, 방향키, Enter=Start)가 egui 포커스 이동과
 /// 버튼 누르기로 새지 않게 설정 창이 닫혀 있을 때 매 화면 갱신마다 부른다.
+/// 글자 입력칸(디버거의 주소 입력)의 포커스는 남긴다. 그동안 키보드는 게임에 가지 않는다.
 pub fn release_widget_focus(ctx: &egui::Context) {
+    if ctx.text_edit_focused() {
+        return;
+    }
     ctx.memory_mut(|m| {
         if let Some(id) = m.focused() {
             m.surrender_focus(id);
@@ -108,6 +113,7 @@
     mode: RunMode,
     settings: SettingsView,
     slots: SlotsView,
+    debugger: DebuggerView,
 }
 
 impl AragornApp {
@@ -150,6 +156,7 @@
             mode: RunMode::Normal,
             settings: SettingsView::default(),
             slots: SlotsView::default(),
+            debugger: DebuggerView::default(),
         };
         if app.worker.is_some() {
             app.dispatch(UpdateEvent::CheckRequested);
@@ -176,6 +183,8 @@
                 self.screen.update(ctx, session.framebuffer());
                 self.notice = Some(format!("{} 실행 중", session.title()));
                 self.slots.invalidate();
+                let mut session = session;
+                session.set_breakpoints(self.debugger.breakpoints());
                 self.session = Some(session);
                 self.last_tick = Instant::now();
                 self.collect_session_errors();
@@ -215,10 +224,12 @@
             }
             return None;
         }
+        // 글자 입력칸에 입력하는 동안 키보드는 게임에 넘기지 않는다 (게임패드는 그대로).
+        let typing = ctx.text_edit_focused();
         Some(input::gather(
             &self.keymap,
-            |key| ctx.input(|i| i.key_down(key)),
-            |key| ctx.input(|i| input::pressed_once(&i.events, key)),
+            |key| !typing && ctx.input(|i| i.key_down(key)),
+            |key| !typing && ctx.input(|i| input::pressed_once(&i.events, key)),
             &pad.input,
         ))
     }
@@ -254,6 +265,31 @@
         self.collect_session_errors();
     }
 
+    /// 디버거 창의 요청을 처리한다. 단계 실행은 일시정지한 상태에서 한다. 종료 중이거나 강제 업데이트
+    /// 중에는 하지 않는다.
+    fn run_debug_command(&mut self, ctx: &egui::Context, command: DebugCommand) {
+        if self.exit_handled || self.flow.blocks_emulation() {
+            return;
+        }
+        let Some(session) = &mut self.session else {
+            return;
+        };
+        match command {
+            DebugCommand::TogglePause => self.speed.toggle_pause(),
+            DebugCommand::StepInstruction | DebugCommand::StepFrame => {
+                self.speed.pause();
+                if command == DebugCommand::StepInstruction {
+                    session.step_instruction();
+                } else {
+                    session.step_frame();
+                }
+                self.screen.update(ctx, session.framebuffer());
+            }
+            DebugCommand::SetBreakpoints(pcs) => session.set_breakpoints(&pcs),
+        }
+        self.collect_session_errors();
+    }
+
     fn input_config_changed(&mut self) {
         self.keymap = Keymap::new(&self.config.input.keyboard);
         self.save_config();
@@ -286,6 +322,14 @@
         let mut within_budget = || started.elapsed() < UNLIMITED_BUDGET;
         if session.run(elapsed, self.mode, &mut within_budget) > 0 {
             self.screen.update(ctx, session.framebuffer());
+        }
+        if self.mode != RunMode::Paused
+            && let Some(pc) = session.breakpoint_hit()
+        {
+            self.speed.pause();
+            self.mode = RunMode::Paused;
+            self.debugger.open = true;
+            self.notice = Some(format!("브레이크포인트 ${pc:04X}에서 멈췄습니다"));
         }
         if let Some(sink) = self.audio.sink() {
             session.pump_audio(sink);
@@ -387,6 +431,7 @@
         }
         let input = self.gather_input(ctx);
         if input.is_some()
+            && !ctx.text_edit_focused()
             && let Some(command) = ctx.input(|i| state_slots::hotkey(&i.events))
         {
             self.run_slot_command(ctx, command);
@@ -405,6 +450,9 @@
             ui.horizontal(|ui| {
                 if ui.button("ROM 열기").clicked() {
                     open_requested = true;
+                }
+                if ui.button("디버거").clicked() {
+                    self.debugger.open = !self.debugger.open;
                 }
                 if ui.button("설정").clicked() {
                     self.settings.open = !self.settings.open;
@@ -469,6 +517,16 @@
             }
         });
 
+        let debug_commands = match &self.session {
+            Some(session) => self
+                .debugger
+                .show(ui.ctx(), session, self.speed.is_paused()),
+            None => Vec::new(),
+        };
+        for command in debug_commands {
+            let ctx = ui.ctx().clone();
+            self.run_debug_command(&ctx, command);
+        }
         if let Some(command) = slot_command {
             let ctx = ui.ctx().clone();
             self.run_slot_command(&ctx, command);
```

- [ ] **Step 4: 통과하는지 확인한다**

Run: `ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: desktop 76 passed (+ debugger 4, app 1), clippy 경고 0

- [ ] **Step 5: 커밋한다**

```bash
cargo fmt --all --check
git add -A crates
git commit -m "feat(desktop): 디버거 CPU·메모리 창

Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>"
```

---
### Task 5: 전체 확인, PR

- [ ] **Step 1: 전체 검사를 실행한다**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && ARAGORN_REQUIRE_TEST_ROMS=1 cargo test --workspace --no-fail-fast 2>&1 | grep "test result" | grep -v " 0 passed"`
Expected: 모든 결과가 `ok`다.

| 테스트 | 개수 |
|---|---|
| gb-core lib | 258 |
| acid2 (DMG) | 1 |
| blargg | 15 |
| cgb_acid2 | 1 |
| cgb_sound | 12 |
| dmg_sound | 12 |
| mooneye | 77 |
| save_state | 14 |
| app | 104 |
| desktop | 76 |
| xtask | 14 |

- [ ] **Step 2: PR을 만들고 CI를 확인한다** (외부 공개 작업이므로 사용자 확인 후)

```bash
git push -u origin feat/m8a-debugger
gh pr create --repo dongjay00/aragorn --base main --title "M8a: 디버거 (CPU·실행 제어·메모리)" --body-file <작성한 본문>
gh pr checks --watch
```

- [ ] **Step 3: 사용 확인을 요청한다** (사용자 작업, 릴리스 뒤)

사용자에게 아래 확인을 요청한다.
- 툴바 "디버거"를 누르면 CPU 창과 메모리 창이 뜬다. 게임 화면은 그대로 보인다.
- CPU 창에서 "일시정지" 뒤 "명령 1개"를 누르면 PC와 디스어셈블리가 한 줄씩 나아간다. "프레임 1개"를 누르면 화면이 한 프레임 나아간다.
- 디스어셈블리 줄 왼쪽 ○를 눌러 브레이크포인트를 걸면 그 주소에서 멈추고 상태 표시줄에 알린다. "계속"을 누르면 이어서 진행한다.
- 메모리 창에 `C000`을 입력하고 이동하면 그 주소로 간다. 입력하는 동안 게임이 키보드에 반응하지 않는다.
