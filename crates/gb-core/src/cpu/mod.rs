//! SM83 CPU. 모든 메모리 접근과 내부 지연마다 `CpuBus::tick`을 호출한다 (M-사이클 tick 모델).

pub mod alu;
pub mod registers;

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;

pub use registers::{Registers, flag};

/// CPU가 보는 메모리 버스. 코어에서는 `Bus`가 구현하고, 테스트는 평면 메모리로 대체한다.
/// 핫 패스이므로 제네릭(정적 디스패치)으로만 쓴다.
pub trait CpuBus {
    /// 값을 읽는다. 사이클은 진행하지 않는다.
    fn read(&mut self, addr: u16) -> u8;
    /// 값을 쓴다. 사이클은 진행하지 않는다.
    fn write(&mut self, addr: u16, value: u8);
    /// 주변장치를 1 M-사이클(4 T-사이클) 진행한다.
    fn tick(&mut self);
}

pub const IF_ADDR: u16 = 0xFF0F;
pub const IE_ADDR: u16 = 0xFFFF;

/// 정의되지 않은 옵코드를 만나 CPU가 멈춘 위치.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IllegalOpcode {
    pub pc: u16,
    pub opcode: u8,
}

#[derive(Debug, Clone)]
pub struct Cpu {
    pub regs: Registers,
    ime: bool,
    /// EI 다음 명령이 끝난 뒤 IME를 켠다.
    ime_pending: bool,
    halted: bool,
    /// IME=0이고 인터럽트가 대기 중일 때 HALT하면 다음 바이트를 두 번 읽는다 (HALT 버그).
    halt_bug: bool,
    lock: Option<IllegalOpcode>,
}

impl Cpu {
    pub fn new(regs: Registers) -> Self {
        Self {
            regs,
            ime: false,
            ime_pending: false,
            halted: false,
            halt_bug: false,
            lock: None,
        }
    }

    pub fn ime(&self) -> bool {
        self.ime
    }

    pub fn halted(&self) -> bool {
        self.halted
    }

    pub fn lock(&self) -> Option<IllegalOpcode> {
        self.lock
    }

    /// 명령 하나, 인터럽트 디스패치 하나, 또는 HALT 대기 1 M-사이클을 실행한다.
    ///
    /// 인터럽트는 옵코드 fetch 사이클이 끝날 때 확인한다 (하드웨어의 fetch/실행 겹침).
    /// 그 사이클 안에 켜진 IF도 보이고, 펜딩이면 읽은 옵코드는 버린다.
    pub fn step<B: CpuBus>(&mut self, bus: &mut B) {
        if self.lock.is_some() {
            bus.tick();
            return;
        }
        if self.halted {
            // HALT 중에도 매 M-사이클이 fetch 사이클처럼 동작한다 (PC는 증가하지 않는다).
            bus.tick();
            if pending_interrupts(bus) != 0 {
                self.halted = false;
                let pc = self.regs.pc;
                if self.ime {
                    self.dispatch(bus, pc);
                } else {
                    // 깨어난 사이클이 다음 명령의 fetch 사이클이다.
                    let opcode = bus.read(pc);
                    self.regs.pc = pc.wrapping_add(1);
                    self.execute(bus, opcode);
                }
            }
            return;
        }
        let enable_ime = self.ime_pending;
        let pc = self.regs.pc;
        let halt_bug = self.halt_bug;
        let opcode = self.fetch8(bus);
        if self.ime && pending_interrupts(bus) != 0 {
            // EI 직후 HALT의 HALT 버그 상태였다면 핸들러는 HALT로 돌아온다.
            let ret = if halt_bug { pc.wrapping_sub(1) } else { pc };
            self.dispatch(bus, ret);
            return;
        }
        self.execute(bus, opcode);
        if enable_ime && self.ime_pending {
            self.ime = true;
            self.ime_pending = false;
        }
    }

    /// fetch 사이클 뒤 남은 4 M-사이클: 내부 지연, PC 상위 바이트 push, 하위 바이트 push, 점프.
    /// 벡터는 상위 바이트를 push한 뒤에 정한다. 그 push가 IE를 덮어써서 펜딩이 사라지면
    /// 디스패치가 취소되어 PC=0x0000이 되고 IF는 그대로 남는다 (mooneye `ie_push`).
    fn dispatch<B: CpuBus>(&mut self, bus: &mut B, ret: u16) {
        // 직전 EI의 지연된 IME 켜기도 취소한다. 남아 있으면 핸들러 안에서 IME가 다시 켜진다.
        self.ime = false;
        self.ime_pending = false;
        let [lo, hi] = ret.to_le_bytes();
        bus.tick();
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        write_cycle(bus, self.regs.sp, hi);
        let pending = pending_interrupts(bus);
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        write_cycle(bus, self.regs.sp, lo);
        self.regs.pc = if pending == 0 {
            0x0000
        } else {
            let bit = pending.trailing_zeros() as u8;
            let iflag = bus.read(IF_ADDR);
            bus.write(IF_ADDR, iflag & !(1 << bit));
            0x0040 + 8 * u16::from(bit)
        };
        bus.tick();
    }

