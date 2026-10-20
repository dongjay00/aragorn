//! 코어의 공개 진입점.

use crate::bus::Bus;
use crate::cartridge::{CartError, Cartridge, Header};
use crate::cheat::{Cheat, Cheats};
use crate::cpu::{Cpu, IllegalOpcode, Registers, disasm};
use crate::debugger::Debugger;
use crate::joypad::Button;
use crate::model::Model;
use crate::ppu::{self, PaletteKind};
use crate::state::{self, RomId, StateError};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct GameBoy {
    cpu: Cpu,
    bus: Bus,
    model: Model,
    /// 브레이크포인트와 실행 흔적. 스테이트에 넣지 않는다.
    #[serde(skip)]
    debugger: Debugger,
}

impl GameBoy {
    /// 카트리지를 읽고 부트 ROM이 끝난 직후 상태로 시작한다 (부트 ROM은 실행하지 않는다).
    pub fn new(rom: Vec<u8>, model: Model) -> Result<GameBoy, CartError> {
        let cart = Cartridge::new(rom)?;
        let model = model.resolve(cart.header().cgb_flag);
        let sgb = model == Model::Dmg && cart.header().supports_sgb();
        let mut bus = Bus::new(cart, model);
        if sgb {
            bus.enable_sgb();
        }
        Ok(GameBoy {
            cpu: Cpu::new(Registers::post_boot(model)),
            bus,
            model,
            debugger: Debugger::default(),
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
        self.traced_step();
    }

    /// CPU를 한 번 진행한다. 명령을 실행하는 경우 그 PC를 실행 흔적에 남긴다.
    fn traced_step(&mut self) {
        if !self.cpu.halted() && self.cpu.lock().is_none() {
            self.debugger.record(self.cpu.regs.pc);
        }
        self.cpu.step(&mut self.bus);
    }

    /// 다음에 실행할 명령이 브레이크포인트인지. HALT·정지 중에는 명령을 실행하지 않으므로 보지 않는다.
    fn at_breakpoint(&self) -> bool {
        !self.cpu.halted()
            && self.cpu.lock().is_none()
            && self.debugger.is_breakpoint(self.cpu.regs.pc)
    }

    /// 다음 VBlank 진입까지 실행한다. 프레임 경계가 오지 않아도(LCD가 꺼져 있거나 게임이
    /// LCD를 계속 껐다 켜는 경우) 한 프레임 분량(70224 T-사이클)이 지나면 반환한다.
    ///
    /// 브레이크포인트 주소의 명령을 실행하기 직전에 멈추고 돌아온다(스펙 §4.3). 그 자리에서 다시
    /// 부르면 그 명령은 한 번 통과한다.
    pub fn run_frame(&mut self) {
        let resume = self.debugger.hit();
        self.debugger.set_hit(None);
        let deadline = self.bus.dots() + u64::from(ppu::DOTS_PER_FRAME);
        let mut first = true;
        loop {
            if self.at_breakpoint() && !(first && resume == Some(self.cpu.regs.pc)) {
                self.debugger.set_hit(Some(self.cpu.regs.pc));
                return;
            }
            first = false;
            self.traced_step();
            if self.bus.take_frame_ready() {
                self.bus.apply_cheat_writes();
                self.bus.frame_done();
                return;
            }
            if self.bus.dots() >= deadline {
                return;
            }
        }
    }

    /// 디버거의 "명령 1개 실행". 브레이크포인트는 보지 않는다. HALT 중이면 깨어나 다음 명령을 실행할
    /// 때까지(최대 한 프레임 분량) 진행한다.
    pub fn step_instruction(&mut self) {
        self.debugger.set_hit(None);
        if self.cpu.halted() {
            let deadline = self.bus.dots() + u64::from(ppu::DOTS_PER_FRAME);
            while self.cpu.halted() && self.bus.dots() < deadline {
                self.cpu.step(&mut self.bus);
            }
        } else {
            self.traced_step();
        }
        // 단계 실행 중에 지난 VBlank 표시는 버린다. 남겨 두면 다음 `run_frame`이 바로 끝난다.
        self.bus.take_frame_ready();
    }

    /// 링크 케이블을 잇거나 끊는다. 끊으면 끝나지 않은 전송은 0xFF를 받고 끝난다.
    pub fn set_link(&mut self, connected: bool) {
        self.bus.set_link(connected);
    }

    pub fn is_linked(&self) -> bool {
        self.bus.is_linked()
    }

    /// 상대 기기가 보낸 링크 메시지를 받는다. 프레임 사이에 부른다.
    pub fn link_receive(&mut self, message: crate::serial::LinkMessage) {
        self.bus.link_receive(message);
    }

    /// 상대 기기에 보낼 링크 메시지를 꺼낸다. 프레임 사이에 부른다.
    pub fn link_outbox(&mut self) -> Vec<crate::serial::LinkMessage> {
        self.bus.link_outbox()
    }

    /// 켤 치트를 바꾼다. 빈 목록이면 모두 끈다. 스테이트를 불러와도 유지한다.
    pub fn set_cheats(&mut self, cheats: &[Cheat]) {
        self.bus.set_cheats(Cheats::new(cheats));
    }

    /// PC 브레이크포인트를 `pcs`로 바꾼다. 빈 목록이면 모두 지운다.
    pub fn set_breakpoints(&mut self, pcs: &[u16]) {
        self.debugger.set_breakpoints(pcs);
    }

    /// 160×144, 각 픽셀은 0xRRGGBBAA.
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.ppu().framebuffer()
    }

    /// 슈퍼 게임보이 기능을 쓰는 게임이면 그 상태 (팔레트, 테두리).
    pub fn sgb(&self) -> Option<&crate::sgb::Sgb> {
        self.bus.sgb()
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

    /// 디버거의 채널 음소거. `channel`은 0–3(채널 1–4)이다. 스테이트를 불러와도 유지한다.
    pub fn set_channel_muted(&mut self, channel: usize, muted: bool) {
        self.bus.apu_mut().set_channel_muted(channel, muted);
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
        loaded.debugger = std::mem::take(&mut self.debugger);
        loaded.debugger.reset_run_state();
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

    pub fn ime(&self) -> bool {
        self.gb.cpu.ime()
    }

    pub fn halted(&self) -> bool {
        self.gb.cpu.halted()
    }

    /// 마지막 `run_frame`이 브레이크포인트에 걸려 멈췄으면 그 PC.
    pub fn breakpoint_hit(&self) -> Option<u16> {
        self.gb.debugger.hit()
    }

    /// 최근 실행한 명령의 PC, 오래된 것부터 (최대 `debugger::TRACE_LEN`개).
    pub fn trace(&self) -> Vec<u16> {
        self.gb.debugger.trace()
    }

    /// `addr`의 명령을 해석한다. 메모리를 읽어도 게임 상태는 바뀌지 않는다.
    pub fn disassemble(&self, addr: u16) -> disasm::Instruction {
        disasm::disassemble(addr, |a| self.peek(a))
    }

    /// CGB 모드로 그리는지 (VRAM 뱅크 2개와 색 팔레트).
    pub fn cgb(&self) -> bool {
        self.gb.bus.ppu().is_cgb()
    }

    /// VRAM 뱅크 하나 (0x8000–0x9FFF에 해당하는 0x2000바이트).
    pub fn vram(&self, bank: u8) -> &'a [u8] {
        self.gb.bus.ppu().vram_bank_data(bank)
    }

    /// OAM 전체 (스프라이트 40개 × 4바이트).
    pub fn oam(&self) -> &'a [u8; 0xA0] {
        self.gb.bus.ppu().oam()
    }

    /// CGB 팔레트 RAM.
    pub fn palette_ram(&self, kind: PaletteKind) -> &'a [u8; 64] {
        self.gb.bus.ppu().palette_ram(kind)
    }

