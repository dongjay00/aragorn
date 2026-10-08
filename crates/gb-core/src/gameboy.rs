//! 코어의 공개 진입점.

use crate::bus::Bus;
use crate::cartridge::{CartError, Cartridge, Header};
use crate::cpu::{Cpu, IllegalOpcode, Registers};
use crate::joypad::Button;
use crate::model::Model;
use crate::ppu;
use crate::state::{self, RomId, StateError};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct GameBoy {
    cpu: Cpu,
    bus: Bus,
    model: Model,
}

impl GameBoy {
    /// 카트리지를 읽고 부트 ROM이 끝난 직후 상태로 시작한다 (부트 ROM은 실행하지 않는다).
    pub fn new(rom: Vec<u8>, model: Model) -> Result<GameBoy, CartError> {
        let cart = Cartridge::new(rom)?;
        let model = model.resolve(cart.header().cgb_flag);
        Ok(GameBoy {
            cpu: Cpu::new(Registers::post_boot(model)),
            bus: Bus::new(cart, model),
            model,
        })
    }

    pub fn model(&self) -> Model {
        self.model
    }

    pub fn header(&self) -> &Header {
        self.bus.cartridge().header()
    }

    /// 명령 하나(또는 인터럽트 디스패치, HALT 대기 1 M-사이클)를 실행한다.
    pub fn step(&mut self) {
        self.cpu.step(&mut self.bus);
    }

    /// 다음 VBlank 진입까지 실행한다. 프레임 경계가 오지 않아도(LCD가 꺼져 있거나 게임이
    /// LCD를 계속 껐다 켜는 경우) 한 프레임 분량(70224 T-사이클)이 지나면 반환한다.
    pub fn run_frame(&mut self) {
        let deadline = self.bus.dots() + u64::from(ppu::DOTS_PER_FRAME);
        loop {
            self.cpu.step(&mut self.bus);
            if self.bus.take_frame_ready() || self.bus.dots() >= deadline {
                return;
            }
        }
    }

    /// 160×144, 각 픽셀은 0xRRGGBBAA.
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.ppu().framebuffer()
    }

    /// 키를 누르거나 뗀다. 선택된 줄에서 새로 눌리면 조이패드 인터럽트를 요청한다.
    pub fn set_button(&mut self, button: Button, pressed: bool) {
        self.bus.set_button(button, pressed);
    }

    /// 세이브 파일 내용: 외부 RAM과, MBC3 RTC가 있으면 48바이트 RTC 블록(BGB/VBA-M 형식).
    /// `now_unix`는 저장 시각으로 기록된다. 배터리가 없는 카트리지는 `None`.
    pub fn battery_ram(&self, now_unix: u64) -> Option<Vec<u8>> {
        self.bus.cartridge().battery_ram(now_unix)
    }

    /// 외부 RAM 크기(바이트). 세이브 파일에서 RTC 블록을 뺀 길이다.
    pub fn cartridge_ram_len(&self) -> usize {
        self.bus.cartridge().ram_len()
    }

    /// 세이브 파일 내용을 싣는다. RTC가 있으면 저장 시각부터 `now_unix`까지 지난 시간만큼 시계를 진행한다.
    pub fn load_battery_ram(&mut self, data: &[u8], now_unix: u64) {
        self.bus.cartridge_mut().load_battery_ram(data, now_unix);
    }

    /// 마지막 호출 뒤 외부 RAM에 쓰기가 있었으면 `true`. 읽으면 초기화된다.
    pub fn battery_dirty(&mut self) -> bool {
        self.bus.cartridge_mut().take_ram_dirty()
    }

    /// 게임이 외부 RAM을 켜 두었는지. 게임은 보통 저장을 마치면 RAM을 끈다.
    pub fn cartridge_ram_enabled(&self) -> bool {
        self.bus.cartridge().ram_enabled()
    }

    /// 소리 출력 샘플레이트(Hz). 프론트엔드가 오디오 버퍼 상태에 맞춰 조금씩 바꾼다(스펙 §4.5).
    pub fn set_sample_rate(&mut self, rate: f64) {
        self.bus.apu_mut().set_sample_rate(rate);
    }

    /// 쌓인 소리를 인터리브 스테레오 f32(-1.0–1.0)로 `out` 뒤에 붙인다.
    /// 꺼내 가지 않으면 오래된 소리는 버린다(약 1초 분량까지만 남긴다).
    pub fn drain_audio(&mut self, out: &mut Vec<f32>) {
        self.bus.apu_mut().drain(out);
    }

    /// 세이브 스테이트가 어느 ROM의 것인지 가리는 값.
    pub fn rom_id(&self) -> RomId {
        self.bus.cartridge().rom_id()
    }

    /// 지금 상태 전체를 스테이트 파일 내용으로 만든다 (스펙 §4.8). ROM은 넣지 않는다.
    /// `now_unix`는 저장 시각으로 기록되어, 불러올 때 RTC를 실제 시간에 맞추는 데 쓴다.
    pub fn save_state(&self, now_unix: u64) -> Vec<u8> {
        state::encode(&self.rom_id(), now_unix, self)
    }

    /// 스테이트를 불러온다. 다른 ROM·형식·손상된 내용이면 오류를 돌려주고 지금 상태를 그대로 둔다.
    /// RTC가 있으면 저장 시각부터 `now_unix`까지 지난 시간만큼 시계를 진행한다.
    /// 소리 출력 샘플레이트는 지금 값을 유지하고, 아직 꺼내지 않은 소리는 버린다.
    pub fn load_state(&mut self, data: &[u8], now_unix: u64) -> Result<(), StateError> {
        let (saved_at, body) = state::split(data, &self.rom_id())?;
        let mut loaded: GameBoy = state::decode(body)?;
        if loaded.model != self.model {
            return Err(StateError::Corrupt("기기 종류가 다릅니다".into()));
        }
        loaded
            .bus
            .adopt_host(&self.bus)
            .map_err(StateError::Corrupt)?;
        loaded
            .bus
            .cartridge_mut()
            .advance_rtc(now_unix.saturating_sub(saved_at));
        *self = loaded;
        Ok(())
    }

    pub fn debug(&self) -> DebugView<'_> {
        DebugView { gb: self }
    }
}

