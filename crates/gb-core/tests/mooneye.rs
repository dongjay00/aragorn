//! mooneye-test-suite acceptance 인수 테스트 (스펙 §1.6-3).
//! 테스트는 끝날 때 시리얼로 피보나치 수 3,5,8,13,21,34(통과) 또는 0x42 6개(실패)를 보낸다.

mod common;

use gb_core::{GameBoy, Model};

const PASS: [u8; 6] = [3, 5, 8, 13, 21, 34];
/// 에뮬레이션 시간 약 10초.
const MAX_FRAMES: u32 = 10 * 60;

fn run_mooneye(rel: &str) {
    let Some(rom) = common::load_rom(&format!("mooneye/{rel}")) else {
        return;
    };
    let mut gb = GameBoy::new(rom, Model::Dmg).expect("테스트 ROM 로드");
    for _ in 0..MAX_FRAMES {
        gb.run_frame();
        let output = gb.debug().serial_output();
        if output.len() >= PASS.len() {
            assert_eq!(
                output[..PASS.len()],
                PASS,
                "{rel} 실패 (0x42는 assert 실패)"
            );
            return;
        }
        if let Some(lock) = gb.debug().illegal_opcode() {
            panic!(
                "{rel}: 정의되지 않은 옵코드 {:#04X} (PC {:#06X})",
                lock.opcode, lock.pc
            );
        }
    }
    panic!(
        "{rel}: 시간 초과 (시리얼 출력 {:?})",
        gb.debug().serial_output()
    );
}

macro_rules! mooneye_tests {
    ($($test:ident => $file:literal,)*) => {
        $(
            #[test]
            fn $test() {
                run_mooneye($file);
            }
        )*
    };
}

mooneye_tests! {
    bits_mem_oam => "acceptance/bits/mem_oam.gb",
    bits_reg_f => "acceptance/bits/reg_f.gb",
    boot_div_dmg_abc_mgb => "acceptance/boot_div-dmgABCmgb.gb",
    boot_regs_dmg_abc => "acceptance/boot_regs-dmgABC.gb",
    di_timing => "acceptance/di_timing-GS.gb",
    div_timing => "acceptance/div_timing.gb",
    ei_sequence => "acceptance/ei_sequence.gb",
    ei_timing => "acceptance/ei_timing.gb",
    halt_ime0_ei => "acceptance/halt_ime0_ei.gb",
    halt_ime0_nointr_timing => "acceptance/halt_ime0_nointr_timing.gb",
    halt_ime1_timing => "acceptance/halt_ime1_timing.gb",
    halt_ime1_timing2 => "acceptance/halt_ime1_timing2-GS.gb",
    if_ie_registers => "acceptance/if_ie_registers.gb",
    instr_daa => "acceptance/instr/daa.gb",
    interrupts_ie_push => "acceptance/interrupts/ie_push.gb",
    intr_timing => "acceptance/intr_timing.gb",
    oam_dma_reg_read => "acceptance/oam_dma/reg_read.gb",
    pop_timing => "acceptance/pop_timing.gb",
    rapid_di_ei => "acceptance/rapid_di_ei.gb",
    reti_intr_timing => "acceptance/reti_intr_timing.gb",
    timer_div_write => "acceptance/timer/div_write.gb",
    timer_rapid_toggle => "acceptance/timer/rapid_toggle.gb",
    timer_tim00 => "acceptance/timer/tim00.gb",
    timer_tim00_div_trigger => "acceptance/timer/tim00_div_trigger.gb",
    timer_tim01 => "acceptance/timer/tim01.gb",
    timer_tim01_div_trigger => "acceptance/timer/tim01_div_trigger.gb",
    timer_tim10 => "acceptance/timer/tim10.gb",
    timer_tim10_div_trigger => "acceptance/timer/tim10_div_trigger.gb",
    timer_tim11 => "acceptance/timer/tim11.gb",
    timer_tim11_div_trigger => "acceptance/timer/tim11_div_trigger.gb",
    timer_tima_reload => "acceptance/timer/tima_reload.gb",
    timer_tima_write_reloading => "acceptance/timer/tima_write_reloading.gb",
    timer_tma_write_reloading => "acceptance/timer/tma_write_reloading.gb",
    add_sp_e_timing => "acceptance/add_sp_e_timing.gb",
    call_cc_timing => "acceptance/call_cc_timing.gb",
    call_cc_timing2 => "acceptance/call_cc_timing2.gb",
    call_timing => "acceptance/call_timing.gb",
    call_timing2 => "acceptance/call_timing2.gb",
    jp_cc_timing => "acceptance/jp_cc_timing.gb",
    jp_timing => "acceptance/jp_timing.gb",
    ld_hl_sp_e_timing => "acceptance/ld_hl_sp_e_timing.gb",
    oam_dma_basic => "acceptance/oam_dma/basic.gb",
    oam_dma_restart => "acceptance/oam_dma_restart.gb",
    oam_dma_start => "acceptance/oam_dma_start.gb",
    oam_dma_timing => "acceptance/oam_dma_timing.gb",
    push_timing => "acceptance/push_timing.gb",
    ret_cc_timing => "acceptance/ret_cc_timing.gb",
    ret_timing => "acceptance/ret_timing.gb",
    reti_timing => "acceptance/reti_timing.gb",
    rst_timing => "acceptance/rst_timing.gb",
}
