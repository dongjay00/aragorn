//! 세이브 스테이트 인수 테스트 (스펙 §4.8). 테스트 ROM을 실행하다 저장하고, 그대로 계속한 결과와
//! 불러와서 계속한 결과가 화면·소리 모두 비트 단위로 같아야 한다. 상태 하나라도 빠지면 어긋난다.

mod common;

use gb_core::{GameBoy, Model};

const SAMPLE_RATE: f64 = 48_000.0;
const SAVED_AT: u64 = 1_000;

/// `frames` 프레임을 돌리며 프레임마다의 화면, 전체 소리, 끝난 뒤의 전체 상태를 모은다.
fn record(gb: &mut GameBoy, frames: u32) -> (Vec<Vec<u32>>, Vec<u32>, Vec<u8>) {
    let mut screens = Vec::new();
    let mut audio = Vec::new();
    for _ in 0..frames {
        gb.run_frame();
        screens.push(gb.framebuffer().to_vec());
        gb.drain_audio(&mut audio);
    }
    let audio = audio.iter().map(|s| s.to_bits()).collect();
    (screens, audio, gb.save_state(SAVED_AT))
}

fn first_difference<T: PartialEq>(a: &[T], b: &[T]) -> Option<usize> {
    a.iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .or_else(|| (a.len() != b.len()).then_some(a.len().min(b.len())))
}

/// 위치를 기억하는 아주 작은 어셈블러. 상대 점프만 라벨로 계산한다.
struct Asm(Vec<u8>);

impl Asm {
    fn op(&mut self, bytes: &[u8]) -> &mut Self {
        self.0.extend_from_slice(bytes);
        self
    }
    fn here(&self) -> usize {
        self.0.len()
    }
    /// `opcode`(JR 계열) 뒤에 `target`까지의 상대 거리.
    fn jr(&mut self, opcode: u8, target: usize) -> &mut Self {
        let offset = target as isize - (self.here() as isize + 2);
        self.op(&[opcode, offset as i8 as u8])
    }
    /// LD A,value ; LDH (reg),A
    fn set(&mut self, reg: u8, value: u8) -> &mut Self {
        self.op(&[0x3E, value, 0xE0, reg])
    }
    /// `start`부터 `count`바이트(0이면 256)를 L xor H 무늬로 채운다.
    fn fill(&mut self, start: u16, count: u8) -> &mut Self {
        let [lo, hi] = start.to_le_bytes();
        self.op(&[0x21, lo, hi, 0x06, count]); // LD HL,start; LD B,count
        let top = self.here();
        self.op(&[0x7D, 0xAC, 0x22, 0x05]); // LD A,L; XOR H; LD (HL+),A; DEC B
        self.jr(0x20, top)
    }
}