    /// 화면에 그릴 때와 같은 0xRRGGBBAA 색 (`ppu::Ppu::palette_color`).
    pub fn palette_color(&self, kind: PaletteKind, palette: u8, color: u8) -> u32 {
        self.gb.bus.ppu().palette_color(kind, palette, color)
    }

    pub fn channel_muted(&self, channel: usize) -> bool {
        self.gb.bus.apu().channel_muted(channel)
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

    /// 받은 바이트에 1을 더해 다음에 돌려줄 값으로 두는 슬레이브. 받은 바이트는 0xC000부터 적는다.
    const SLAVE: [u8; 26] = [
        0x21, 0x00, 0xC0, 0xAF, 0xE0, 0x01, // LD HL,C000 ; SB=0
        0x3E, 0x80, 0xE0, 0x02, // loop: SC=0x80 (외부 클럭으로 받을 준비)
        0xF0, 0x02, 0xCB, 0x7F, 0x20, 0xFA, // 끝날 때까지 SC 비트 7을 본다
        0xF0, 0x01, 0x22, 0x3C, 0xE0, 0x01, // 받은 값을 적고, 1을 더해 SB에
        0x18, 0xEE, 0x00, 0x00, // JR loop
    ];
    /// 1부터 16까지 보내고 받은 바이트를 0xC000부터 적는 마스터.
    const MASTER: [u8; 29] = [
        0x21, 0x00, 0xC0, 0x06, 0x01, // LD HL,C000 ; LD B,1
        0x78, 0xE0, 0x01, 0x3E, 0x81, 0xE0,
        0x02, // loop: SB=B ; SC=0x81 (내부 클럭으로 보냄)
        0xF0, 0x02, 0xCB, 0x7F, 0x20, 0xFA, // 끝날 때까지 기다린다
        0xF0, 0x01, 0x22, 0x04, 0x78, 0xFE, 0x11, 0x20,
        0xEA, // 적고 B+1, 17이 아니면 다시
        0x18, 0xFE,
    ];

    fn linked_pair() -> (GameBoy, GameBoy) {
        let (mut master, mut slave) = (gb_with_program(&MASTER), gb_with_program(&SLAVE));
        master.set_link(true);
        slave.set_link(true);
        (master, slave)
    }

    /// 프레임마다 두 기기를 돌리고 그 사이에 메시지를 전한다. `slave_first`면 받는 쪽이 먼저 돈다.
    fn run_linked(master: &mut GameBoy, slave: &mut GameBoy, frames: usize, slave_first: bool) {
        for _ in 0..frames {
            if slave_first {
                slave.run_frame();
                master.run_frame();
            } else {
                master.run_frame();
                slave.run_frame();
            }
            for message in master.link_outbox() {
                slave.link_receive(message);
            }
            for message in slave.link_outbox() {
                master.link_receive(message);
            }
        }
    }

    fn written(gb: &GameBoy, count: u16) -> Vec<u8> {
        (0..count).map(|i| gb.debug().peek(0xC000 + i)).collect()
    }

    #[test]
    fn linked_game_boys_exchange_bytes_through_the_serial_port() {
        for slave_first in [true, false] {
            let (mut master, mut slave) = linked_pair();
            run_linked(&mut master, &mut slave, 60, slave_first);
            assert_eq!(
                written(&slave, 16),
                (1..=16).collect::<Vec<u8>>(),
                "{slave_first}"
            );
            // 처음에는 슬레이브가 준비한 0, 그 뒤로는 슬레이브가 받은 값 + 1
            let mut expected = vec![0];
            expected.extend(2..=16);
            assert_eq!(written(&master, 16), expected, "{slave_first}");
        }
    }

    #[test]
    fn unlinked_master_reads_ff() {
        let mut master = gb_with_program(&MASTER);
        master.run_frame();
        assert_eq!(written(&master, 2), [0xFF, 0xFF]);
    }

    #[test]
    fn sgb_games_get_sgb_features_but_stay_dmg() {
        let mut rom = test_rom(0x00, 0x00, 0x00);
        assert!(
            GameBoy::new(rom.clone(), Model::Auto)
                .unwrap()
                .sgb()
                .is_none()
        );
        rom[0x0146] = 0x03;
        rom[0x014B] = 0x33;
        let gb = GameBoy::new(rom.clone(), Model::Auto).unwrap();
        assert_eq!(gb.model(), Model::Dmg);
        assert!(gb.sgb().is_some());
        // CGB 게임은 SGB 플래그가 있어도 컬러로 돈다.
        rom[0x0143] = 0x80;
        assert!(GameBoy::new(rom, Model::Auto).unwrap().sgb().is_none());
    }

    #[test]
    fn game_code_sends_an_sgb_palette_through_the_joypad_port() {
        // 0x0200의 16바이트 패킷을 P1 쓰기로 보내는 프로그램 (리셋 펄스, 비트마다 0x10/0x20 뒤 0x30, 정지 비트).
        let program = [
            0x21, 0x00, 0x02, // LD HL,0x0200
            0x3E, 0x00, 0xE0, 0x00, // 리셋
            0x3E, 0x30, 0xE0, 0x00, //
            0x06, 0x10, // LD B,16
            0x2A, 0x57, 0x0E, 0x08, // byte: LD A,(HL+) ; LD D,A ; LD C,8
            0x3E, 0x10, 0xCB, 0x42, 0x20, 0x02, 0x3E, 0x20, // bit: 1이면 0x10, 0이면 0x20
            0xE0, 0x00, 0x3E, 0x30, 0xE0, 0x00, // 보내고 0x30
            0xCB, 0x1A, 0x0D, 0x20, 0xED, // RR D ; DEC C ; JR NZ,bit
            0x05, 0x20, 0xE6, // DEC B ; JR NZ,byte
            0x3E, 0x20, 0xE0, 0x00, 0x3E, 0x30, 0xE0, 0x00, // 정지 비트
            0x18, 0xFE,
        ];
        let mut rom = test_rom(0x00, 0x00, 0x00);
        rom[0x0100..0x0100 + program.len()].copy_from_slice(&program);
        // PAL01: 색 0 = 빨강(0x001F)
        rom[0x0200..0x0203].copy_from_slice(&[1, 0x1F, 0x00]); // PAL01, 패킷 1개
        rom[0x0146] = 0x03;
        rom[0x014B] = 0x33;
        let mut gb = GameBoy::new(rom, Model::Auto).unwrap();
        let before = gb.sgb().unwrap().colorize(gb.framebuffer())[0];
        gb.run_frame();
        gb.run_frame();
        let after = gb.sgb().unwrap().colorize(gb.framebuffer())[0];
        assert_ne!(before, after);
        assert_eq!(after, 0xFF00_00FF);
    }

    #[test]
    fn state_from_before_sgb_still_loads() {
        let mut rom = test_rom(0x00, 0x00, 0x00);
        rom[0x0146] = 0x03;
        rom[0x014B] = 0x33;
        let mut gb = GameBoy::new(rom.clone(), Model::Auto).unwrap();
        // SGB 기능이 없던 버전의 스테이트: sgb 필드가 없다.
        gb.bus.sgb = None;
        let old = gb.save_state(0);
        let mut gb = GameBoy::new(rom, Model::Auto).unwrap();
        gb.load_state(&old, 0).unwrap();
        assert!(gb.sgb().is_some(), "다시 켠다");
    }

    #[test]
    fn game_genie_changes_what_the_cpu_reads() {
        // LD A,(0x0200) ; JR -2. ROM의 0x0200은 0이다.
        let mut gb = gb_with_program(&[0xFA, 0x00, 0x02, 0x18, 0xFE]);
        // 0x0200을 0x42로: AB=42, 주소 (F^F)CDE = 0 2 0 0 → C=2, D=0, E=0, F=F
        gb.set_cheats(&[Cheat::parse("422-00F").unwrap()]);
        gb.step();
        assert_eq!(gb.debug().registers().a, 0x42);
        gb.set_cheats(&[]);
        assert_eq!(gb.debug().peek(0x0200), 0x00, "끄면 원래 값");
    }

    #[test]
    fn game_shark_writes_every_frame_and_survives_state_loading() {
        let mut gb = gb_with_program(&[0x18, 0xFE]);
        let state = gb.save_state(0);
        gb.set_cheats(&[Cheat::parse("0163 38CD").unwrap()]);
        gb.run_frame();
        assert_eq!(gb.debug().peek(0xCD38), 0x63);
        gb.load_state(&state, 0).unwrap();
        assert_eq!(gb.debug().peek(0xCD38), 0x00, "스테이트의 메모리");
        gb.run_frame();
        assert_eq!(
            gb.debug().peek(0xCD38),
            0x63,
            "불러온 뒤에도 치트는 켜져 있다"
        );
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
    fn breakpoint_stops_before_the_instruction_mid_frame() {
        // 0x0100부터 NOP만 있다 (ROM의 나머지는 0).
        let mut gb = gb_with_program(&[]);
        gb.set_breakpoints(&[0x0200]);
        gb.run_frame();
        assert_eq!(gb.debug().registers().pc, 0x0200);
        assert_eq!(gb.debug().breakpoint_hit(), Some(0x0200));
        assert_eq!(gb.debug().trace().last(), Some(&0x01FF));
    }

    #[test]
    fn resuming_passes_the_breakpoint_once() {
        let mut gb = gb_with_program(&[]);
        gb.set_breakpoints(&[0x0200]);
        gb.run_frame();
        gb.run_frame();
        assert_eq!(
            gb.debug().breakpoint_hit(),
            None,
            "직선 코드는 다시 오지 않는다"
        );
        assert!(gb.debug().registers().pc > 0x0200);
    }

    #[test]
    fn loop_hits_the_breakpoint_every_iteration() {
        let mut gb = gb_with_program(&[0x18, 0xFE]); // JR -2
        gb.set_breakpoints(&[0x0100]);
        gb.run_frame();
        assert_eq!(gb.debug().breakpoint_hit(), Some(0x0100));
        assert!(gb.debug().trace().is_empty(), "시작 위치에서 바로 멈춘다");
        gb.run_frame();
        assert_eq!(gb.debug().breakpoint_hit(), Some(0x0100));
        assert_eq!(gb.debug().trace(), [0x0100]);
    }

    #[test]
    fn cleared_breakpoints_run_whole_frames() {
        let mut gb = gb_with_program(&[0x18, 0xFE]);
        gb.set_breakpoints(&[0x0100]);
        gb.run_frame();
        gb.set_breakpoints(&[]);
        gb.run_frame();
        assert_eq!(gb.debug().breakpoint_hit(), None);
        assert_eq!(gb.debug().trace().len(), crate::debugger::TRACE_LEN);
    }

    #[test]
    fn step_instruction_runs_one_instruction_ignoring_breakpoints() {
        let mut gb = gb_with_program(&[0x01, 0x34, 0x12, 0x00]); // LD BC,$1234 ; NOP
        gb.set_breakpoints(&[0x0103]);
        gb.step_instruction();
        assert_eq!(gb.debug().registers().pc, 0x0103);
        assert_eq!(gb.debug().registers().c, 0x34);
        gb.step_instruction();
        assert_eq!(gb.debug().registers().pc, 0x0104);
        assert_eq!(
            gb.debug().disassemble(0x0100).text,
            "LD BC,$1234",
            "디스어셈블은 게임 메모리를 그대로 읽는다"
        );
    }

    #[test]
    fn step_instruction_while_halted_forever_returns() {
        // DI ; XOR A ; LDH (IE),A ; HALT — 깨울 인터럽트가 없다
        let mut gb = gb_with_program(&[0xF3, 0xAF, 0xE0, 0xFF, 0x76]);
        for _ in 0..4 {
            gb.step_instruction();
        }
        assert!(gb.debug().halted());
        gb.step_instruction();
        assert!(gb.debug().halted(), "한 프레임 분량 뒤 돌아온다");
    }

    #[test]
    fn breakpoint_after_halt_wake_without_ime_hits() {
        // DI ; XOR A ; LDH (IF),A ; LD A,1 ; LDH (IE),A ; HALT ; NOP(0x0109) ; JR -2
        // 대기 중인 인터럽트를 지운 뒤 IME=0으로 HALT하고, VBlank이 깨운다.
        let mut gb = gb_with_program(&[
            0xF3, 0xAF, 0xE0, 0x0F, 0x3E, 0x01, 0xE0, 0xFF, 0x76, 0x00, 0x18, 0xFE,
        ]);
        gb.set_breakpoints(&[0x0109]);
        for _ in 0..3 {
            gb.run_frame();
            if gb.debug().breakpoint_hit().is_some() {
                break;
            }
        }
        assert_eq!(gb.debug().breakpoint_hit(), Some(0x0109));
        assert_eq!(gb.debug().registers().pc, 0x0109);
        assert!(!gb.debug().halted());
        gb.step_instruction();
        assert_eq!(
            gb.debug().registers().pc,
            0x010A,
            "멈춘 명령부터 이어서 실행한다"
        );
    }

    #[test]
    fn breakpoints_survive_loading_a_state() {
        let mut gb = gb_with_program(&[0x18, 0xFE]);
        let state = gb.save_state(0);
        gb.set_breakpoints(&[0x0100]);
        gb.run_frame();
        gb.load_state(&state, 0).unwrap();
        assert_eq!(gb.debug().breakpoint_hit(), None);
        assert!(gb.debug().trace().is_empty());
        gb.run_frame();
        assert_eq!(gb.debug().breakpoint_hit(), Some(0x0100));
    }

    #[test]
    fn channel_mutes_survive_loading_a_state() {
        let mut gb = gb_with_program(&[0x18, 0xFE]);
        let state = gb.save_state(0);
        gb.set_channel_muted(2, true);
        gb.load_state(&state, 0).unwrap();
        assert!(gb.debug().channel_muted(2));
        assert!(!gb.debug().channel_muted(0));
        assert!(!gb_with_program(&[0x18, 0xFE]).debug().channel_muted(2));
    }

    #[test]
    fn debug_view_exposes_vram_banks_oam_and_palettes() {
        let mut rom = test_rom(0x00, 0x00, 0x80);
        // LD A,1 ; LDH (VBK),A ; LD A,$5A ; LD ($8010),A ; LD A,$77 ; LD ($FE00),A ; JR -2
        let program = [
            0x3E, 0x01, 0xE0, 0x4F, 0x3E, 0x5A, 0xEA, 0x10, 0x80, 0x3E, 0x77, 0xEA, 0x00, 0xFE,
            0x18, 0xFE,
        ];
        rom[0x0100..0x0100 + program.len()].copy_from_slice(&program);
        let mut gb = GameBoy::new(rom, Model::Cgb).unwrap();
        gb.run_frame();
        let debug = gb.debug();
        assert!(debug.cgb());
        assert_eq!(debug.vram(1)[0x10], 0x5A);
        assert_eq!(debug.vram(0)[0x10], 0x00);
        assert_eq!(debug.oam()[0], 0x77);
        assert_eq!(debug.palette_ram(PaletteKind::Bg).len(), 64);
        assert_eq!(debug.palette_color(PaletteKind::Bg, 0, 0), 0xFFFF_FFFF);
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
