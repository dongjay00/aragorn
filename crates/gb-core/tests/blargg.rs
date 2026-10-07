//! Blargg 테스트 ROM 인수 테스트 (스펙 §8-3). 결과는 시리얼 출력의 "Passed"/"Failed"로 판정한다.

mod common;

use gb_core::{GameBoy, Model};

/// 에뮬레이션 시간 약 60초. 가장 오래 걸리는 ROM도 이 안에 끝난다.
const MAX_FRAMES: u32 = 60 * 60;

/// Blargg ROM은 CGB 플래그가 있지만 DMG로 돌린다.
fn run_blargg(rel: &str) {
    let Some(rom) = common::load_rom(&format!("blargg/{rel}")) else {
        return;
    };
    let mut gb = GameBoy::new(rom, Model::Dmg).expect("테스트 ROM 로드");
    for _ in 0..MAX_FRAMES {
        gb.run_frame();
        let output = String::from_utf8_lossy(gb.debug().serial_output()).into_owned();
        if output.contains("Passed") {
            return;
        }
        if output.contains("Failed") {
            panic!("{rel} 실패:\n{output}");
        }
        if let Some(lock) = gb.debug().illegal_opcode() {
            panic!(
                "{rel}: 정의되지 않은 옵코드 {:#04X} (PC {:#06X})\n{output}",
                lock.opcode, lock.pc
            );
        }
    }
    panic!(
        "{rel}: 시간 초과\n{}",
        String::from_utf8_lossy(gb.debug().serial_output())
    );
}

macro_rules! blargg_tests {
    ($($test:ident => $file:literal,)*) => {
        $(
            #[test]
            fn $test() {
                run_blargg($file);
            }
        )*
    };
}

blargg_tests! {
    cpu_instrs_01_special => "cpu_instrs/individual/01-special.gb",
    cpu_instrs_02_interrupts => "cpu_instrs/individual/02-interrupts.gb",
    cpu_instrs_03_op_sp_hl => "cpu_instrs/individual/03-op sp,hl.gb",
    cpu_instrs_04_op_r_imm => "cpu_instrs/individual/04-op r,imm.gb",
    cpu_instrs_05_op_rp => "cpu_instrs/individual/05-op rp.gb",
    cpu_instrs_06_ld_r_r => "cpu_instrs/individual/06-ld r,r.gb",
    cpu_instrs_07_jr_jp_call_ret_rst => "cpu_instrs/individual/07-jr,jp,call,ret,rst.gb",
    cpu_instrs_08_misc_instrs => "cpu_instrs/individual/08-misc instrs.gb",
    cpu_instrs_09_op_r_r => "cpu_instrs/individual/09-op r,r.gb",
    cpu_instrs_10_bit_ops => "cpu_instrs/individual/10-bit ops.gb",
    cpu_instrs_11_op_a_hl => "cpu_instrs/individual/11-op a,(hl).gb",
    instr_timing => "instr_timing/instr_timing.gb",
    mem_timing_01_read => "mem_timing/individual/01-read_timing.gb",
    mem_timing_02_write => "mem_timing/individual/02-write_timing.gb",
    mem_timing_03_modify => "mem_timing/individual/03-modify_timing.gb",
}