/// 스테이트에 들어가야 하는 하드웨어 상태를 한꺼번에 쓰는 합성 ROM.
/// - 소리: 네 채널 모두 길이·엔벨로프·스윕을 켜고 약 60ms마다 다시 울린다.
/// - 화면: 무늬 타일, 배경·윈도우·스프라이트, 타이머 인터럽트마다 SCX를, 루프마다 SCY·WX를 바꾼다.
/// - CGB: 팔레트, 2배속, 루프마다 WRAM 뱅크를 바꿔 값을 쓰고 그 WRAM에서 HBlank HDMA를 다시 건다.
fn stress_rom(cgb: bool) -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    rom[0x0100..0x0104].copy_from_slice(&[0x00, 0xC3, 0x50, 0x01]); // NOP; JP 0x0150
    rom[0x0134..0x013F].copy_from_slice(b"STATESTRESS");
    rom[0x0143] = if cgb { 0x80 } else { 0x00 };
    // 타이머 인터럽트: PUSH AF; LDH A,(SCX); INC A; LDH (SCX),A; POP AF; RETI
    rom[0x0050..0x0058].copy_from_slice(&[0xF5, 0xF0, 0x43, 0x3C, 0xE0, 0x43, 0xF1, 0xD9]);

    let mut a = Asm(Vec::new());
    a.op(&[0xF3]).set(0x40, 0x00); // DI; LCD 끄기
    for page in 0..32u16 {
        a.fill(0x8000 + page * 0x100, 0); // 타일 데이터와 두 타일맵
    }
    a.fill(0xFE00, 0xA0); // OAM
    a.fill(0xFF30, 0x10); // 파형 RAM
    if cgb {
        a.set(0x68, 0x80).set(0x6A, 0x80); // BCPS·OCPS 자동 증가
        a.op(&[0x06, 0x40]); // LD B,64
        let top = a.here();
        a.op(&[0x78, 0xE0, 0x69, 0x2F, 0xE0, 0x6B, 0x05]); // LD A,B; LDH (BCPD),A; CPL; LDH (OCPD),A; DEC B
        a.jr(0x20, top);
        // WRAM 뱅크 1–7의 0xD000–0xD7FF를 뱅크마다 다른 무늬(L xor H xor 뱅크)로 채운다
        a.op(&[0x0E, 0x07]); // LD C,7
        let bank = a.here();
        a.op(&[0x79, 0xE0, 0x70, 0x21, 0x00, 0xD0, 0x16, 0x08]); // LD A,C; LDH (SVBK),A; LD HL,D000; LD D,8
        let page = a.here();
        a.op(&[0x06, 0x00]); // LD B,0
        let byte = a.here();
        a.op(&[0x7D, 0xA9, 0xAC, 0x22, 0x05]); // LD A,L; XOR C; XOR H; LD (HL+),A; DEC B
        a.jr(0x20, byte);
        a.op(&[0x15]).jr(0x20, page); // DEC D
        a.op(&[0x0D]).jr(0x20, bank); // DEC C
        a.set(0x4D, 0x01).op(&[0x10, 0x00]); // KEY1 준비; STOP → 2배속
    }
    // 소리
    a.set(0x26, 0x80).set(0x24, 0x77).set(0x25, 0xFF);
    a.set(0x10, 0x19)
        .set(0x11, 0x80)
        .set(0x12, 0xF3)
        .set(0x13, 0x00);
    a.set(0x16, 0x41).set(0x17, 0x82).set(0x18, 0x80);
    a.set(0x1A, 0x80)
        .set(0x1B, 0x00)
        .set(0x1C, 0x20)
        .set(0x1D, 0x40);
    a.set(0x20, 0x00).set(0x21, 0xF1).set(0x22, 0x55);
    // 화면: BGP, OBP0, WY, WX, LCDC(켜기, 윈도우 맵 9C00, 윈도우, 타일 8000, OBJ, BG)
    a.set(0x47, 0xE4)
        .set(0x48, 0xD2)
        .set(0x4A, 0x20)
        .set(0x4B, 0x30);
    a.set(0x40, 0xF3);
    // 타이머 인터럽트
    a.set(0x07, 0x05).set(0xFF, 0x04).op(&[0xFB]); // TAC; IE=타이머; EI

    let main = a.here();
    a.op(&[0x0E, 0x10]); // LD C,16
    let outer = a.here();
    a.op(&[0x06, 0x00]); // LD B,0
    let inner = a.here();
    a.op(&[0x05]).jr(0x20, inner); // DEC B; JR NZ
    a.op(&[0x0D]).jr(0x20, outer); // DEC C; JR NZ
    a.set(0x14, 0xC7)
        .set(0x19, 0xC6)
        .set(0x1E, 0xC5)
        .set(0x23, 0xC0); // 네 채널 다시 울리기
    a.op(&[0xF0, 0x42, 0x3C, 0xE0, 0x42]); // SCY++
    a.op(&[0xF0, 0x4B, 0x3C, 0xE0, 0x4B]); // WX++
    if cgb {
        // SVBK = (SVBK + 1) & 7; LD (0xD000),A — 뱅크마다 다른 값을 남긴다
        a.op(&[0xF0, 0x70, 0x3C, 0xE6, 0x07, 0xE0, 0x70, 0xEA, 0x00, 0xD0]);
        // 0xD000(바뀌는 WRAM)에서 VRAM 0x8800으로 HBlank HDMA 128블록
        a.set(0x51, 0xD0)
            .set(0x52, 0x00)
            .set(0x53, 0x08)
            .set(0x54, 0x00)
            .set(0x55, 0xFF);
    }
    a.jr(0x18, main);

    rom[0x0150..0x0150 + a.0.len()].copy_from_slice(&a.0);
    rom
}

/// `rom`을 `warmup` 프레임 돌린 뒤 저장하고 이어서 비교한다 ([`resumes_identically`]와 같다).
fn rom_resumes_identically(name: &str, rom: Vec<u8>, model: Model, warmup: u32, frames: u32) {
    compare_resume(name, rom, model, warmup, frames);
}

