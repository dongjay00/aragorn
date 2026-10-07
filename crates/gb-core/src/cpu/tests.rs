use super::testing::FlatBus;
use super::*;

/// PC=0x0100, SP=0xD000, HL=0xC000, 나머지 0인 CPU.
fn cpu() -> Cpu {
    let mut regs = Registers {
        sp: 0xD000,
        pc: 0x0100,
        ..Registers::default()
    };
    regs.set_hl(0xC000);
    Cpu::new(regs)
}

/// 프로그램을 0x0100에 놓고 `setup` 후 `steps`번 실행한다.
fn run(program: &[u8], steps: usize, setup: impl FnOnce(&mut Cpu, &mut FlatBus)) -> (Cpu, FlatBus) {
    let mut bus = FlatBus::with_program(program);
    let mut cpu = cpu();
    setup(&mut cpu, &mut bus);
    for _ in 0..steps {
        cpu.step(&mut bus);
    }
    (cpu, bus)
}

/// Blargg instr_timing의 옵코드별 M-사이클 표. F=0일 때 기준이라 NZ/NC 조건은 분기하고
/// Z/C 조건은 분기하지 않는다. 0은 여기서 재지 않는 옵코드다 (STOP, HALT, CB 접두, 정의되지 않음).
#[rustfmt::skip]
const OP_CYCLES_F0: [u8; 256] = [
    1,3,2,2,1,1,2,1,5,2,2,2,1,1,2,1,
    0,3,2,2,1,1,2,1,3,2,2,2,1,1,2,1,
    3,3,2,2,1,1,2,1,2,2,2,2,1,1,2,1,
    3,3,2,2,3,3,3,1,2,2,2,2,1,1,2,1,
    1,1,1,1,1,1,2,1,1,1,1,1,1,1,2,1,
    1,1,1,1,1,1,2,1,1,1,1,1,1,1,2,1,
    1,1,1,1,1,1,2,1,1,1,1,1,1,1,2,1,
    2,2,2,2,2,2,0,2,1,1,1,1,1,1,2,1,
    1,1,1,1,1,1,2,1,1,1,1,1,1,1,2,1,
    1,1,1,1,1,1,2,1,1,1,1,1,1,1,2,1,
    1,1,1,1,1,1,2,1,1,1,1,1,1,1,2,1,
    1,1,1,1,1,1,2,1,1,1,1,1,1,1,2,1,
    5,3,4,4,6,4,2,4,2,4,3,0,3,6,2,4,
    5,3,4,0,6,4,2,4,2,4,3,0,3,0,2,4,
    3,3,2,0,0,4,2,4,4,1,4,0,0,0,2,4,
    3,3,2,1,0,4,2,4,3,2,4,1,0,0,2,4,
];