/// 디버거와 테스트용 읽기 전용 상태.
pub struct DebugView<'a> {
    gb: &'a GameBoy,
}

impl<'a> DebugView<'a> {
    pub fn registers(&self) -> Registers {
        self.gb.cpu.regs
    }

    pub fn illegal_opcode(&self) -> Option<IllegalOpcode> {
        self.gb.cpu.lock()
    }

    pub fn serial_output(&self) -> &'a [u8] {
        self.gb.bus.serial_output()
    }

    pub fn peek(&self, addr: u16) -> u8 {
        self.gb.bus.peek(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::test_rom;

    fn gb_with_program(program: &[u8]) -> GameBoy {
        let mut rom = test_rom(0x00, 0x00, 0x00);
        rom[0x0100..0x0100 + program.len()].copy_from_slice(program);
        GameBoy::new(rom, Model::Dmg).unwrap()
    }

    #[test]
    fn auto_model_follows_cgb_flag() {
        let gb = GameBoy::new(test_rom(0x00, 0x00, 0x80), Model::Auto).unwrap();
        assert_eq!(gb.model(), Model::Cgb);
        assert_eq!(gb.debug().registers().a, 0x11);
        let gb = GameBoy::new(test_rom(0x00, 0x00, 0x80), Model::Dmg).unwrap();
        assert_eq!(gb.model(), Model::Dmg);
        assert_eq!(gb.debug().registers().a, 0x01);
        assert_eq!(gb.header().title, "TEST");
    }

    #[test]
    fn unsupported_cartridge_is_an_error() {
        assert_eq!(
            GameBoy::new(test_rom(0x05, 0x00, 0x00), Model::Auto).err(),
            Some(CartError::Unsupported(0x05))
        );
    }

    #[test]
    fn step_executes_one_instruction() {
        let mut gb = gb_with_program(&[0x3C]); // INC A
        gb.step();
        assert_eq!(gb.debug().registers().pc, 0x0101);
    }

    #[test]
    fn run_frame_stops_at_vblank() {
        let mut gb = gb_with_program(&[0x18, 0xFE]); // JR -2
        gb.run_frame();
        assert_eq!(gb.debug().peek(0xFF44), 144);
    }

    #[test]
    fn run_frame_returns_with_lcd_off() {
        // XOR A ; LDH (0x40),A ; JR -2
        let mut gb = gb_with_program(&[0xAF, 0xE0, 0x40, 0x18, 0xFE]);
        gb.run_frame();
        assert_eq!(gb.debug().peek(0xFF40), 0x00);
        assert_eq!(gb.debug().peek(0xFF44), 0);
    }

    #[test]
    fn run_frame_returns_when_lcd_toggles_every_frame() {
        // XOR A ; LDH (0x40),A ; LD A,0x91 ; LDH (0x40),A ; JR -9
        let mut gb = gb_with_program(&[0xAF, 0xE0, 0x40, 0x3E, 0x91, 0xE0, 0x40, 0x18, 0xF7]);
        gb.run_frame();
        gb.run_frame();
    }

    #[test]
    fn halt_forever_does_not_hang_run_frame() {
        // DI ; HALT, IE=0: 깨어날 수 없지만 프레임은 계속 진행해야 한다.
        let mut gb = gb_with_program(&[0xF3, 0x76]);
        gb.run_frame();
        gb.run_frame();
        assert_eq!(gb.debug().registers().pc, 0x0102);
    }

    #[test]
    fn illegal_opcode_does_not_hang_run_frame() {
        let mut gb = gb_with_program(&[0xD3]);
        gb.run_frame();
        assert_eq!(
            gb.debug().illegal_opcode(),
            Some(IllegalOpcode {
                pc: 0x0100,
                opcode: 0xD3
            })
        );
    }

    #[test]
    fn serial_output_is_visible_in_debug_view() {
        // LD A,'O' ; LDH (0x01),A ; LD A,0x81 ; LDH (0x02),A ; JR -2
        let mut gb = gb_with_program(&[0x3E, b'O', 0xE0, 0x01, 0x3E, 0x81, 0xE0, 0x02, 0x18, 0xFE]);
        gb.run_frame();
        assert_eq!(gb.debug().serial_output(), b"O");
    }

    /// MBC1+RAM+BATTERY(0x03), 8KB RAM. 0x0100: RAM 켜기 → 0xA000에 0x42 쓰기 → RAM 끄기 → 제자리 루프.
    fn saving_rom() -> Vec<u8> {
        let mut rom = test_rom(0x03, 0x00, 0x00);
        rom[0x0149] = 0x02;
        let program = [
            0x3E, 0x0A, 0xEA, 0x00, 0x00, // LD A,0x0A ; LD (0x0000),A
            0x3E, 0x42, 0xEA, 0x00, 0xA0, // LD A,0x42 ; LD (0xA000),A
            0xAF, 0xEA, 0x00, 0x00, // XOR A ; LD (0x0000),A
            0x18, 0xFE, // JR -2
        ];
        rom[0x0100..0x0100 + program.len()].copy_from_slice(&program);
        rom
    }

    #[test]
    fn battery_ram_is_exposed_only_for_battery_carts() {
        let gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
        assert_eq!(gb.battery_ram(0).map(|r| r.len()), Some(0x2000));
        let gb = gb_with_program(&[0x00]);
        assert_eq!(gb.battery_ram(0), None);
    }

    #[test]
    fn game_writes_mark_battery_dirty_and_disable_ram() {
        let mut gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
        assert!(!gb.battery_dirty());
        gb.run_frame();
        assert!(gb.battery_dirty());
        assert!(!gb.battery_dirty(), "읽으면 초기화");
        assert!(!gb.cartridge_ram_enabled());
        assert_eq!(gb.battery_ram(0).unwrap()[0], 0x42);
    }

    #[test]
    fn loaded_battery_ram_is_visible_to_the_game() {
        let mut gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
        gb.load_battery_ram(&[0x11, 0x22], 0);
        let ram = gb.battery_ram(0).unwrap();
        assert_eq!((ram[0], ram[1], ram[2]), (0x11, 0x22, 0x00));
        assert!(!gb.battery_dirty(), "불러오기는 게임의 쓰기가 아니다");
    }

    #[test]
    fn state_round_trips_ram_registers_and_screen() {
        let mut gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
        gb.run_frame();
        let state = gb.save_state(0);
        let registers = gb.debug().registers();
        let mut other = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
        other.load_state(&state, 0).unwrap();
        assert_eq!(other.debug().registers(), registers);
        assert_eq!(
            other.battery_ram(0).unwrap()[0],
            0x42,
            "외부 RAM도 되돌린다"
        );
        assert_eq!(other.framebuffer(), gb.framebuffer());
        assert_eq!(other.save_state(0), state);
    }

    #[test]
    fn state_of_another_rom_is_rejected_and_nothing_changes() {
        let mut other_rom = saving_rom();
        other_rom[0x0134..0x0138].copy_from_slice(b"GOLD");
        let other = GameBoy::new(other_rom, Model::Dmg).unwrap();
        let state = other.save_state(0);
        let mut gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
        gb.run_frame();
        let before = gb.save_state(0);
        assert_eq!(
            gb.load_state(&state, 0),
            Err(StateError::WrongRom {
                title: "GOLD".into()
            })
        );
        assert_eq!(gb.save_state(0), before);
    }

    #[test]
    fn same_rom_with_different_global_checksum_is_rejected() {
        let mut patched = saving_rom();
        patched[0x014E] = 0x12;
        let state = GameBoy::new(patched, Model::Dmg).unwrap().save_state(0);
        let mut gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
        assert!(matches!(
            gb.load_state(&state, 0),
            Err(StateError::WrongRom { .. })
        ));
    }

    #[test]
    fn state_from_another_model_is_rejected() {
        let rom = test_rom(0x00, 0x00, 0x80);
        let cgb = GameBoy::new(rom.clone(), Model::Cgb).unwrap();
        let mut dmg = GameBoy::new(rom, Model::Dmg).unwrap();
        let before = dmg.save_state(0);
        assert!(matches!(
            dmg.load_state(&cgb.save_state(0), 0),
            Err(StateError::Corrupt(_))
        ));
        assert_eq!(dmg.save_state(0), before);
    }

    #[test]
    fn garbage_is_rejected_without_panic() {
        let mut gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
        assert_eq!(gb.load_state(b"", 0), Err(StateError::NotAState));
        let mut state = gb.save_state(0);
        state.truncate(state.len() / 2);
        assert!(matches!(
            gb.load_state(&state, 0),
            Err(StateError::Corrupt(_))
        ));
    }

    #[test]
    fn loading_advances_rtc_by_real_time_since_saving() {
        let mut rom = test_rom(0x10, 0x00, 0x00);
        rom[0x0149] = 0x03;
        let mut gb = GameBoy::new(rom.clone(), Model::Dmg).unwrap();
        let state = gb.save_state(1_000);
        gb.load_state(&state, 1_000 + 2 * 3600 + 5).unwrap();
        // 세이브 파일의 RTC 블록: RAM 뒤 현재 레지스터 초·분·시 (u32 LE)
        let ram = gb.battery_ram(0).unwrap();
        let rtc = &ram[0x8000..];
        assert_eq!((rtc[0], rtc[4], rtc[8]), (5, 0, 2));
        // 시계가 거꾸로 가면 진행하지 않는다
        let mut gb = GameBoy::new(rom, Model::Dmg).unwrap();
        gb.load_state(&state, 10).unwrap();
        let rtc = &gb.battery_ram(0).unwrap()[0x8000..];
        assert_eq!((rtc[0], rtc[4], rtc[8]), (0, 0, 0));
    }

    #[test]
    fn loading_keeps_host_sample_rate_and_drops_pending_audio() {
        let mut gb = GameBoy::new(saving_rom(), Model::Dmg).unwrap();
        gb.set_sample_rate(48_000.0);
        let state = gb.save_state(0);
        gb.set_sample_rate(22_050.0);
        for _ in 0..10 {
            gb.run_frame();
        }
        gb.load_state(&state, 0).unwrap();
        let mut pending = Vec::new();
        gb.drain_audio(&mut pending);
        assert!(pending.is_empty(), "불러오기 전 소리는 버린다");
        for _ in 0..60 {
            gb.run_frame();
        }
        let mut audio = Vec::new();
        gb.drain_audio(&mut audio);
        let per_second = audio.len() as f64 / 2.0 / (60.0 * 70_224.0 / 4_194_304.0);
        assert!((per_second - 22_050.0).abs() < 100.0, "{per_second}");
    }

    #[test]
    fn pressed_button_is_visible_through_p1() {
        // LD A,0x10 ; LDH (0x00),A (버튼 줄 선택) ; JR -2
        let mut gb = gb_with_program(&[0x3E, 0x10, 0xE0, 0x00, 0x18, 0xFE]);
        gb.run_frame();
        gb.set_button(Button::Start, true);
        assert_eq!(gb.debug().peek(0xFF00) & 0x0F, 0x07);
        assert_eq!(
            gb.debug().peek(0xFF0F) & 0x10,
            0x10,
            "조이패드 인터럽트 요청"
        );
    }

    #[test]
    fn audio_is_produced_at_requested_sample_rate() {
        let mut gb = gb_with_program(&[0x18, 0xFE]);
        gb.set_sample_rate(32_000.0);
        let mut samples = Vec::new();
        for _ in 0..60 {
            gb.run_frame();
            gb.drain_audio(&mut samples);
        }
        // 60프레임은 약 1초다. 인터리브 스테레오라 샘플 수는 프레임 수의 두 배다.
        let frames = samples.len() / 2;
        assert!((31_500..=32_200).contains(&frames), "{frames}");
        assert!(samples.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn frames_keep_their_length_in_double_speed() {
        // LD A,1 ; LDH (0x4D),A ; STOP ; JR -2 (CGB)
        let mut rom = test_rom(0x00, 0x00, 0xC0);
        let program = [0x3E, 0x01, 0xE0, 0x4D, 0x10, 0x00, 0x18, 0xFE];
        rom[0x0100..0x0100 + program.len()].copy_from_slice(&program);
        let mut gb = GameBoy::new(rom, Model::Auto).unwrap();
        gb.run_frame();
        assert_eq!(gb.debug().peek(0xFF4D), 0xFE, "2배속");
        let before = gb.bus.dots();
        gb.run_frame();
        assert_eq!(gb.bus.dots() - before, u64::from(ppu::DOTS_PER_FRAME));
        assert_eq!(gb.debug().peek(0xFF44), 144);
    }
}
