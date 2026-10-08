//! Blargg cgb_sound 인수 테스트 (CGB 모드) (스펙 §8-3). 이 ROM들은 시리얼로 결과를 보내지 않고 카트리지 RAM에 남긴다:
//! $A001–$A003에 서명 DE B0 61, $A000에 상태(0x80 실행 중, 0 통과, 그 밖은 실패), $A004부터 결과 문구.

mod common;

use gb_core::{GameBoy, Model};

/// 에뮬레이션 시간 약 60초.
const MAX_FRAMES: u32 = 60 * 60;
const SIGNATURE: [u8; 3] = [0xDE, 0xB0, 0x61];
const RUNNING: u8 = 0x80;

fn run_cgb_sound(name: &str) {
    let Some(rom) = common::load_rom(&format!("blargg/cgb_sound/rom_singles/{name}")) else {
        return;
    };
    let mut gb = GameBoy::new(rom, Model::Cgb).expect("테스트 ROM 로드");
    for _ in 0..MAX_FRAMES {
        gb.run_frame();
        let debug = gb.debug();
        let signature = [debug.peek(0xA001), debug.peek(0xA002), debug.peek(0xA003)];
        let status = debug.peek(0xA000);
        if signature != SIGNATURE || status == RUNNING {
            continue;
        }
        let text: Vec<u8> = (0xA004..0xBFFF)
            .map(|addr| debug.peek(addr))
            .take_while(|&b| b != 0)
            .collect();
        assert_eq!(
            status,
            0,
            "{name} 실패:\n{}",
            String::from_utf8_lossy(&text)
        );
        return;
    }
    panic!("{name}: 시간 초과");
}

macro_rules! cgb_sound_tests {
    ($($test:ident => $file:literal,)*) => {
        $(
            #[test]
            fn $test() {
                run_cgb_sound($file);
            }
        )*
    };
}

cgb_sound_tests! {
    cgb_sound_01_registers => "01-registers.gb",
    cgb_sound_02_len_ctr => "02-len ctr.gb",
    cgb_sound_03_trigger => "03-trigger.gb",
    cgb_sound_04_sweep => "04-sweep.gb",
    cgb_sound_05_sweep_details => "05-sweep details.gb",
    cgb_sound_06_overflow_on_trigger => "06-overflow on trigger.gb",
    cgb_sound_07_len_sweep_period_sync => "07-len sweep period sync.gb",
    cgb_sound_08_len_ctr_during_power => "08-len ctr during power.gb",
    cgb_sound_09_wave_read_while_on => "09-wave read while on.gb",
    cgb_sound_10_wave_trigger_while_on => "10-wave trigger while on.gb",
    cgb_sound_11_regs_after_power => "11-regs after power.gb",
    cgb_sound_12_wave => "12-wave.gb",
}