#[test]
fn unprefixed_opcode_cycles_match_hardware() {
    let mut wrong = Vec::new();
    for op in 0..=0xFFu8 {
        let expected = OP_CYCLES_F0[usize::from(op)];
        if expected == 0 {
            continue;
        }
        let (_, bus) = run(&[op, 0x00, 0x00], 1, |_, _| {});
        if bus.cycles != u32::from(expected) {
            wrong.push(format!("{op:#04X}: {} (기대 {expected})", bus.cycles));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn branch_cycles_with_z_and_c_set() {
    let cases = [
        (0x20, 2),
        (0x28, 3),
        (0x30, 2),
        (0x38, 3),
        (0xC0, 2),
        (0xC8, 5),
        (0xD0, 2),
        (0xD8, 5),
        (0xC2, 3),
        (0xCA, 4),
        (0xD2, 3),
        (0xDA, 4),
        (0xC4, 3),
        (0xCC, 6),
        (0xD4, 3),
        (0xDC, 6),
    ];
    for (op, expected) in cases {
        let (_, bus) = run(&[op, 0x00, 0x00], 1, |c, _| c.regs.f = flag::Z | flag::C);
        assert_eq!(bus.cycles, expected, "{op:#04X}");
    }
}

#[test]
fn cb_opcode_cycles_match_hardware() {
    for op in 0..=0xFFu8 {
        let expected = match (op & 7, op >> 6) {
            (6, 1) => 3,
            (6, _) => 4,
            _ => 2,
        };
        let (_, bus) = run(&[0xCB, op], 1, |_, _| {});
        assert_eq!(bus.cycles, expected, "CB {op:#04X}");
    }
}

#[test]
fn ld_r_r_copies_register() {
    let (cpu, bus) = run(&[0x41], 1, |c, _| c.regs.c = 0x42); // LD B,C
    assert_eq!((cpu.regs.b, cpu.regs.pc, bus.cycles), (0x42, 0x0101, 1));
}

#[test]
fn ld_hl_immediate_writes_memory() {
    let (_, bus) = run(&[0x36, 0x99], 1, |_, _| {}); // LD (HL),0x99
    assert_eq!(bus.mem[0xC000], 0x99);
}

#[test]
fn ldi_and_ldd_move_hl() {
    // LD (HL+),A ; LD A,(HL-)
    let (cpu, bus) = run(&[0x22, 0x3A], 2, |c, b| {
        c.regs.a = 5;
        b.mem[0xC001] = 7;
    });
    assert_eq!((bus.mem[0xC000], cpu.regs.a, cpu.regs.hl()), (5, 7, 0xC000));
}

#[test]
fn ld_nn_sp_stores_little_endian() {
    let (_, bus) = run(&[0x08, 0x00, 0xC0], 1, |c, _| c.regs.sp = 0xBEEF);
    assert_eq!((bus.mem[0xC000], bus.mem[0xC001]), (0xEF, 0xBE));
}

#[test]
fn inc_hl_indirect_updates_memory_and_flags() {
    let (cpu, bus) = run(&[0x34], 1, |_, b| b.mem[0xC000] = 0xFF);
    assert_eq!((bus.mem[0xC000], cpu.regs.f), (0x00, flag::Z | flag::H));
}

#[test]
fn push_and_pop_af_masks_flags() {
    // PUSH BC ; POP AF
    let (cpu, bus) = run(&[0xC5, 0xF1], 2, |c, _| c.regs.set_bc(0x12FF));
    assert_eq!((bus.mem[0xCFFF], bus.mem[0xCFFE]), (0x12, 0xFF));
    assert_eq!((cpu.regs.af(), cpu.regs.sp), (0x12F0, 0xD000));
}

#[test]
fn push_wraps_stack_pointer() {
    let (cpu, bus) = run(&[0xC5], 1, |c, _| {
        c.regs.sp = 0x0000;
        c.regs.set_bc(0xABCD);
    });
    assert_eq!(
        (cpu.regs.sp, bus.mem[0xFFFF], bus.mem[0xFFFE]),
        (0xFFFE, 0xAB, 0xCD)
    );
}

#[test]
fn call_pushes_return_address_and_ret_pops_it() {
    let (cpu, bus) = run(&[0xCD, 0x00, 0x02], 1, |_, _| {}); // CALL 0x0200
    assert_eq!((cpu.regs.pc, cpu.regs.sp), (0x0200, 0xCFFE));
    assert_eq!((bus.mem[0xCFFF], bus.mem[0xCFFE]), (0x01, 0x03));
    let (cpu, _) = run(&[0xCD, 0x00, 0x02], 2, |_, b| b.mem[0x0200] = 0xC9); // RET
    assert_eq!((cpu.regs.pc, cpu.regs.sp), (0x0103, 0xD000));
}

#[test]
fn jr_negative_offset() {
    let (cpu, _) = run(&[0x18, 0xFE], 1, |_, _| {});
    assert_eq!(cpu.regs.pc, 0x0100);
}

#[test]
fn conditional_jump_not_taken_skips_operand() {
    let (cpu, bus) = run(&[0xCA, 0x00, 0x02], 1, |_, _| {}); // JP Z (Z=0)
    assert_eq!((cpu.regs.pc, bus.cycles), (0x0103, 3));
}

#[test]
fn rst_pushes_return_address() {
    let (cpu, bus) = run(&[0xFF], 1, |_, _| {});
    assert_eq!(cpu.regs.pc, 0x0038);
    assert_eq!((bus.mem[0xCFFF], bus.mem[0xCFFE]), (0x01, 0x01));
}

#[test]
fn cb_bit_res_swap() {
    let (cpu, _) = run(&[0xCB, 0x7C], 1, |c, _| c.regs.h = 0x80); // BIT 7,H
    assert_eq!(cpu.regs.f, flag::H);
    let (_, bus) = run(&[0xCB, 0x86], 1, |_, b| b.mem[0xC000] = 0xFF); // RES 0,(HL)
    assert_eq!(bus.mem[0xC000], 0xFE);
    let (cpu, _) = run(&[0xCB, 0x37], 1, |c, _| c.regs.a = 0x12); // SWAP A
    assert_eq!(cpu.regs.a, 0x21);
}

#[test]
fn add_sp_and_ld_hl_sp_e() {
    let (cpu, _) = run(&[0xE8, 0xFF], 1, |_, _| {}); // ADD SP,-1
    assert_eq!((cpu.regs.sp, cpu.regs.f), (0xCFFF, 0));
    let (cpu, _) = run(&[0xF8, 0x02], 1, |c, _| c.regs.sp = 0xFFF8); // LD HL,SP+2
    assert_eq!((cpu.regs.hl(), cpu.regs.sp), (0xFFFA, 0xFFF8));
}

#[test]
fn ldh_writes_high_page() {
    let (_, bus) = run(&[0xE0, 0x80], 1, |c, _| c.regs.a = 0x77);
    assert_eq!(bus.mem[0xFF80], 0x77);
}

#[test]
fn rotate_a_clears_zero_flag() {
    let (cpu, _) = run(&[0x07], 1, |_, _| {}); // RLCA, A=0
    assert_eq!((cpu.regs.a, cpu.regs.f), (0, 0));
}

#[test]
fn pc_wraps_around_at_ffff() {
    let (cpu, _) = run(&[], 1, |c, _| c.regs.pc = 0xFFFF); // 0xFFFF의 NOP
    assert_eq!(cpu.regs.pc, 0x0000);
}

#[test]
fn illegal_opcode_locks_cpu() {
    let (cpu, bus) = run(&[0xD3, 0x00], 3, |_, _| {});
    assert_eq!(
        cpu.lock(),
        Some(IllegalOpcode {
            pc: 0x0100,
            opcode: 0xD3
        })
    );
    assert_eq!((cpu.regs.pc, bus.cycles), (0x0101, 3));
}

#[test]
fn all_illegal_opcodes_lock() {
    for op in [
        0xD3, 0xDB, 0xDD, 0xE3, 0xE4, 0xEB, 0xEC, 0xED, 0xF4, 0xFC, 0xFD,
    ] {
        let (cpu, _) = run(&[op], 1, |_, _| {});
        assert!(cpu.lock().is_some(), "{op:#04X}");
    }
}

#[test]
fn ei_enables_ime_after_next_instruction() {
    let (cpu, _) = run(&[0xFB, 0x00], 1, |_, _| {});
    assert!(!cpu.ime());
    let (cpu, _) = run(&[0xFB, 0x00], 2, |_, _| {});
    assert!(cpu.ime());
}

#[test]
fn di_right_after_ei_keeps_ime_off() {
    let (cpu, _) = run(&[0xFB, 0xF3, 0x00], 3, |_, _| {});
    assert!(!cpu.ime());
}

#[test]
fn reti_enables_ime_immediately() {
    let (cpu, _) = run(&[0xD9], 1, |_, b| {
        b.mem[0xD000] = 0x34;
        b.mem[0xD001] = 0x12;
    });
    assert!(cpu.ime());
    assert_eq!(cpu.regs.pc, 0x1234);
}

#[test]
fn halt_sets_halted_when_no_interrupt_pending() {
    let (cpu, _) = run(&[0x76], 1, |_, _| {});
    assert!(cpu.halted());
}

#[test]
fn halt_bug_reads_next_byte_twice() {
    // IME=0이고 인터럽트가 대기 중이면 HALT 다음 INC A가 두 번 실행된다.
    let (cpu, _) = run(&[0x76, 0x3C], 3, |_, b| {
        b.mem[0xFFFF] = 0x01;
        b.mem[0xFF0F] = 0x01;
    });
    assert_eq!((cpu.regs.a, cpu.regs.pc, cpu.halted()), (2, 0x0102, false));
}

/// IE, IF, IME를 설정한다.
fn with_interrupt(ie: u8, iflag: u8, ime: bool) -> impl FnOnce(&mut Cpu, &mut FlatBus) {
    move |c: &mut Cpu, b: &mut FlatBus| {
        b.mem[0xFFFF] = ie;
        b.mem[0xFF0F] = iflag;
        c.ime = ime;
    }
}

#[test]
fn dispatches_timer_interrupt() {
    let (cpu, bus) = run(&[0x00], 1, with_interrupt(0x04, 0x04, true));
    assert_eq!((cpu.regs.pc, cpu.regs.sp, bus.cycles), (0x0050, 0xCFFE, 5));
    assert_eq!((bus.mem[0xCFFF], bus.mem[0xCFFE]), (0x01, 0x00));
    assert_eq!(bus.mem[0xFF0F], 0x00);
    assert!(!cpu.ime());
}

#[test]
fn highest_priority_interrupt_wins() {
    let (cpu, bus) = run(&[0x00], 1, with_interrupt(0x1F, 0x06, true));
    assert_eq!((cpu.regs.pc, bus.mem[0xFF0F]), (0x0048, 0x04));
}

#[test]
fn ignores_upper_ie_bits() {
    let (cpu, _) = run(&[0x00], 1, with_interrupt(0xE0, 0xFF, true));
    assert_eq!(cpu.regs.pc, 0x0101);
}

#[test]
fn no_dispatch_when_ime_off() {
    let (cpu, _) = run(&[0x00], 1, with_interrupt(0x01, 0x01, false));
    assert_eq!(cpu.regs.pc, 0x0101);
}

#[test]
fn halt_waits_one_cycle_per_step() {
    let (cpu, bus) = run(&[0x76, 0x3C], 3, |_, _| {});
    assert!(cpu.halted());
    assert_eq!((cpu.regs.pc, cpu.regs.a, bus.cycles), (0x0101, 0, 3));
}

#[test]
fn interrupt_wakes_halt_without_ime() {
    let mut bus = FlatBus::with_program(&[0x76, 0x3C]);
    let mut cpu = cpu();
    bus.mem[0xFFFF] = 0x01;
    cpu.step(&mut bus); // HALT
    cpu.step(&mut bus); // 대기
    assert!(cpu.halted());
    bus.mem[0xFF0F] = 0x01;
    cpu.step(&mut bus); // 깨어나서 INC A
    assert!(!cpu.halted());
    assert_eq!((cpu.regs.a, cpu.regs.pc), (1, 0x0102));
}

#[test]
fn interrupt_wakes_halt_and_dispatches_with_ime() {
    let mut bus = FlatBus::with_program(&[0x76, 0x3C]);
    let mut cpu = cpu();
    cpu.ime = true;
    bus.mem[0xFFFF] = 0x01;
    cpu.step(&mut bus); // HALT
    bus.mem[0xFF0F] = 0x01;
    cpu.step(&mut bus); // 디스패치
    assert_eq!(cpu.regs.pc, 0x0040);
    assert_eq!((bus.mem[0xCFFF], bus.mem[0xCFFE]), (0x01, 0x01));
}

#[test]
fn interrupt_dispatches_after_instruction_following_ei() {
    let (cpu, bus) = run(&[0xFB, 0x00, 0x00], 3, with_interrupt(0x01, 0x01, false));
    assert_eq!(cpu.regs.pc, 0x0040);
    assert_eq!((bus.mem[0xCFFF], bus.mem[0xCFFE]), (0x01, 0x02));
}

#[test]
fn locked_cpu_ignores_interrupts() {
    let mut bus = FlatBus::with_program(&[0xD3]);
    let mut cpu = cpu();
    cpu.step(&mut bus);
    cpu.ime = true;
    bus.mem[0xFFFF] = 0x01;
    bus.mem[0xFF0F] = 0x01;
    cpu.step(&mut bus);
    assert_eq!(cpu.regs.pc, 0x0101);
    assert!(cpu.lock().is_some());
}

#[test]
fn ei_then_halt_with_pending_interrupt_returns_to_halt() {
    // EI ; HALT, IE=IF=1, 핸들러 0x40은 INC B. 핸들러 첫 바이트가 두 번 실행되면 안 되고,
    // 복귀 주소는 HALT(0x0101)여야 한다.
    let (cpu, bus) = run(&[0xFB, 0x76, 0x00], 4, |_, b| {
        b.mem[0xFFFF] = 0x01;
        b.mem[0xFF0F] = 0x01;
        b.mem[0x0040] = 0x04;
    });
    assert_eq!((cpu.regs.pc, cpu.regs.b), (0x0041, 1));
    assert_eq!((bus.mem[0xCFFF], bus.mem[0xCFFE]), (0x01, 0x01));
}

#[test]
fn interrupt_raised_during_fetch_cycle_preempts_that_instruction() {
    // NOP ; INC A. 두 번째 fetch 사이클에 타이머 IF가 켜지면 INC A 대신 디스패치한다.
    let (cpu, bus) = run(&[0x00, 0x3C], 2, |c, b| {
        c.ime = true;
        b.mem[0xFFFF] = 0x04;
        b.irq_at = Some((2, 0x04));
    });
    assert_eq!((cpu.regs.pc, cpu.regs.a, bus.cycles), (0x0050, 0, 6));
    assert_eq!((bus.mem[0xCFFF], bus.mem[0xCFFE]), (0x01, 0x01));
}

#[test]
fn ie_overwritten_by_upper_pc_push_cancels_dispatch() {
    // SP=0x0000이면 PC 상위 바이트(0x02)가 IE(0xFFFF)에 써진다. VBlank 비트가 꺼지므로 취소된다.
    let (cpu, bus) = run(&[], 1, |c, b| {
        c.ime = true;
        c.regs.pc = 0x0200;
        c.regs.sp = 0x0000;
        b.mem[0xFFFF] = 0x01;
        b.mem[0xFF0F] = 0x01;
    });
    assert_eq!((cpu.regs.pc, cpu.ime(), bus.cycles), (0x0000, false, 5));
    assert_eq!(
        (bus.mem[0xFFFF], bus.mem[0xFFFE], bus.mem[0xFF0F]),
        (0x02, 0x00, 0x01)
    );
}

#[test]
fn ie_overwritten_by_lower_pc_push_does_not_cancel() {
    // SP=0x0001이면 하위 바이트가 IE에 써지지만 벡터는 이미 정해졌다.
    let (cpu, bus) = run(&[], 1, |c, b| {
        c.ime = true;
        c.regs.pc = 0x0200;
        c.regs.sp = 0x0001;
        b.mem[0xFFFF] = 0x01;
        b.mem[0xFF0F] = 0x01;
    });
    assert_eq!(cpu.regs.pc, 0x0040);
    assert_eq!(
        (bus.mem[0x0000], bus.mem[0xFFFF], bus.mem[0xFF0F]),
        (0x02, 0x00, 0x00)
    );
}

#[test]
fn halt_with_ime_dispatches_in_the_wake_cycle() {
    // HALT 중 2번째 사이클에 IF가 켜지면 그 사이클이 fetch 사이클이 되어 바로 디스패치한다.
    let (cpu, bus) = run(&[0x76, 0x00], 2, |c, b| {
        c.ime = true;
        b.mem[0xFFFF] = 0x01;
        b.irq_at = Some((2, 0x01));
    });
    assert_eq!((cpu.regs.pc, bus.cycles), (0x0040, 6));
    assert_eq!((bus.mem[0xCFFF], bus.mem[0xCFFE]), (0x01, 0x01));
}

#[test]
fn halt_without_ime_wakes_and_executes_in_the_same_cycle() {
    let (cpu, bus) = run(&[0x76, 0x3C], 2, |_, b| {
        b.mem[0xFFFF] = 0x01;
        b.irq_at = Some((2, 0x01));
    });
    assert_eq!(
        (cpu.regs.a, cpu.regs.pc, cpu.halted(), bus.cycles),
        (1, 0x0102, false, 2)
    );
}

#[test]
fn dispatch_right_after_ei_keeps_ime_off_in_handler() {
    // IME=1에서 EI ; NOP. NOP의 fetch 사이클에 인터럽트가 디스패치되면 EI의 지연된 IME 켜기는
    // 사라져야 한다. 핸들러 첫 명령 뒤에도 IME=0이어야 중첩 인터럽트가 생기지 않는다.
    let (cpu, _) = run(&[0xFB, 0x00], 3, |c, b| {
        c.ime = true;
        b.mem[0xFFFF] = 0x04;
        b.irq_at = Some((2, 0x04));
    });
    assert_eq!(cpu.regs.pc, 0x0051);
    assert!(!cpu.ime());
}
