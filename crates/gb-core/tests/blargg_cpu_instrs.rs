//! Blargg cpu_instrs 인수 테스트 (스펙 §8-3). ROM은 `scripts/fetch-test-roms.sh`로 내려받는다.

use gb_core::{GameBoy, Model};
use std::path::PathBuf;

/// 에뮬레이션 시간 약 60초. 가장 오래 걸리는 ROM도 이 안에 끝난다.
const MAX_FRAMES: u32 = 60 * 60;

fn rom_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/roms/blargg/cpu_instrs/individual")
        .join(name)
}

/// ROM이 없으면 경고 후 건너뛴다. `ARAGORN_REQUIRE_TEST_ROMS`가 있으면(CI) 실패시킨다.
fn load_rom(name: &str) -> Option<Vec<u8>> {
    match std::fs::read(rom_path(name)) {
        Ok(rom) => Some(rom),
        Err(e) if std::env::var_os("ARAGORN_REQUIRE_TEST_ROMS").is_none() => {
            eprintln!(
                "경고: {name}을(를) 읽을 수 없어 건너뜁니다 ({e}). scripts/fetch-test-roms.sh로 내려받으세요."
            );
            None
        }
        Err(e) => panic!("{name}을(를) 읽을 수 없습니다: {e}"),
    }
}

/// 시리얼 출력에 "Passed"가 나올 때까지 실행한다. Blargg ROM은 CGB 플래그가 있지만 DMG로 돌린다.
fn run_blargg(name: &str) {
    let Some(rom) = load_rom(name) else {
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
            panic!("{name} 실패:\n{output}");
        }
        if let Some(lock) = gb.debug().illegal_opcode() {
            panic!(
                "{name}: 정의되지 않은 옵코드 {:#04X} (PC {:#06X})\n{output}",
                lock.opcode, lock.pc
            );
        }
    }
    panic!(
        "{name}: 시간 초과\n{}",
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
    special_01 => "01-special.gb",
    interrupts_02 => "02-interrupts.gb",
    op_sp_hl_03 => "03-op sp,hl.gb",
    op_r_imm_04 => "04-op r,imm.gb",
    op_rp_05 => "05-op rp.gb",
    ld_r_r_06 => "06-ld r,r.gb",
    jr_jp_call_ret_rst_07 => "07-jr,jp,call,ret,rst.gb",
    misc_instrs_08 => "08-misc instrs.gb",
    op_r_r_09 => "09-op r,r.gb",
    bit_ops_10 => "10-bit ops.gb",
    op_a_hl_11 => "11-op a,(hl).gb",
}
