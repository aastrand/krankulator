#[cfg(test)]
mod tests {
    use crate::emu;
    use crate::emu::io::loader;
    use crate::test_input;
    use crate::test_rom;
    use crate::util;
    #[allow(unused_imports)]
    use util::get_status_str;

    #[test]
    fn test_instructions() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!(
            "ascii/instructions"
        ))));
        emu.run();

        assert_eq!(emu.cpu.a, 0x84);
        assert_eq!(emu.cpu.x, 0xc1);
        assert!(emu.cpu.carry_flag());
        assert!(emu.cpu.negative_flag());
    }

    #[test]
    fn test_lda_sta() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!(
            "ascii/ldasta"
        ))));
        emu.run();

        assert_eq!(emu.cpu.a, 8);
        assert_eq!(emu.mem.cpu_read(0x200), 1);
        assert_eq!(emu.mem.cpu_read(0x201), 5);
        assert_eq!(emu.mem.cpu_read(0x202), 8);
    }

    #[test]
    fn test_transfers() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!(
            "ascii/transfers"
        ))));
        emu.run();

        assert_eq!(emu.cpu.a, 0x42);
        assert_eq!(emu.cpu.x, 0x42);
        assert_eq!(emu.cpu.y, 0x43);
        assert_eq!(emu.cpu.sp, 0x42);
        assert!(!emu.cpu.zero_flag());
        assert!(!emu.cpu.negative_flag());
    }

    #[test]
    fn test_subtract_with_carry() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!(
            "ascii/sbc"
        ))));
        emu.run();

        assert_eq!(emu.cpu.a, 0xfc);
        assert!(emu.cpu.carry_flag());
        assert!(emu.cpu.negative_flag());
    }

    #[test]
    fn test_stores() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!(
            "ascii/stores"
        ))));
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
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!(
            "ascii/compares"
        ))));
        emu.run();

        assert_eq!(emu.cpu.a, 1);
        assert_eq!(emu.cpu.x, 0);
        assert_eq!(emu.cpu.y, 1);
        assert_eq!(emu.mem.cpu_read(0x100), 1);
        assert!(emu.cpu.zero_flag());
        assert!(emu.cpu.carry_flag());
        assert!(!emu.cpu.negative_flag());
    }

    #[test]
    fn test_bne() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!(
            "ascii/bne"
        ))));
        emu.run();

        assert_eq!(emu.cpu.x, 3);
        assert_eq!(emu.mem.cpu_read(0x0201), 3);
        assert_eq!(emu.mem.cpu_read(0x0200), 3);
        assert!(emu.cpu.zero_flag());
    }

    #[test]
    fn test_beq() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!(
            "ascii/beq"
        ))));
        emu.run();

        assert_eq!(emu.cpu.x, 1);
        assert_eq!(emu.mem.cpu_read(0x0201), 1);
        assert_eq!(emu.mem.cpu_read(0x0200), 1);
        assert!(!emu.cpu.zero_flag());
    }

    #[test]
    fn test_take_no_branch() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!(
            "ascii/take_no_branch"
        ))));
        emu.run();

        assert_eq!(emu.cpu.y, 8);
    }

    #[test]
    fn test_take_all_branches() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!(
            "ascii/take_all_branches"
        ))));
        emu.run();

        assert_eq!(emu.cpu.x, 8);
    }

    #[test]
    fn test_stackloop() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!(
            "ascii/stackloop"
        ))));
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
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!(
            "ascii/jmp"
        ))));
        emu.run();

        assert_eq!(emu.cpu.a, 0x03);
        assert_eq!(emu.mem.cpu_read(0x200), 0x03);
    }

    #[test]
    fn test_jsrrts() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(test_input!(
            "ascii/jsrtrs"
        ))));
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
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_nes(&String::from(
            test_rom!("other/nestest.nes"),
        )));
        emu.cpu.pc = 0xc000;
        emu.cpu.sp = 0xfd;
        emu.cycles = 7;
        emu.cpu.cycle = 7;
        emu.ppu.cycle = 21;
        emu.cpu.set_status_flag(emu::cpu::INTERRUPT_BIT);

        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(false);
        emu.toggle_verbose_mode(true);

        if let Ok(lines) = util::read_lines(String::from(test_rom!("other/nestest.log"))) {
            for line in lines {
                if let Ok(expected) = line {
                    println!("{expected}");
                    let expected_addr = &expected[0..4];
                    let pc = emu.cpu.pc;

                    let expected_status = &expected[65..67];
                    let status = emu.cpu.status;

                    let expected_ppu_cycles: String = expected[82..85]
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect();
                    let expected_ppu_scanline: String = expected[78..81]
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
            test_rom!("instr_test-v5/official_only.nes"),
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
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_nes(&String::from(
            test_rom!("cpu_reset/registers.nes"),
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

        let expected = String::from("A  X  Y  P  S\n34 56 78 FF 0F \n\nregisters\n\nPassed\n");
        let buf = get_status_str(&mut emu, 0x6004, expected.len());

        assert_eq!(0, emu.mem.cpu_read(0x6000));
        assert_eq!(expected, buf);
    }

    #[test]
    fn test_nes_ram_after_reset() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_nes(&String::from(
            test_rom!("cpu_reset/ram_after_reset.nes"),
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
            test_rom!("oam_read/oam_read.nes"),
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
            test_rom!("ppu_vbl_nmi/rom_singles/01-vbl_basics.nes"),
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
            test_rom!("ppu_vbl_nmi/rom_singles/03-vbl_clear_time.nes"),
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
            test_rom!("ppu_vbl_nmi/rom_singles/04-nmi_control.nes"),
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
            test_rom!("ppu_vbl_nmi/rom_singles/09-even_odd_frames.nes"),
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

    fn run_blargg_test(rom_path: &str, test_name: &str) {
        let mut emu = init_blargg_emu(rom_path);
        emu.run();

        let buf = get_status_str(&mut emu, 0x6004, 300);
        let status = emu.mem.cpu_read(0x6000);
        assert_eq!(
            0,
            status,
            "{}: status 0x{:02X}: {}",
            test_name,
            status,
            buf.trim()
        );
    }

    /// Run a test ROM that uses the older blargg framework (no $6000 protocol).
    /// Runs up to `max_frames`, then scans PPU nametable VRAM for "Passed"/"Failed".
    fn run_screen_test(rom_path: &str, test_name: &str, max_frames: u32) {
        let mut emu = init_blargg_emu(rom_path);

        for _ in 0..max_frames {
            if !emu.run_one_frame() {
                break;
            }
            let status = emu.mem.cpu_read(0x6000);
            if status != 0x80 && status != 0x00 {
                break;
            }
        }

        let screen_text = read_nametable_text(&emu);
        assert!(
            screen_text.contains("Passed"),
            "{test_name}: expected 'Passed' on screen but got:\n{screen_text}"
        );
    }

    fn init_blargg_emu(rom_path: &str) -> emu::Emulator {
        let mut emu = emu::Emulator::new_headless(loader::load_nes(&String::from(rom_path)));
        emu.cpu.status = 0x34;
        emu.cpu.sp = 0xfd;
        emu.toggle_should_trigger_nmi(true);
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);
        emu
    }

    fn read_nametable_text(emu: &emu::Emulator) -> String {
        let mut text = String::new();
        for row in 0..30u16 {
            for col in 0..32u16 {
                let tile = emu.mem.ppu_read(0x2000 + row * 32 + col);
                let ch = if tile >= 0x20 && tile < 0x7F {
                    tile as char
                } else {
                    ' '
                };
                text.push(ch);
            }
            text.push('\n');
        }
        text
    }

    // --- cpu_interrupts_v2 ---

    #[test]
    fn test_cpu_interrupts_cli_latency() {
        run_blargg_test(
            test_rom!("cpu_interrupts_v2/rom_singles/1-cli_latency.nes"),
            "1-cli_latency",
        );
    }

    #[test]
    #[ignore]
    fn test_cpu_interrupts_nmi_and_brk() {
        run_blargg_test(
            test_rom!("cpu_interrupts_v2/rom_singles/2-nmi_and_brk.nes"),
            "2-nmi_and_brk",
        );
    }

    #[test]
    #[ignore]
    fn test_cpu_interrupts_nmi_and_irq() {
        run_blargg_test(
            test_rom!("cpu_interrupts_v2/rom_singles/3-nmi_and_irq.nes"),
            "3-nmi_and_irq",
        );
    }

    #[test]
    #[ignore]
    fn test_cpu_interrupts_irq_and_dma() {
        run_blargg_test(
            test_rom!("cpu_interrupts_v2/rom_singles/4-irq_and_dma.nes"),
            "4-irq_and_dma",
        );
    }

    #[test]
    #[ignore]
    fn test_cpu_interrupts_branch_delays_irq() {
        run_blargg_test(
            test_rom!("cpu_interrupts_v2/rom_singles/5-branch_delays_irq.nes"),
            "5-branch_delays_irq",
        );
    }

    // --- branch_timing_tests ---

    #[test]
    fn test_branch_timing_basics() {
        run_blargg_test(
            test_rom!("branch_timing_tests/1.Branch_Basics.nes"),
            "1.Branch_Basics",
        );
    }

    #[test]
    fn test_branch_timing_backward() {
        run_blargg_test(
            test_rom!("branch_timing_tests/2.Backward_Branch.nes"),
            "2.Backward_Branch",
        );
    }

    #[test]
    fn test_branch_timing_forward() {
        run_blargg_test(
            test_rom!("branch_timing_tests/3.Forward_Branch.nes"),
            "3.Forward_Branch",
        );
    }

    // --- cpu_timing_test6 ---

    #[test]
    fn test_cpu_timing() {
        run_blargg_test(
            test_rom!("cpu_timing_test6/cpu_timing_test.nes"),
            "cpu_timing_test",
        );
    }

    // --- instr_misc ---

    #[test]
    fn test_instr_misc_abs_x_wrap() {
        run_blargg_test(
            test_rom!("instr_misc/rom_singles/01-abs_x_wrap.nes"),
            "01-abs_x_wrap",
        );
    }

    #[test]
    fn test_instr_misc_branch_wrap() {
        run_blargg_test(
            test_rom!("instr_misc/rom_singles/02-branch_wrap.nes"),
            "02-branch_wrap",
        );
    }

    #[test]
    fn test_instr_misc_dummy_reads() {
        run_blargg_test(
            test_rom!("instr_misc/rom_singles/03-dummy_reads.nes"),
            "03-dummy_reads",
        );
    }

    #[test]
    fn test_instr_misc_dummy_reads_apu() {
        run_blargg_test(
            test_rom!("instr_misc/rom_singles/04-dummy_reads_apu.nes"),
            "04-dummy_reads_apu",
        );
    }

    // --- instr_test-v5 ---

    #[test]
    fn test_instr_test_v5() {
        run_blargg_test(
            test_rom!("instr_test-v5/official_only.nes"),
            "instr_test-v5",
        );
    }

    // --- instr_timing ---

    #[test]
    fn test_instr_timing_1() {
        run_blargg_test(
            test_rom!("instr_timing/rom_singles/1-instr_timing.nes"),
            "1-instr_timing",
        );
    }

    #[test]
    fn test_instr_timing_2() {
        run_blargg_test(
            test_rom!("instr_timing/rom_singles/2-branch_timing.nes"),
            "2-branch_timing",
        );
    }

    // --- cpu_exec_space ---

    #[test]
    fn test_cpu_exec_space_ppuio() {
        run_blargg_test(
            test_rom!("cpu_exec_space/test_cpu_exec_space_ppuio.nes"),
            "test_cpu_exec_space_ppuio",
        );
    }

    // --- ppu_vbl_nmi (remaining) ---

    #[test]
    fn test_ppu_vbl_set_time() {
        run_blargg_test(
            test_rom!("ppu_vbl_nmi/rom_singles/02-vbl_set_time.nes"),
            "02-vbl_set_time",
        );
    }

    #[test]
    fn test_ppu_nmi_timing() {
        run_blargg_test(
            test_rom!("ppu_vbl_nmi/rom_singles/05-nmi_timing.nes"),
            "05-nmi_timing",
        );
    }

    #[test]
    fn test_ppu_suppression() {
        run_blargg_test(
            test_rom!("ppu_vbl_nmi/rom_singles/06-suppression.nes"),
            "06-suppression",
        );
    }

    #[test]
    #[ignore]
    fn test_ppu_nmi_on_timing() {
        run_blargg_test(
            test_rom!("ppu_vbl_nmi/rom_singles/07-nmi_on_timing.nes"),
            "07-nmi_on_timing",
        );
    }

    #[test]
    fn test_ppu_nmi_off_timing() {
        run_blargg_test(
            test_rom!("ppu_vbl_nmi/rom_singles/08-nmi_off_timing.nes"),
            "08-nmi_off_timing",
        );
    }

    #[test]
    fn test_ppu_even_odd_timing() {
        run_blargg_test(
            test_rom!("ppu_vbl_nmi/rom_singles/10-even_odd_timing.nes"),
            "10-even_odd_timing",
        );
    }

    // --- blargg_ppu_tests_2005 ---

    #[test]
    fn test_ppu_palette_ram() {
        run_blargg_test(
            test_rom!("blargg_ppu_tests_2005.09.15b/palette_ram.nes"),
            "palette_ram",
        );
    }

    #[test]
    fn test_ppu_sprite_ram() {
        run_blargg_test(
            test_rom!("blargg_ppu_tests_2005.09.15b/sprite_ram.nes"),
            "sprite_ram",
        );
    }

    #[test]
    fn test_ppu_vram_access() {
        run_blargg_test(
            test_rom!("blargg_ppu_tests_2005.09.15b/vram_access.nes"),
            "vram_access",
        );
    }

    #[test]
    fn test_ppu_vbl_clear_time() {
        run_blargg_test(
            test_rom!("blargg_ppu_tests_2005.09.15b/vbl_clear_time.nes"),
            "vbl_clear_time",
        );
    }

    #[test]
    fn test_ppu_power_up_palette() {
        run_blargg_test(
            test_rom!("blargg_ppu_tests_2005.09.15b/power_up_palette.nes"),
            "power_up_palette",
        );
    }

    // --- ppu_open_bus ---

    #[test]
    fn test_ppu_open_bus() {
        run_blargg_test(test_rom!("ppu_open_bus/ppu_open_bus.nes"), "ppu_open_bus");
    }

    // --- oam_stress ---

    #[test]
    fn test_ppu_oam_stress() {
        run_blargg_test(test_rom!("oam_stress/oam_stress.nes"), "oam_stress");
    }

    // --- dmc_tests ---

    #[test]
    fn test_dmc_status() {
        run_blargg_test(test_rom!("dmc_tests/status.nes"), "dmc-status");
    }

    #[test]
    fn test_dmc_status_irq() {
        run_blargg_test(test_rom!("dmc_tests/status_irq.nes"), "dmc-status_irq");
    }

    #[test]
    fn test_dmc_buffer_retained() {
        run_blargg_test(
            test_rom!("dmc_tests/buffer_retained.nes"),
            "dmc-buffer_retained",
        );
    }

    #[test]
    fn test_dmc_latency() {
        run_blargg_test(test_rom!("dmc_tests/latency.nes"), "dmc-latency");
    }

    // --- vbl_nmi_timing ---

    #[test]
    fn test_vbl_nmi_timing_frame_basics() {
        run_blargg_test(
            test_rom!("vbl_nmi_timing/1.frame_basics.nes"),
            "1.frame_basics",
        );
    }

    #[test]
    fn test_vbl_nmi_timing_vbl_timing() {
        run_blargg_test(test_rom!("vbl_nmi_timing/2.vbl_timing.nes"), "2.vbl_timing");
    }

    #[test]
    fn test_vbl_nmi_timing_even_odd_frames() {
        run_blargg_test(
            test_rom!("vbl_nmi_timing/3.even_odd_frames.nes"),
            "3.even_odd_frames",
        );
    }

    #[test]
    fn test_vbl_nmi_timing_vbl_clear_timing() {
        run_blargg_test(
            test_rom!("vbl_nmi_timing/4.vbl_clear_timing.nes"),
            "4.vbl_clear_timing",
        );
    }

    #[test]
    fn test_vbl_nmi_timing_nmi_suppression() {
        run_blargg_test(
            test_rom!("vbl_nmi_timing/5.nmi_suppression.nes"),
            "5.nmi_suppression",
        );
    }

    #[test]
    fn test_vbl_nmi_timing_nmi_disable() {
        run_blargg_test(
            test_rom!("vbl_nmi_timing/6.nmi_disable.nes"),
            "6.nmi_disable",
        );
    }

    #[test]
    fn test_vbl_nmi_timing_nmi_timing() {
        run_blargg_test(test_rom!("vbl_nmi_timing/7.nmi_timing.nes"), "7.nmi_timing");
    }

    // --- ppu_read_buffer ---

    #[test]
    #[ignore]
    fn test_ppu_read_buffer() {
        run_screen_test(
            test_rom!("ppu_read_buffer/test_ppu_read_buffer.nes"),
            "test_ppu_read_buffer",
            600,
        );
    }

    // --- sprite_hit_tests_2005 ---

    #[test]
    fn test_sprite_hit_basics() {
        run_blargg_test(
            test_rom!("sprite_hit_tests_2005.10.05/01.basics.nes"),
            "01.basics",
        );
    }

    #[test]
    fn test_sprite_hit_alignment() {
        run_blargg_test(
            test_rom!("sprite_hit_tests_2005.10.05/02.alignment.nes"),
            "02.alignment",
        );
    }

    #[test]
    fn test_sprite_hit_corners() {
        run_blargg_test(
            test_rom!("sprite_hit_tests_2005.10.05/03.corners.nes"),
            "03.corners",
        );
    }

    #[test]
    fn test_sprite_hit_flip() {
        run_blargg_test(
            test_rom!("sprite_hit_tests_2005.10.05/04.flip.nes"),
            "04.flip",
        );
    }

    #[test]
    fn test_sprite_hit_left_clip() {
        run_blargg_test(
            test_rom!("sprite_hit_tests_2005.10.05/05.left_clip.nes"),
            "05.left_clip",
        );
    }

    #[test]
    fn test_sprite_hit_right_edge() {
        run_blargg_test(
            test_rom!("sprite_hit_tests_2005.10.05/06.right_edge.nes"),
            "06.right_edge",
        );
    }

    #[test]
    fn test_sprite_hit_screen_bottom() {
        run_blargg_test(
            test_rom!("sprite_hit_tests_2005.10.05/07.screen_bottom.nes"),
            "07.screen_bottom",
        );
    }

    #[test]
    fn test_sprite_hit_double_height() {
        run_blargg_test(
            test_rom!("sprite_hit_tests_2005.10.05/08.double_height.nes"),
            "08.double_height",
        );
    }

    #[test]
    fn test_sprite_hit_timing_basics() {
        run_blargg_test(
            test_rom!("sprite_hit_tests_2005.10.05/09.timing_basics.nes"),
            "09.timing_basics",
        );
    }

    #[test]
    fn test_sprite_hit_timing_order() {
        run_blargg_test(
            test_rom!("sprite_hit_tests_2005.10.05/10.timing_order.nes"),
            "10.timing_order",
        );
    }

    #[test]
    fn test_sprite_hit_edge_timing() {
        run_blargg_test(
            test_rom!("sprite_hit_tests_2005.10.05/11.edge_timing.nes"),
            "11.edge_timing",
        );
    }

    // --- sprite_overflow_tests ---

    #[test]
    fn test_sprite_overflow_basics() {
        run_blargg_test(test_rom!("sprite_overflow_tests/1.Basics.nes"), "1.Basics");
    }

    #[test]
    fn test_sprite_overflow_details() {
        run_blargg_test(
            test_rom!("sprite_overflow_tests/2.Details.nes"),
            "2.Details",
        );
    }

    #[test]
    fn test_sprite_overflow_timing() {
        run_blargg_test(test_rom!("sprite_overflow_tests/3.Timing.nes"), "3.Timing");
    }

    #[test]
    fn test_sprite_overflow_obscure() {
        run_blargg_test(
            test_rom!("sprite_overflow_tests/4.Obscure.nes"),
            "4.Obscure",
        );
    }

    #[test]
    fn test_sprite_overflow_emulator() {
        run_blargg_test(
            test_rom!("sprite_overflow_tests/5.Emulator.nes"),
            "5.Emulator",
        );
    }

    // --- cpu_dummy_reads ---

    #[test]
    fn test_cpu_dummy_reads() {
        run_screen_test(
            test_rom!("cpu_dummy_reads/cpu_dummy_reads.nes"),
            "cpu_dummy_reads",
            600,
        );
    }

    // --- cpu_dummy_writes ---

    #[test]
    fn test_cpu_dummy_writes_oam() {
        run_blargg_test(
            test_rom!("cpu_dummy_writes/cpu_dummy_writes_oam.nes"),
            "cpu_dummy_writes_oam",
        );
    }

    #[test]
    fn test_cpu_dummy_writes_ppumem() {
        run_blargg_test(
            test_rom!("cpu_dummy_writes/cpu_dummy_writes_ppumem.nes"),
            "cpu_dummy_writes_ppumem",
        );
    }

    // --- dmc_dma_during_read4 ---

    #[test]
    #[ignore]
    fn test_dmc_dma_2007_read() {
        run_screen_test(
            test_rom!("dmc_dma_during_read4/dma_2007_read.nes"),
            "dma_2007_read",
            600,
        );
    }

    #[test]
    #[ignore]
    fn test_dmc_dma_2007_write() {
        run_screen_test(
            test_rom!("dmc_dma_during_read4/dma_2007_write.nes"),
            "dma_2007_write",
            600,
        );
    }

    #[test]
    #[ignore]
    fn test_dmc_dma_4016_read() {
        run_screen_test(
            test_rom!("dmc_dma_during_read4/dma_4016_read.nes"),
            "dma_4016_read",
            600,
        );
    }

    #[test]
    #[ignore]
    fn test_dmc_dma_double_2007_read() {
        run_screen_test(
            test_rom!("dmc_dma_during_read4/double_2007_read.nes"),
            "double_2007_read",
            600,
        );
    }

    #[test]
    #[ignore]
    fn test_dmc_dma_read_write_2007() {
        run_screen_test(
            test_rom!("dmc_dma_during_read4/read_write_2007.nes"),
            "read_write_2007",
            600,
        );
    }

    // --- sprdma_and_dmc_dma (fails — DMA cycle timing off by 1) ---

    #[test]
    #[ignore]
    fn test_sprdma_and_dmc_dma() {
        run_blargg_test(
            test_rom!("sprdma_and_dmc_dma/sprdma_and_dmc_dma.nes"),
            "sprdma_and_dmc_dma",
        );
    }

    #[test]
    #[ignore]
    fn test_sprdma_and_dmc_dma_512() {
        run_blargg_test(
            test_rom!("sprdma_and_dmc_dma/sprdma_and_dmc_dma_512.nes"),
            "sprdma_and_dmc_dma_512",
        );
    }

    // --- savestate tests ---

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
        let mut emu = emu::Emulator::new_headless(loader::load_nes(&String::from(test_rom!(
            "other/nestest.nes"
        ))));
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

        let mut emu2 = emu::Emulator::new_headless(loader::load_nes(&String::from(test_rom!(
            "other/nestest.nes"
        ))));
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
        let mut emu1 = emu::Emulator::new_headless(loader::load_nes(&String::from(test_rom!(
            "other/nestest.nes"
        ))));
        emu1.toggle_quiet_mode(true);

        let saved = emu1.save_state_to_bytes();

        let mut emu2 = emu::Emulator::_new();
        let result = emu2.load_state_from_bytes(&saved);
        assert!(result.is_err());
    }

    // --- AccuracyCoin ---
    //
    // AccuracyCoin is a 141-test NES accuracy suite by 100thCoin.
    // https://github.com/100thCoin/AccuracyCoin
    //
    // Protocol:
    //   - Press Start at page top to run all tests
    //   - $35 = RunningAllTests flag ($01 = running, $00 = done)
    //   - $0400-$0492 = per-test results: $01 = pass, $FF = skip,
    //     odd > 1 = draw (multiple valid behaviors), even = fail (error code)

    use crate::emu::apu;
    use crate::emu::io;
    use crate::emu::io::controller;
    use crate::emu::memory;

    use std::cell::Cell;
    use std::rc::Rc;

    struct ScriptedIOHandler {
        buttons: Rc<Cell<u8>>,
    }

    impl io::IOHandler for ScriptedIOHandler {
        fn init(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn log(&self, _logline: String) {}
        fn poll(
            &mut self,
            mem: &mut dyn memory::MemoryMapper,
            _apu: &mut apu::APU,
        ) -> io::PollResult {
            mem.controllers()[0].load_status(self.buttons.get());
            io::PollResult::default()
        }
        fn render(&mut self, _buf: &crate::emu::gfx::buf::Buffer) {}
        fn exit(&self, _s: String) {}
    }

    #[rustfmt::skip]
    const ACCURACY_COIN_TESTS: [(u16, &str, &str); 146] = [
        // Page 1: CPU Behavior (9)
        (0x0405, "CPU Behavior", "ROMnotWritable"),
        (0x0403, "CPU Behavior", "RAMMirror"),
        (0x044D, "CPU Behavior", "ProgramCounter_Wraparound"),
        (0x0474, "CPU Behavior", "DecimalFlag"),
        (0x0475, "CPU Behavior", "BFlag"),
        (0x0406, "CPU Behavior", "DummyReads"),
        (0x0407, "CPU Behavior", "DummyWrites"),
        (0x0408, "CPU Behavior", "OpenBus"),
        (0x047D, "CPU Behavior", "AllNOPs"),
        // Page 2: Addressing Modes (6)
        (0x046E, "Addressing Modes", "AbsIndex"),
        (0x046F, "Addressing Modes", "ZPgIndex"),
        (0x0470, "Addressing Modes", "Indirect"),
        (0x0471, "Addressing Modes", "IndIndeX"),
        (0x0472, "Addressing Modes", "IndIndeY"),
        (0x0473, "Addressing Modes", "Relative"),
        // Page 3: Unofficial SLO (7)
        (0x0409, "Unofficial: SLO", "SLO_03"),
        (0x040A, "Unofficial: SLO", "SLO_07"),
        (0x040B, "Unofficial: SLO", "SLO_0F"),
        (0x040C, "Unofficial: SLO", "SLO_13"),
        (0x040D, "Unofficial: SLO", "SLO_17"),
        (0x040E, "Unofficial: SLO", "SLO_1B"),
        (0x040F, "Unofficial: SLO", "SLO_1F"),
        // Page 4: Unofficial RLA (7)
        (0x0419, "Unofficial: RLA", "RLA_23"),
        (0x041A, "Unofficial: RLA", "RLA_27"),
        (0x041B, "Unofficial: RLA", "RLA_2F"),
        (0x041C, "Unofficial: RLA", "RLA_33"),
        (0x041D, "Unofficial: RLA", "RLA_37"),
        (0x041E, "Unofficial: RLA", "RLA_3B"),
        (0x041F, "Unofficial: RLA", "RLA_3F"),
        // Page 5: Unofficial SRE (7)
        (0x0420, "Unofficial: SRE", "SRE_43"),
        (0x047F, "Unofficial: SRE", "SRE_47"),
        (0x0422, "Unofficial: SRE", "SRE_4F"),
        (0x0423, "Unofficial: SRE", "SRE_53"),
        (0x0424, "Unofficial: SRE", "SRE_57"),
        (0x0425, "Unofficial: SRE", "SRE_5B"),
        (0x0426, "Unofficial: SRE", "SRE_5F"),
        // Page 6: Unofficial RRA (7)
        (0x0427, "Unofficial: RRA", "RRA_63"),
        (0x0428, "Unofficial: RRA", "RRA_67"),
        (0x0429, "Unofficial: RRA", "RRA_6F"),
        (0x042A, "Unofficial: RRA", "RRA_73"),
        (0x042B, "Unofficial: RRA", "RRA_77"),
        (0x042C, "Unofficial: RRA", "RRA_7B"),
        (0x042D, "Unofficial: RRA", "RRA_7F"),
        // Page 7: Unofficial _AX (10)
        (0x042E, "Unofficial: _AX", "SAX_83"),
        (0x042F, "Unofficial: _AX", "SAX_87"),
        (0x0430, "Unofficial: _AX", "SAX_8F"),
        (0x0431, "Unofficial: _AX", "SAX_97"),
        (0x0432, "Unofficial: _AX", "LAX_A3"),
        (0x0433, "Unofficial: _AX", "LAX_A7"),
        (0x0434, "Unofficial: _AX", "LAX_AF"),
        (0x0435, "Unofficial: _AX", "LAX_B3"),
        (0x0436, "Unofficial: _AX", "LAX_B7"),
        (0x0437, "Unofficial: _AX", "LAX_BF"),
        // Page 8: Unofficial DCP (7)
        (0x0438, "Unofficial: DCP", "DCP_C3"),
        (0x0439, "Unofficial: DCP", "DCP_C7"),
        (0x043A, "Unofficial: DCP", "DCP_CF"),
        (0x043B, "Unofficial: DCP", "DCP_D3"),
        (0x043C, "Unofficial: DCP", "DCP_D7"),
        (0x043D, "Unofficial: DCP", "DCP_DB"),
        (0x043E, "Unofficial: DCP", "DCP_DF"),
        // Page 9: Unofficial ISC (7)
        (0x043F, "Unofficial: ISC", "ISC_E3"),
        (0x0440, "Unofficial: ISC", "ISC_E7"),
        (0x0441, "Unofficial: ISC", "ISC_EF"),
        (0x0442, "Unofficial: ISC", "ISC_F3"),
        (0x0443, "Unofficial: ISC", "ISC_F7"),
        (0x0444, "Unofficial: ISC", "ISC_FB"),
        (0x0445, "Unofficial: ISC", "ISC_FF"),
        // Page 10: Unofficial SH_ (6)
        (0x0446, "Unofficial: SH_", "SHA_93"),
        (0x0447, "Unofficial: SH_", "SHA_9F"),
        (0x0448, "Unofficial: SH_", "SHS_9B"),
        (0x0449, "Unofficial: SH_", "SHY_9C"),
        (0x044A, "Unofficial: SH_", "SHX_9E"),
        (0x044B, "Unofficial: SH_", "LAE_BB"),
        // Page 11: Unofficial Immediates (8)
        (0x0410, "Unofficial: Imm", "ANC_0B"),
        (0x0411, "Unofficial: Imm", "ANC_2B"),
        (0x0412, "Unofficial: Imm", "ASR_4B"),
        (0x0413, "Unofficial: Imm", "ARR_6B"),
        (0x0414, "Unofficial: Imm", "ANE_8B"),
        (0x0415, "Unofficial: Imm", "LXA_AB"),
        (0x0416, "Unofficial: Imm", "AXS_CB"),
        (0x0417, "Unofficial: Imm", "SBC_EB"),
        // Page 12: CPU Interrupts (3)
        (0x0461, "CPU Interrupts", "IFlagLatency"),
        (0x0462, "CPU Interrupts", "NmiAndBrk"),
        (0x0463, "CPU Interrupts", "NmiAndIrq"),
        // Page 13: DMA Tests (10)
        (0x046C, "DMA Tests", "DMA_Plus_OpenBus"),
        (0x0488, "DMA Tests", "DMA_Plus_2002R"),
        (0x044C, "DMA Tests", "DMA_Plus_2007R"),
        (0x044F, "DMA Tests", "DMA_Plus_2007W"),
        (0x045D, "DMA Tests", "DMA_Plus_4015R"),
        (0x045E, "DMA Tests", "DMA_Plus_4016R"),
        (0x046B, "DMA Tests", "DMABusConflict"),
        (0x0477, "DMA Tests", "DMCDMAPlusOAMDMA"),
        (0x0479, "DMA Tests", "ExplicitDMAAbort"),
        (0x0478, "DMA Tests", "ImplicitDMAAbort"),
        // Page 14: APU Timing (9)
        (0x0465, "APU Timing", "APULengthCounter"),
        (0x0466, "APU Timing", "APULengthTable"),
        (0x0467, "APU Timing", "FrameCounterIRQ"),
        (0x0468, "APU Timing", "FrameCounter4Step"),
        (0x0469, "APU Timing", "FrameCounter5Step"),
        (0x046A, "APU Timing", "DeltaModulationChannel"),
        (0x045C, "APU Timing", "APURegActivation"),
        (0x045F, "APU Timing", "ControllerStrobing"),
        (0x047A, "APU Timing", "ControllerClocking"),
        // Page 15: Power-On State (5 DRAW)
        (0x03FF, "Power-On State", "DrawTest_1"),
        (0x03FF, "Power-On State", "DrawTest_2"),
        (0x03FF, "Power-On State", "DrawTest_3"),
        (0x03FF, "Power-On State", "DrawTest_4"),
        (0x03FF, "Power-On State", "DrawTest_5"),
        // Page 16: PPU Behavior (8)
        (0x0485, "PPU Behavior", "CHRROMIsNotWritable"),
        (0x0404, "PPU Behavior", "PPURegMirror"),
        (0x044E, "PPU Behavior", "PPUOpenBus"),
        (0x0476, "PPU Behavior", "PPUReadBuffer"),
        (0x047E, "PPU Behavior", "PaletteRAMQuirks"),
        (0x0486, "PPU Behavior", "RenderingFlagBehavior"),
        (0x048A, "PPU Behavior", "Rendering2007Read"),
        (0x0481, "PPU Behavior", "AttributesAsTiles"),
        // Page 17: PPU Timing (7)
        (0x0450, "PPU Timing", "VBlank_Beginning"),
        (0x0451, "PPU Timing", "VBlank_End"),
        (0x0452, "PPU Timing", "NMI_Control"),
        (0x0453, "PPU Timing", "NMI_Timing"),
        (0x0454, "PPU Timing", "NMI_Suppression"),
        (0x0455, "PPU Timing", "NMI_VBL_End"),
        (0x0456, "PPU Timing", "NMI_Disabled_VBL_Start"),
        // Page 18: Sprite Zero Hits (9)
        (0x0459, "Sprite Zero Hits", "SprOverflow_Behavior"),
        (0x0457, "Sprite Zero Hits", "Sprite0Hit_Behavior"),
        (0x048D, "Sprite Zero Hits", "2002FlagClearTiming"),
        (0x0489, "Sprite Zero Hits", "SuddenlyResizeSprite"),
        (0x0458, "Sprite Zero Hits", "ArbitrarySpriteZero"),
        (0x045A, "Sprite Zero Hits", "MisalignedOAM_Behavior"),
        (0x045B, "Sprite Zero Hits", "Address2004_Behavior"),
        (0x047B, "Sprite Zero Hits", "OAM_Corruption"),
        (0x0480, "Sprite Zero Hits", "INC4014"),
        // Page 19: PPU Misc (9)
        (0x0482, "PPU Misc", "tRegisterQuirks"),
        (0x0483, "PPU Misc", "StaleBGShiftRegisters"),
        (0x048F, "PPU Misc", "StaleSpriteShiftRegs"),
        (0x0487, "PPU Misc", "BGSerialIn"),
        (0x0484, "PPU Misc", "Scanline0Sprites"),
        (0x048C, "PPU Misc", "2004_Stress"),
        (0x048E, "PPU Misc", "2007_Stress"),
        (0x0491, "PPU Misc", "ALERead"),
        (0x0492, "PPU Misc", "HybridAddresses"),
        // Page 20: CPU Behavior 2 (5)
        (0x0460, "CPU Behavior 2", "InstructionTiming"),
        (0x046D, "CPU Behavior 2", "ImpliedDummyRead"),
        (0x048B, "CPU Behavior 2", "BranchDummyRead"),
        (0x047C, "CPU Behavior 2", "JSREdgeCases"),
        (0x0490, "CPU Behavior 2", "InternalDataBus"),
    ];

    fn run_accuracy_coin() -> (u32, u32, u32, u32, Vec<String>) {
        let rom_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../input/nes/AccuracyCoin.nes");

        let buttons = Rc::new(Cell::new(0u8));
        let io_handler = Box::new(ScriptedIOHandler {
            buttons: buttons.clone(),
        });
        let audio = Box::new(crate::emu::audio::SilentAudioOutput::new())
            as Box<dyn crate::emu::audio::AudioBackend>;
        let mapper = loader::load_nes(&String::from(rom_path));
        let mut emu = emu::Emulator::new_with(io_handler, mapper, audio);
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);
        emu.toggle_should_trigger_nmi(true);

        for _ in 0..180 {
            emu.run_one_frame();
        }

        buttons.set(controller::START);
        for _ in 0..5 {
            emu.run_one_frame();
        }
        buttons.set(0);

        let mut was_running = false;
        for _ in 0..(60 * 60 * 30) {
            if !emu.run_one_frame() {
                break;
            }
            let running = emu.mem.cpu_read(0x35);
            if running == 0x01 {
                was_running = true;
            } else if was_running && running == 0x00 {
                for _ in 0..120 {
                    emu.run_one_frame();
                }
                break;
            }
        }
        assert!(
            was_running,
            "AccuracyCoin never started — Start press may not have registered"
        );

        let mut pass = 0u32;
        let mut fail = 0u32;
        let mut skip = 0u32;
        let mut draw = 0u32;
        let mut failures = Vec::new();

        for &(addr, page, name) in &ACCURACY_COIN_TESTS {
            let result = emu.mem.cpu_read(addr);
            match result {
                0xFF => skip += 1,
                0x01 => pass += 1,
                r if r & 0x01 != 0 => draw += 1,
                0x00 => {
                    skip += 1;
                }
                err => {
                    fail += 1;
                    let sub_test = (err >> 2) & 0x3F;
                    failures.push(format!(
                        "  {page} / {name} (${addr:04X}): sub-test {sub_test}",
                    ));
                }
            }
        }

        (pass, fail, draw, skip, failures)
    }

    #[test]
    #[ignore]
    fn test_accuracy_coin() {
        let (pass, fail, draw, skip, failures) = run_accuracy_coin();
        let total = pass + fail + draw;

        println!("\n=== AccuracyCoin Results ===");
        println!(
            "Passed: {pass}/{total} ({:.1}%)",
            pass as f64 / total.max(1) as f64 * 100.0
        );
        println!("Failed: {fail}");
        println!("Draw:   {draw}");
        if skip > 0 {
            println!("Skip:   {skip}");
        }

        if !failures.is_empty() {
            println!("\nFailed tests:");
            for f in &failures {
                println!("{f}");
            }
        }
    }

    #[test]
    #[ignore]
    fn test_accuracy_coin_single() {
        let rom_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../input/nes/AccuracyCoin.nes");
        let buttons = Rc::new(Cell::new(0u8));
        let io_handler = Box::new(ScriptedIOHandler {
            buttons: buttons.clone(),
        });
        let audio = Box::new(crate::emu::audio::SilentAudioOutput::new())
            as Box<dyn crate::emu::audio::AudioBackend>;
        let mapper = loader::load_nes(&String::from(rom_path));
        let mut emu = emu::Emulator::new_with(io_handler, mapper, audio);
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);
        emu.toggle_should_trigger_nmi(true);

        for _ in 0..180 {
            emu.run_one_frame();
        }

        // Press Down to move to first test, then A to run it
        buttons.set(controller::DOWN);
        for _ in 0..5 {
            emu.run_one_frame();
        }
        buttons.set(0);
        for _ in 0..10 {
            emu.run_one_frame();
        }
        buttons.set(controller::A);
        for _ in 0..5 {
            emu.run_one_frame();
        }
        buttons.set(0);

        // Wait for the test to complete
        for _ in 0..300 {
            emu.run_one_frame();
        }

        // Check the result at $0400 and the test's own address $0405
        eprintln!("After single test run:");
        eprintln!("$0400 = {:#04X}", emu.mem.cpu_read(0x0400));
        eprintln!("$0405 = {:#04X}", emu.mem.cpu_read(0x0405));
        for addr in 0x0400u16..=0x040Fu16 {
            eprint!("{:02X} ", emu.mem.cpu_read(addr));
        }
        eprintln!();
    }

    #[test]
    #[ignore]
    fn test_accuracy_coin_dump() {
        let rom_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../input/nes/AccuracyCoin.nes");
        let buttons = Rc::new(Cell::new(0u8));
        let io_handler = Box::new(ScriptedIOHandler {
            buttons: buttons.clone(),
        });
        let audio = Box::new(crate::emu::audio::SilentAudioOutput::new())
            as Box<dyn crate::emu::audio::AudioBackend>;
        let mapper = loader::load_nes(&String::from(rom_path));
        let mut emu = emu::Emulator::new_with(io_handler, mapper, audio);
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);
        emu.toggle_should_trigger_nmi(true);

        for _ in 0..180 {
            emu.run_one_frame();
        }
        buttons.set(controller::START);
        for _ in 0..5 {
            emu.run_one_frame();
        }
        buttons.set(0);

        let mut was_running = false;
        for _ in 0..(60 * 60 * 30) {
            if !emu.run_one_frame() {
                break;
            }
            let running = emu.mem.cpu_read(0x35);
            if running == 0x01 {
                was_running = true;
            } else if was_running && running == 0x00 {
                for _ in 0..120 {
                    emu.run_one_frame();
                }
                break;
            }
        }
        assert!(was_running);

        eprintln!("\nRaw memory $0400-$0492:");
        for row in 0..10 {
            let base = 0x0400 + row * 16;
            let mut line = format!("${:04X}: ", base);
            for col in 0..16u16 {
                let addr = base + col;
                if addr > 0x0492 {
                    break;
                }
                line.push_str(&format!("{:02X} ", emu.mem.cpu_read(addr)));
            }
            eprintln!("{line}");
        }
    }
}