/// `warmup` 프레임 뒤에 저장하고, 이어서 `frames` 프레임을 세 번 돌려 비교한다:
/// 그대로 계속, 같은 인스턴스에 불러와서 계속, 새 인스턴스에 불러와서 계속.
fn resumes_identically(rel: &str, model: Model, warmup: u32, frames: u32) {
    let Some(rom) = common::load_rom(rel) else {
        return;
    };
    compare_resume(rel, rom, model, warmup, frames);
}

fn compare_resume(rel: &str, rom: Vec<u8>, model: Model, warmup: u32, frames: u32) {
    let mut gb = GameBoy::new(rom.clone(), model).expect("테스트 ROM 로드");
    gb.set_sample_rate(SAMPLE_RATE);
    for _ in 0..warmup {
        gb.run_frame();
    }
    gb.drain_audio(&mut Vec::new());
    let state = gb.save_state(SAVED_AT);

    let expected = record(&mut gb, frames);

    gb.load_state(&state, SAVED_AT)
        .expect("같은 인스턴스에 불러오기");
    let same = record(&mut gb, frames);

    let mut fresh = GameBoy::new(rom, model).expect("테스트 ROM 로드");
    fresh.set_sample_rate(SAMPLE_RATE);
    fresh
        .load_state(&state, SAVED_AT)
        .expect("새 인스턴스에 불러오기");
    let other = record(&mut fresh, frames);

    for (name, got) in [("같은 인스턴스", same), ("새 인스턴스", other)] {
        assert!(
            got.0 == expected.0,
            "{rel} ({name}): {}번째 프레임 화면이 다릅니다",
            first_difference(&got.0, &expected.0).unwrap_or(0)
        );
        assert!(
            got.1 == expected.1,
            "{rel} ({name}): 소리 샘플 {}번째부터 다릅니다 (길이 {} / {})",
            first_difference(&got.1, &expected.1).unwrap_or(0),
            got.1.len(),
            expected.1.len()
        );
        assert!(
            got.2 == expected.2,
            "{rel} ({name}): 끝난 뒤 상태가 {}번째 바이트부터 다릅니다",
            first_difference(&got.2, &expected.2).unwrap_or(0)
        );
    }
}

#[test]
fn dmg_acid2_ppu() {
    resumes_identically("dmg-acid2/dmg-acid2.gb", Model::Dmg, 2, 30);
}

#[test]
fn cgb_acid2_ppu() {
    resumes_identically("cgb-acid2/cgb-acid2.gbc", Model::Cgb, 2, 30);
}

#[test]
fn dmg_sound_wave_channel() {
    resumes_identically(
        "blargg/dmg_sound/rom_singles/09-wave read while on.gb",
        Model::Dmg,
        30,
        120,
    );
}

#[test]
fn dmg_sound_sweep() {
    resumes_identically(
        "blargg/dmg_sound/rom_singles/05-sweep details.gb",
        Model::Dmg,
        20,
        120,
    );
}

#[test]
fn cgb_sound_noise_and_length() {
    resumes_identically(
        "blargg/cgb_sound/rom_singles/08-len ctr during power.gb",
        Model::Cgb,
        20,
        120,
    );
}

#[test]
fn cpu_interrupts_and_timer() {
    resumes_identically(
        "blargg/cpu_instrs/individual/02-interrupts.gb",
        Model::Dmg,
        10,
        60,
    );
}

#[test]
fn mbc1_banking() {
    resumes_identically(
        "mooneye/emulator-only/mbc1/bits_bank2.gb",
        Model::Dmg,
        1,
        30,
    );
}

#[test]
fn mbc5_banking() {
    resumes_identically("mooneye/emulator-only/mbc5/rom_64Mb.gb", Model::Dmg, 1, 30);
}

#[test]
fn timer_div() {
    resumes_identically("mooneye/acceptance/timer/div_write.gb", Model::Dmg, 1, 30);
}

#[test]
fn dmg_stress_rom() {
    rom_resumes_identically("DMG 부하 ROM", stress_rom(false), Model::Dmg, 45, 180);
}

#[test]
fn cgb_stress_rom() {
    rom_resumes_identically("CGB 부하 ROM", stress_rom(true), Model::Cgb, 45, 180);
}