    fn fetch8<B: CpuBus>(&mut self, bus: &mut B) -> u8 {
        let value = read_cycle(bus, self.regs.pc);
        if self.halt_bug {
            self.halt_bug = false;
        } else {
            self.regs.pc = self.regs.pc.wrapping_add(1);
        }
        value
    }

    fn fetch16<B: CpuBus>(&mut self, bus: &mut B) -> u16 {
        let lo = self.fetch8(bus);
        let hi = self.fetch8(bus);
        u16::from_le_bytes([lo, hi])
    }

    /// 내부 지연 1 M-사이클 + 쓰기 2번.
    fn push16<B: CpuBus>(&mut self, bus: &mut B, value: u16) {
        let [lo, hi] = value.to_le_bytes();
        bus.tick();
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        write_cycle(bus, self.regs.sp, hi);
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        write_cycle(bus, self.regs.sp, lo);
    }

    fn pop16<B: CpuBus>(&mut self, bus: &mut B) -> u16 {
        let lo = read_cycle(bus, self.regs.sp);
        self.regs.sp = self.regs.sp.wrapping_add(1);
        let hi = read_cycle(bus, self.regs.sp);
        self.regs.sp = self.regs.sp.wrapping_add(1);
        u16::from_le_bytes([lo, hi])
    }

    /// r8 인코딩: 0 B, 1 C, 2 D, 3 E, 4 H, 5 L, 6 (HL), 7 A
    fn read_r8<B: CpuBus>(&self, bus: &mut B, index: u8) -> u8 {
        match index {
            0 => self.regs.b,
            1 => self.regs.c,
            2 => self.regs.d,
            3 => self.regs.e,
            4 => self.regs.h,
            5 => self.regs.l,
            6 => read_cycle(bus, self.regs.hl()),
            _ => self.regs.a,
        }
    }

    fn write_r8<B: CpuBus>(&mut self, bus: &mut B, index: u8, value: u8) {
        match index {
            0 => self.regs.b = value,
            1 => self.regs.c = value,
            2 => self.regs.d = value,
            3 => self.regs.e = value,
            4 => self.regs.h = value,
            5 => self.regs.l = value,
            6 => write_cycle(bus, self.regs.hl(), value),
            _ => self.regs.a = value,
        }
    }

    /// r16 인코딩: 0 BC, 1 DE, 2 HL, 3 SP
    fn r16(&self, index: u8) -> u16 {
        match index & 3 {
            0 => self.regs.bc(),
            1 => self.regs.de(),
            2 => self.regs.hl(),
            _ => self.regs.sp,
        }
    }

    fn set_r16(&mut self, index: u8, value: u16) {
        match index & 3 {
            0 => self.regs.set_bc(value),
            1 => self.regs.set_de(value),
            2 => self.regs.set_hl(value),
            _ => self.regs.sp = value,
        }
    }

    /// PUSH/POP용 r16 인코딩: 3은 SP 대신 AF다.
    fn r16_stack(&self, index: u8) -> u16 {
        if index & 3 == 3 {
            self.regs.af()
        } else {
            self.r16(index)
        }
    }

    fn set_r16_stack(&mut self, index: u8, value: u16) {
        if index & 3 == 3 {
            self.regs.set_af(value);
        } else {
            self.set_r16(index, value);
        }
    }

    /// 조건 인코딩: 0 NZ, 1 Z, 2 NC, 3 C
    fn condition(&self, index: u8) -> bool {
        match index & 3 {
            0 => !self.regs.flag(flag::Z),
            1 => self.regs.flag(flag::Z),
            2 => !self.regs.flag(flag::C),
            _ => self.regs.flag(flag::C),
        }
    }

    fn jr<B: CpuBus>(&mut self, bus: &mut B, taken: bool) {
        let offset = self.fetch8(bus) as i8;
        if taken {
            bus.tick();
            self.regs.pc = self.regs.pc.wrapping_add_signed(i16::from(offset));
        }
    }

