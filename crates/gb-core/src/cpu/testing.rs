//! CPU 테스트용 평면 64KB 메모리 버스.

use super::CpuBus;

pub struct FlatBus {
    pub mem: Vec<u8>,
    /// 진행한 M-사이클 수.
    pub cycles: u32,
}

impl FlatBus {
    /// 0x0100부터 `program`을 담은 버스.
    pub fn with_program(program: &[u8]) -> Self {
        let mut mem = vec![0; 0x10000];
        mem[0x0100..0x0100 + program.len()].copy_from_slice(program);
        Self { mem, cycles: 0 }
    }
}

impl CpuBus for FlatBus {
    fn read(&mut self, addr: u16) -> u8 {
        self.mem[usize::from(addr)]
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.mem[usize::from(addr)] = value;
    }

    fn tick(&mut self) {
        self.cycles += 1;
    }
}