/// v1 스테이트 고정 파일. 형식이 바뀌어도 이전 스테이트를 읽을 수 있어야 한다(스펙 §4.8).
/// 파일이 없을 때 `ARAGORN_WRITE_STATE_FIXTURE=1`로 실행하면 지금 코드로 만든다.
#[test]
fn version_1_state_still_loads() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/state-v1-cgb.argn"
    );
    let rom = stress_rom(true);
    if std::env::var_os("ARAGORN_WRITE_STATE_FIXTURE").is_some() {
        let mut gb = GameBoy::new(rom.clone(), Model::Cgb).unwrap();
        for _ in 0..30 {
            gb.run_frame();
        }
        let path = std::path::Path::new(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, gb.save_state(SAVED_AT)).unwrap();
    }
    let data = std::fs::read(path).expect("v1 스테이트 고정 파일");
    let mut gb = GameBoy::new(rom, Model::Cgb).unwrap();
    gb.load_state(&data, SAVED_AT)
        .expect("v1 스테이트는 앞으로도 읽을 수 있어야 한다");
    for _ in 0..10 {
        gb.run_frame();
    }
}

#[test]
fn stress_rom_actually_makes_sound_and_varied_screens() {
    for (cgb, model) in [(false, Model::Dmg), (true, Model::Cgb)] {
        let mut gb = GameBoy::new(stress_rom(cgb), model).unwrap();
        let mut audio = Vec::new();
        let mut screens = std::collections::HashSet::new();
        for _ in 0..120 {
            gb.run_frame();
            gb.drain_audio(&mut audio);
            screens.insert(gb.framebuffer().to_vec());
        }
        let loud = audio.iter().filter(|s| s.abs() > 0.01).count();
        assert!(
            loud > audio.len() / 4,
            "{model:?}: 소리가 거의 없다 ({loud}/{})",
            audio.len()
        );
        assert!(
            screens.len() > 30,
            "{model:?}: 화면이 거의 변하지 않는다 ({})",
            screens.len()
        );
    }
}

/// 값 트리에서 정수 잎마다 경로를 모은다. 경로는 (맵 값 / 배열 원소) 인덱스의 나열이다.
fn integer_paths(value: &rmpv::Value, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
    match value {
        rmpv::Value::Integer(_) => out.push(path.clone()),
        rmpv::Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                path.push(i);
                integer_paths(item, path, out);
                path.pop();
            }
        }
        rmpv::Value::Map(entries) => {
            for (i, (_, item)) in entries.iter().enumerate() {
                path.push(i);
                integer_paths(item, path, out);
                path.pop();
            }
        }
        _ => {}
    }
}

fn at<'a>(value: &'a mut rmpv::Value, path: &[usize]) -> &'a mut rmpv::Value {
    path.iter().fold(value, |v, &i| match v {
        rmpv::Value::Array(items) => &mut items[i],
        rmpv::Value::Map(entries) => &mut entries[i].1,
        _ => unreachable!(),
    })
}

/// 손상된 스테이트: 본문의 정수를 하나씩 극단값으로 바꿔 불러온다. 거부하거나, 받아들이면 몇 프레임
/// 돌아야 한다. 어느 경우에도 패닉하면 안 된다.
#[test]
fn corrupted_integers_never_panic() {
    for (cgb, model) in [(false, Model::Dmg), (true, Model::Cgb)] {
        let rom = stress_rom(cgb);
        let mut gb = GameBoy::new(rom.clone(), model).unwrap();
        for _ in 0..30 {
            gb.run_frame();
        }
        let state = gb.save_state(SAVED_AT);
        let header_len = 17 + gb.rom_id().title.len();
        let (header, body) = state.split_at(header_len);
        let tree = rmpv::decode::read_value(&mut &body[..]).unwrap();
        let mut paths = Vec::new();
        integer_paths(&tree, &mut Vec::new(), &mut paths);
        assert!(paths.len() > 100, "정수 필드가 너무 적다: {}", paths.len());
        let mut accepted = 0;
        for path in &paths {
            for value in [0u64, 1, 0x7F, 0xFF, 0xFFFF, 0xFFFF_FFFF] {
                let mut tree = tree.clone();
                *at(&mut tree, path) = rmpv::Value::from(value);
                let mut data = header.to_vec();
                rmpv::encode::write_value(&mut data, &tree).unwrap();
                let mut target = GameBoy::new(rom.clone(), model).unwrap();
                if target.load_state(&data, SAVED_AT).is_ok() {
                    accepted += 1;
                    for _ in 0..2 {
                        target.run_frame();
                    }
                    target.drain_audio(&mut Vec::new());
                }
            }
        }
        assert!(accepted > 0);
    }
}