    fn halt<B: CpuBus>(&mut self, bus: &mut B) {
        if !self.ime && pending_interrupts(bus) != 0 {
            self.halt_bug = true;
        } else {
            self.halted = true;
        }
    }

    fn execute<B: CpuBus>(&mut self, bus: &mut B, opcode: u8) {
        let y = (opcode >> 3) & 7;
        let z = opcode & 7;
        let p = (opcode >> 4) & 3;
        match opcode {
            0x00 => {}
            // STOP: M1에서는 2바이트 NOP. 저전력 모드와 CGB 속도 전환은 M6에서 구현한다.
            0x10 => {
                self.fetch8(bus);
            }
            0x76 => self.halt(bus),
            0xCB => {
                let op = self.fetch8(bus);
                self.execute_cb(bus, op);
            }
            0xF3 => {
                self.ime = false;
                self.ime_pending = false;
            }
            0xFB => self.ime_pending = true,

            // 16비트 로드와 산술
            0x01 | 0x11 | 0x21 | 0x31 => {
                let value = self.fetch16(bus);
                self.set_r16(p, value);
            }
            0x03 | 0x13 | 0x23 | 0x33 => {
                bus.tick();
                let value = self.r16(p).wrapping_add(1);
                self.set_r16(p, value);
            }
            0x0B | 0x1B | 0x2B | 0x3B => {
                bus.tick();
                let value = self.r16(p).wrapping_sub(1);
                self.set_r16(p, value);
            }
            0x09 | 0x19 | 0x29 | 0x39 => {
                bus.tick();
                let value = self.r16(p);
                alu::add_hl(&mut self.regs, value);
            }
            0x08 => {
                let addr = self.fetch16(bus);
                let [lo, hi] = self.regs.sp.to_le_bytes();
                write_cycle(bus, addr, lo);
                write_cycle(bus, addr.wrapping_add(1), hi);
            }
            0xF9 => {
                bus.tick();
                self.regs.sp = self.regs.hl();
            }
            0xE8 => {
                let offset = self.fetch8(bus) as i8;
                bus.tick();
                bus.tick();
                let value = alu::add_sp_e(&mut self.regs, offset);
                self.regs.sp = value;
            }
            0xF8 => {
                let offset = self.fetch8(bus) as i8;
                bus.tick();
                let value = alu::add_sp_e(&mut self.regs, offset);
                self.regs.set_hl(value);
            }

            // A와 메모리 사이의 간접 로드
            0x02 => write_cycle(bus, self.regs.bc(), self.regs.a),
            0x12 => write_cycle(bus, self.regs.de(), self.regs.a),
            0x22 | 0x32 => {
                let hl = self.regs.hl();
                write_cycle(bus, hl, self.regs.a);
                let next = if opcode == 0x22 {
                    hl.wrapping_add(1)
                } else {
                    hl.wrapping_sub(1)
                };
                self.regs.set_hl(next);
            }
            0x0A => self.regs.a = read_cycle(bus, self.regs.bc()),
            0x1A => self.regs.a = read_cycle(bus, self.regs.de()),
            0x2A | 0x3A => {
                let hl = self.regs.hl();
                self.regs.a = read_cycle(bus, hl);
                let next = if opcode == 0x2A {
                    hl.wrapping_add(1)
                } else {
                    hl.wrapping_sub(1)
                };
                self.regs.set_hl(next);
            }
            0xE0 => {
                let n = self.fetch8(bus);
                write_cycle(bus, 0xFF00 | u16::from(n), self.regs.a);
            }
            0xF0 => {
                let n = self.fetch8(bus);
                self.regs.a = read_cycle(bus, 0xFF00 | u16::from(n));
            }
            0xE2 => write_cycle(bus, 0xFF00 | u16::from(self.regs.c), self.regs.a),
            0xF2 => self.regs.a = read_cycle(bus, 0xFF00 | u16::from(self.regs.c)),
            0xEA => {
                let addr = self.fetch16(bus);
                write_cycle(bus, addr, self.regs.a);
            }
            0xFA => {
                let addr = self.fetch16(bus);
                self.regs.a = read_cycle(bus, addr);
            }

            // 8비트 INC/DEC/LD n
            _ if opcode & 0xC7 == 0x04 => {
                let value = self.read_r8(bus, y);
                let result = alu::inc8(&mut self.regs, value);
                self.write_r8(bus, y, result);
            }
            _ if opcode & 0xC7 == 0x05 => {
                let value = self.read_r8(bus, y);
                let result = alu::dec8(&mut self.regs, value);
                self.write_r8(bus, y, result);
            }
            _ if opcode & 0xC7 == 0x06 => {
                let value = self.fetch8(bus);
                self.write_r8(bus, y, value);
            }

            // A 회전(RLCA, RRCA, RLA, RRA)과 플래그 명령
            0x07 | 0x0F | 0x17 | 0x1F => {
                let a = self.regs.a;
                let result = alu::shift(&mut self.regs, y, a);
                self.regs.a = result;
                self.regs.f &= !flag::Z;
            }
            0x27 => alu::daa(&mut self.regs),
            0x2F => alu::cpl(&mut self.regs),
            0x37 => alu::scf(&mut self.regs),
            0x3F => alu::ccf(&mut self.regs),

            // 상대 점프
            0x18 => self.jr(bus, true),
            0x20 | 0x28 | 0x30 | 0x38 => {
                let taken = self.condition(y);
                self.jr(bus, taken);
            }

            // LD r,r' 와 ALU A,r
            0x40..=0x7F => {
                let value = self.read_r8(bus, z);
                self.write_r8(bus, y, value);
            }
            0x80..=0xBF => {
                let value = self.read_r8(bus, z);
                alu::alu(&mut self.regs, y, value);
            }
            _ if opcode & 0xC7 == 0xC6 => {
                let value = self.fetch8(bus);
                alu::alu(&mut self.regs, y, value);
            }

            // 스택, 점프, 호출
            0xC0 | 0xC8 | 0xD0 | 0xD8 => {
                bus.tick();
                if self.condition(y) {
                    let pc = self.pop16(bus);
                    bus.tick();
                    self.regs.pc = pc;
                }
            }
            0xC9 | 0xD9 => {
                let pc = self.pop16(bus);
                bus.tick();
                self.regs.pc = pc;
                if opcode == 0xD9 {
                    self.ime = true;
                }
            }
            0xC1 | 0xD1 | 0xE1 | 0xF1 => {
                let value = self.pop16(bus);
                self.set_r16_stack(p, value);
            }
            0xC5 | 0xD5 | 0xE5 | 0xF5 => {
                let value = self.r16_stack(p);
                self.push16(bus, value);
            }
            0xC2 | 0xCA | 0xD2 | 0xDA => {
                let addr = self.fetch16(bus);
                if self.condition(y) {
                    bus.tick();
                    self.regs.pc = addr;
                }
            }
            0xC3 => {
                let addr = self.fetch16(bus);
                bus.tick();
                self.regs.pc = addr;
            }
            0xE9 => self.regs.pc = self.regs.hl(),
            0xC4 | 0xCC | 0xD4 | 0xDC => {
                let addr = self.fetch16(bus);
                if self.condition(y) {
                    let pc = self.regs.pc;
                    self.push16(bus, pc);
                    self.regs.pc = addr;
                }
            }
            0xCD => {
                let addr = self.fetch16(bus);
                let pc = self.regs.pc;
                self.push16(bus, pc);
                self.regs.pc = addr;
            }
            _ if opcode & 0xC7 == 0xC7 => {
                let pc = self.regs.pc;
                self.push16(bus, pc);
                self.regs.pc = u16::from(opcode & 0x38);
            }

            // 0xD3 0xDB 0xDD 0xE3 0xE4 0xEB 0xEC 0xED 0xF4 0xFC 0xFD
            _ => {
                self.lock = Some(IllegalOpcode {
                    pc: self.regs.pc.wrapping_sub(1),
                    opcode,
                });
            }
        }
    }

    fn execute_cb<B: CpuBus>(&mut self, bus: &mut B, opcode: u8) {
        let y = (opcode >> 3) & 7;
        let z = opcode & 7;
        let value = self.read_r8(bus, z);
        match opcode >> 6 {
            0 => {
                let result = alu::shift(&mut self.regs, y, value);
                self.write_r8(bus, z, result);
            }
            1 => alu::bit(&mut self.regs, y, value),
            2 => self.write_r8(bus, z, value & !(1 << y)),
            _ => self.write_r8(bus, z, value | (1 << y)),
        }
    }
}

fn read_cycle<B: CpuBus>(bus: &mut B, addr: u16) -> u8 {
    bus.tick();
    bus.read(addr)
}

fn write_cycle<B: CpuBus>(bus: &mut B, addr: u16, value: u8) {
    bus.tick();
    bus.write(addr, value);
}

/// IE & IF의 하위 5비트.
fn pending_interrupts<B: CpuBus>(bus: &mut B) -> u8 {
    bus.read(IE_ADDR) & bus.read(IF_ADDR) & 0x1F
}
