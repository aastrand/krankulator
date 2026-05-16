#[cfg(test)]
mod tests {
    use crate::emu;
    use crate::emu::io::loader;
    use crate::test_input;
    use crate::util;
    #[allow(unused_imports)]
    use util::get_status_str;

    #[test]
    fn test_instructions() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(
            test_input!("ascii/instructions"),
        )));
        emu.run();

        assert_eq!(emu.cpu.a, 0x84);
        assert_eq!(emu.cpu.x, 0xc1);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), true);
    }

    #[test]
    fn test_lda_sta() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!("ascii/ldasta"))));
        emu.run();

        assert_eq!(emu.cpu.a, 8);
        assert_eq!(emu.mem.cpu_read(0x200), 1);
        assert_eq!(emu.mem.cpu_read(0x201), 5);
        assert_eq!(emu.mem.cpu_read(0x202), 8);
    }

    #[test]
    fn test_transfers() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!("ascii/transfers"))));
        emu.run();

        assert_eq!(emu.cpu.a, 0x42);
        assert_eq!(emu.cpu.x, 0x42);
        assert_eq!(emu.cpu.y, 0x43);
        assert_eq!(emu.cpu.sp, 0x42);
        assert_eq!(emu.cpu.zero_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), false);
    }

    #[test]
    fn test_subtract_with_carry() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!("ascii/sbc"))));
        emu.run();

        assert_eq!(emu.cpu.a, 0xfc);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), true);
    }

    #[test]
    fn test_stores() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!("ascii/stores"))));
        emu.run();

        assert_eq!(emu.cpu.a, 1);
        assert_eq!(emu.cpu.x, 2);
        assert_eq!(emu.cpu.y, 3);
        assert_eq!(emu.mem.cpu_read(1), 1);
        assert_eq!(emu.mem.cpu_read(2), 2);
        assert_eq!(emu.mem.cpu_read(3), 3);
        assert_eq!(emu.mem.cpu_read(0x0100), 1);
        assert_eq!(emu.mem.cpu_read(0x0200), 2);
        assert_eq!(emu.mem.cpu_read(0x0300), 3);
    }

    #[test]
    fn test_compares() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!("ascii/compares"))));
        emu.run();

        assert_eq!(emu.cpu.a, 1);
        assert_eq!(emu.cpu.x, 0);
        assert_eq!(emu.cpu.y, 1);
        assert_eq!(emu.mem.cpu_read(0x100), 1);
        assert_eq!(emu.cpu.zero_flag(), true);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
    }

    #[test]
    fn test_bne() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!("ascii/bne"))));
        emu.run();

        assert_eq!(emu.cpu.x, 3);
        assert_eq!(emu.mem.cpu_read(0x0201), 3);
        assert_eq!(emu.mem.cpu_read(0x0200), 3);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_beq() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!("ascii/beq"))));
        emu.run();

        assert_eq!(emu.cpu.x, 1);
        assert_eq!(emu.mem.cpu_read(0x0201), 1);
        assert_eq!(emu.mem.cpu_read(0x0200), 1);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_take_no_branch() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(
            test_input!("ascii/take_no_branch"),
        )));
        emu.run();

        assert_eq!(emu.cpu.y, 8);
    }

    #[test]
    fn test_take_all_branches() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(
            test_input!("ascii/take_all_branches"),
        )));
        emu.run();

        assert_eq!(emu.cpu.x, 8);
    }

    #[test]
    fn test_stackloop() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!("ascii/stackloop"))));
        emu.run();

        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.x, 0x10);
        assert_eq!(emu.cpu.y, 0x20);
        assert_eq!(emu.cpu.sp, 0xff);

        for sp in 0..15 {
            assert_eq!(emu.mem.cpu_read(0x1ff - sp), sp as u8);
            assert_eq!(emu.mem.cpu_read(0x200 + sp), sp as u8);
            assert_eq!(emu.mem.cpu_read(0x21f - sp), sp as u8);
        }
    }

    #[test]
    fn test_jmp() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!("ascii/jmp"))));
        emu.run();

        assert_eq!(emu.cpu.a, 0x03);
        assert_eq!(emu.mem.cpu_read(0x200), 0x03);
    }

    #[test]
    fn test_jsrrts() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!("ascii/jsrtrs"))));
        emu.run();

        assert_eq!(emu.cpu.x, 0x15);
        assert_eq!(emu.cpu.sp, 0xfd);
        assert_eq!(emu.mem.cpu_read(0x1fe), 0x08);
        assert_eq!(emu.mem.cpu_read(0x1ff), 0x06);
    }

    #[test]
    fn test_klaus_2m5() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_bin(&String::from(
            test_input!("bin/6502_functional_test.bin"),
        )));
        emu.toggle_should_trigger_nmi(false);
        emu.apu.write(0x4017, 0x40, 0);
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);
        emu.run();

        assert_eq!(emu.cpu.pc, 0x3469);
    }

    #[test]
    fn test_nes_nestest() {
        let mut emu: emu::Emulator =
            emu::Emulator::new_headless(loader::load_nes(&String::from(test_input!("nes/nestest.nes"))));
        emu.cpu.pc = 0xc000;
        emu.cpu.sp = 0xfd;
        emu.cycles = 7;
        emu.cpu.cycle = 7;
        emu.ppu.cycle = 21;
        emu.cpu.set_status_flag(emu::cpu::INTERRUPT_BIT);

        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(false);
        emu.toggle_verbose_mode(true);

        if let Ok(lines) = util::read_lines(&String::from(test_input!("nes/nestest.log"))) {
            for line in lines {
                if let Ok(expected) = line {
                    println!("{}", expected);
                    let expected_addr = &expected[0..4];
                    let pc = emu.cpu.pc;

                    let expected_status = &expected[65..67];
                    let status = emu.cpu.status;

                    let expected_ppu_cycles: String = (&expected[82..85])
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect();
                    let expected_ppu_scanline: String = (&expected[78..81])
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect();
                    let expected_cycles = &expected[90..];

                    let mut cycles: u64 = emu.cpu.cycle;
                    let mut ppu_cycles: u16 = emu.ppu.cycle;
                    let mut ppu_scanline: u16 = emu.ppu.scanline;

                    'next_instr: loop {
                        let state = emu.cycle();
                        match state {
                            emu::CycleState::CpuAhead => {
                                cycles = emu.cpu.cycle;
                                ppu_cycles = emu.ppu.cycle;
                                ppu_scanline = emu.ppu.scanline;
                            }
                            _ => break 'next_instr,
                        }
                    }

                    assert_eq!(util::hex_str_to_u16(expected_addr).ok().unwrap(), pc);
                    assert_eq!(util::hex_str_to_u8(expected_status).ok().unwrap(), status);
                    assert_eq!(expected_cycles.parse::<u64>().unwrap(), cycles);
                    assert_eq!(expected_ppu_cycles.parse::<u16>().unwrap(), ppu_cycles);
                    assert_eq!(expected_ppu_scanline.parse::<u16>().unwrap(), ppu_scanline);
                } else {
                    panic!("{}", "Error iterating over nesttest.log");
                }
            }
        } else {
            panic!("{}", "Could not read nestest.log");
        }
    }

    #[test]
    fn test_nes_instr_test() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_nes(&String::from(
            test_input!("nes/official_only.nes"),
        )));
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        emu.run();

        let expected = String::from("All 16 tests passed\n\n\n");
        let buf = get_status_str(&mut emu, 0x6004, expected.len());

        assert_eq!(0, emu.mem.cpu_read(0x6000));
        assert_eq!(expected, buf);
    }

    #[test]
    fn test_nes_registers_after_reset() {
        let mut emu: emu::Emulator =
            emu::Emulator::new_headless(loader::load_nes(&String::from(test_input!("nes/registers.nes"))));
        emu.cpu.status = 0x34;
        emu.cpu.sp = 0xfd;
        emu.toggle_should_trigger_nmi(false);

        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        emu.run();
        emu.reset();
        emu.run();

        let expected = String::from("A  X  Y  P  S\n34 56 78 FF 0F \n\nregisters\n\nPassed\n");
        let buf = get_status_str(&mut emu, 0x6004, expected.len());

        assert_eq!(0, emu.mem.cpu_read(0x6000));
        assert_eq!(expected, buf);
    }

    #[test]
    fn test_nes_ram_after_reset() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_nes(&String::from(
            test_input!("nes/ram_after_reset.nes"),
        )));
        emu.cpu.status = 0x34;
        emu.cpu.sp = 0xfd;
        emu.toggle_should_trigger_nmi(false);

        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        emu.run();
        emu.reset();
        emu.run();

        let expected = String::from("\nram_after_reset\n\nPassed\n");
        let buf = get_status_str(&mut emu, 0x6004, expected.len());

        assert_eq!(0, emu.mem.cpu_read(0x6000));
        assert_eq!(expected, buf);
    }

    #[test]
    fn test_nes_oam_read() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_nes(&String::from(
            test_input!("nes/ppu/oam_read.nes"),
        )));
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        emu.run();

        let expected = String::from("----------------\n----------------\n----------------\n----------------\n----------------\n----------------\n----------------\n----------------\n----------------\n----------------\n----------------\n----------------\n----------------\n----------------\n----------------\n----------------\n\noam_read\n\nPassed\n");
        let buf = get_status_str(&mut emu, 0x6004, expected.len());

        assert_eq!(0, emu.mem.cpu_read(0x6000));
        assert_eq!(expected, buf);
    }

    #[test]
    fn test_nes_ppu_vbl_basics() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_nes(&String::from(
            test_input!("nes/ppu/vbl_nmi/01-vbl_basics.nes"),
        )));
        emu.cpu.status = 0x34;
        emu.cpu.sp = 0xfd;
        emu.toggle_should_trigger_nmi(true);

        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        emu.run();

        let expected = String::from("\n01-vbl_basics\n\nPassed\n");
        let buf = get_status_str(&mut emu, 0x6004, expected.len());

        assert_eq!(0, emu.mem.cpu_read(0x6000));
        assert_eq!(expected, buf);
    }

    #[test]
    fn test_nes_ppu_vbl_clear_time() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_nes(&String::from(
            test_input!("nes/ppu/vbl_nmi/03-vbl_clear_time.nes"),
        )));
        emu.cpu.status = 0x34;
        emu.cpu.sp = 0xfd;
        emu.toggle_should_trigger_nmi(true);

        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        emu.run();

        let expected = String::from(
            "00 V\n01 V\n02 V\n03 V\n04 V\n05 V\n06 -\n07 -\n08 -\n\n03-vbl_clear_time\n\nPassed\n",
        );
        let buf = get_status_str(&mut emu, 0x6004, expected.len());

        assert_eq!(0, emu.mem.cpu_read(0x6000));
        assert_eq!(expected, buf);
    }

    #[test]
    fn test_nes_ppu_nmi_control() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_nes(&String::from(
            test_input!("nes/ppu/vbl_nmi/04-nmi_control.nes"),
        )));
        emu.cpu.status = 0x34;
        emu.cpu.sp = 0xfd;
        emu.toggle_should_trigger_nmi(true);

        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        emu.run();

        let expected = String::from("\n04-nmi_control\n\nPassed\n");
        let buf = get_status_str(&mut emu, 0x6004, expected.len());

        assert_eq!(0, emu.mem.cpu_read(0x6000));
        assert_eq!(expected, buf);
    }

    #[test]
    fn test_nes_ppu_even_odd_frames() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_nes(&String::from(
            test_input!("nes/ppu/vbl_nmi/09-even_odd_frames.nes"),
        )));
        emu.cpu.status = 0x34;
        emu.cpu.sp = 0xfd;
        emu.toggle_should_trigger_nmi(true);

        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        emu.run();

        let expected = String::from("00 01 01 02 \n09-even_odd_frames\n\nPassed\n");
        let buf = get_status_str(&mut emu, 0x6004, expected.len());

        assert_eq!(0, emu.mem.cpu_read(0x6000));
        assert_eq!(expected, buf);
    }

    #[test]
    fn test_savestate_roundtrip_simple() {
        let mut emu = emu::Emulator::_new();
        emu.toggle_should_exit_on_infinite_loop(false);
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);

        emu.mem.cpu_write(0x600, 0xA9);
        emu.mem.cpu_write(0x601, 0x42);
        emu.mem.cpu_write(0x602, 0x85);
        emu.mem.cpu_write(0x603, 0x10);
        emu.mem.cpu_write(0x604, 0xEA);
        emu.mem.cpu_write(0x605, 0xEA);
        emu.mem.cpu_write(0x606, 0x00);

        for _ in 0..20 {
            emu.cycle();
        }
        assert_eq!(emu.cpu.a, 0x42);
        assert_eq!(emu.mem.cpu_read(0x10), 0x42);

        let saved = emu.save_state_to_bytes();
        let saved_pc = emu.cpu.pc;
        let saved_a = emu.cpu.a;
        let saved_cycles = emu.cycles;

        emu.cpu.a = 0x00;
        emu.cpu.pc = 0x600;
        emu.mem.cpu_write(0x10, 0x00);
        emu.cycles = 999999;

        assert_eq!(emu.cpu.a, 0x00);
        assert_eq!(emu.mem.cpu_read(0x10), 0x00);

        emu.load_state_from_bytes(&saved).unwrap();

        assert_eq!(emu.cpu.pc, saved_pc);
        assert_eq!(emu.cpu.a, saved_a);
        assert_eq!(emu.cycles, saved_cycles);
        assert_eq!(emu.mem.cpu_read(0x10), 0x42);
    }

    #[test]
    fn test_savestate_roundtrip_nestest() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_nes(&String::from(test_input!("nes/nestest.nes"))));
        emu.cpu.pc = 0xc000;
        emu.cpu.sp = 0xfd;
        emu.cycles = 7;
        emu.cpu.cycle = 7;
        emu.ppu.cycle = 21;
        emu.cpu.set_status_flag(emu::cpu::INTERRUPT_BIT);
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_should_exit_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        for _ in 0..5000 {
            emu.cycle();
        }

        let saved = emu.save_state_to_bytes();
        let saved_pc = emu.cpu.pc;
        let saved_a = emu.cpu.a;
        let saved_x = emu.cpu.x;
        let saved_y = emu.cpu.y;
        let saved_sp = emu.cpu.sp;
        let saved_status = emu.cpu.status;
        let saved_ppu_cycle = emu.ppu.cycle;
        let saved_ppu_scanline = emu.ppu.scanline;

        for _ in 0..5000 {
            emu.cycle();
        }
        assert_ne!(emu.cpu.pc, saved_pc);

        emu.load_state_from_bytes(&saved).unwrap();

        assert_eq!(emu.cpu.pc, saved_pc);
        assert_eq!(emu.cpu.a, saved_a);
        assert_eq!(emu.cpu.x, saved_x);
        assert_eq!(emu.cpu.y, saved_y);
        assert_eq!(emu.cpu.sp, saved_sp);
        assert_eq!(emu.cpu.status, saved_status);
        assert_eq!(emu.ppu.cycle, saved_ppu_cycle);
        assert_eq!(emu.ppu.scanline, saved_ppu_scanline);

        for _ in 0..5000 {
            emu.cycle();
        }

        let mut emu2 =
            emu::Emulator::new_headless(loader::load_nes(&String::from(test_input!("nes/nestest.nes"))));
        emu2.cpu.pc = 0xc000;
        emu2.cpu.sp = 0xfd;
        emu2.cycles = 7;
        emu2.cpu.cycle = 7;
        emu2.ppu.cycle = 21;
        emu2.cpu.set_status_flag(emu::cpu::INTERRUPT_BIT);
        emu2.toggle_debug_on_infinite_loop(false);
        emu2.toggle_should_exit_on_infinite_loop(false);
        emu2.toggle_quiet_mode(true);
        emu2.toggle_verbose_mode(false);

        for _ in 0..10000 {
            emu2.cycle();
        }

        assert_eq!(emu.cpu.pc, emu2.cpu.pc);
        assert_eq!(emu.cpu.a, emu2.cpu.a);
        assert_eq!(emu.cpu.x, emu2.cpu.x);
        assert_eq!(emu.cpu.y, emu2.cpu.y);
        assert_eq!(emu.cpu.sp, emu2.cpu.sp);
        assert_eq!(emu.cpu.status, emu2.cpu.status);
        assert_eq!(emu.cpu.cycle, emu2.cpu.cycle);
        assert_eq!(emu.ppu.cycle, emu2.ppu.cycle);
        assert_eq!(emu.ppu.scanline, emu2.ppu.scanline);
        assert_eq!(emu.cycles, emu2.cycles);
    }

    #[test]
    fn test_savestate_mapper_mismatch() {
        let mut emu1 =
            emu::Emulator::new_headless(loader::load_nes(&String::from(test_input!("nes/nestest.nes"))));
        emu1.toggle_quiet_mode(true);

        let saved = emu1.save_state_to_bytes();

        let mut emu2 = emu::Emulator::_new();
        let result = emu2.load_state_from_bytes(&saved);
        assert!(result.is_err());
    }
}
