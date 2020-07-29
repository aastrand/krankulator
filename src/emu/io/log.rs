use super::memory;

use std::collections::VecDeque;

pub struct LogFormatter {
    pub buffer_capacity: usize,
    log_lines: VecDeque<String>,
}

impl LogFormatter {
    pub fn new(buffer_capacity: usize) -> LogFormatter {
        let log_lines = VecDeque::with_capacity(buffer_capacity);

        LogFormatter {
            buffer_capacity,
            log_lines,
        }
    }

    pub fn log_stack(&self, mem: &mut Box<dyn memory::MemoryMapper>, stack_ptr: u8) -> String {
        let mut addr: u16 = 0x1ff;
        let mut buf = String::new();
        buf.push_str(&format!("stack contents:"));
        let mut cols = 0;

        loop {
            if addr == mem.stack_addr(stack_ptr) {
                break;
            }
            buf.push_str(&format!(
                "0x{:x} = 0x{:x} \t",
                addr,
                mem.cpu_read(addr)
            ));
            cols += 1;
            addr = addr.wrapping_sub(1);

            if cols > 8 {
                buf.push('\n');
                cols = 0;
            }
        }

        buf
    }

    pub fn log_str(
        &self,
        opcode: [u8; 3],
        opcode_name: &str,
        size: u16,
        pc: u16,
        registers: String,
        cycles: u64,
        status: String,
        ppu_scanline: u16,
        ppu_cycle: u16,
        logdata: &Vec<u16>
    ) -> String {
        let mut logline = String::with_capacity(80);

        logline.push_str(&format!("{:04X} ", pc));
        logline.push_str(&(1..(7 - logline.len())).map(|_| " ").collect::<String>());
        logline.push_str(&format!("{:02X}", opcode[0]));

        if size > 1 {
            logline.push_str(&format!(" {:02X}", opcode[1]));
            if size > 2 {
                logline.push_str(&format!(" {:02X}", opcode[2]));
            }
        }
        logline.push_str(&(1..(16 - logline.len())).map(|_| " ").collect::<String>());
        logline.push_str(&format!(" {}", opcode_name));

        if logdata.len() > 0 {
            logline.push_str(&format!(" {:X}", logdata[0]));
            if logdata.len() > 1 {
                logline.push_str(&format!(" = {:X}", logdata[1]));
            }
        }

        logline.push_str(&(1..(49 - logline.len())).map(|_| " ").collect::<String>());
        logline.push_str(&registers);

        logline.push_str(&format!(" PPU: {:>2},{:>3}", ppu_scanline, ppu_cycle));
        logline.push_str(&format!(" CYC:{}", cycles));

        logline.push_str(&(1..(110 - logline.len())).map(|_| " ").collect::<String>());
        logline.push_str(&status);

        logline
    }

    pub fn log(
        &mut self,
        logline: String,
    ) -> String {
        self.log_lines.push_back(logline);
        if self.log_lines.len() > self.buffer_capacity {
            self.log_lines.pop_front();
        }

        self.log_lines.back().unwrap().to_string()
    }

    pub fn replay(&self) -> String {
        let mut buf: String = String::new();
        for line in self.log_lines.iter() {
            buf.push_str(&line);
            buf.push('\n');
        }

        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_str_size_3() {
        let sut = LogFormatter::new(10);
        let s = sut.log_str([0x4c, 0x11, 0x47], &format!("JMP"), 3, 0x400, format!("regs"), 0, format!("status"), 0, 0, &vec![]);

        assert_eq!(s, "0400  4C 11 47  JMP                             regs PPU:  0,  0 CYC:0                                       status");
    }

    #[test]
    fn test_log_str_size_2() {
        let sut = LogFormatter::new(10);
        let s = sut.log_str([0x29, 0x42, 0x0], &format!("AND"), 2, 0xc000, format!("regs"), 0, format!("status"), 0, 0, &vec![]);

        assert_eq!(s, "C000  29 42     AND                             regs PPU:  0,  0 CYC:0                                       status");
    }

    #[test]
    fn test_log_str_size_1() {
        let sut = LogFormatter::new(10);
        let s = sut.log_str([0xea, 0x0, 0x0], &format!("NOP"), 1, 0xfffe, format!("regs"), 0, format!("status"), 0, 0, &vec![]);

        assert_eq!(s, "FFFE  EA        NOP                             regs PPU:  0,  0 CYC:0                                       status");
    }

    #[test]
    fn test_log_str_cyc() {
        let sut = LogFormatter::new(10);
        let s = sut.log_str([0xea, 0x0, 0x0], &format!("NOP"), 1, 0xfffe, format!("regs"), 43432423, format!("status"), 0, 0, &vec![]);

        assert_eq!(s, "FFFE  EA        NOP                             regs PPU:  0,  0 CYC:43432423                                status");
    }

    #[test]
    fn test_log_stack() {
        let sut = LogFormatter::new(10);
        let mut mem: Box<dyn memory::MemoryMapper> = Box::new(memory::IdentityMapper::new(0));
        let s = sut.log_stack(&mut mem, 0xfa);

        assert_eq!(s, "stack contents:0x1ff = 0x0 \t0x1fe = 0x0 \t0x1fd = 0x0 \t0x1fc = 0x0 \t0x1fb = 0x0 \t");
    }

    #[test]
    fn test_replay() {
        let mut sut = LogFormatter::new(10);
        sut.log(sut.log_str([0x4c, 0x11, 0x47], &format!("JMP"), 3, 0x400, format!("regs"), 0, format!("status"), 0, 0, &vec![]));
        sut.log(sut.log_str([0x4c, 0x11, 0x47], &format!("JMP"), 3, 0x1337, format!("regs2"), 0, format!("status2"), 0, 0, &vec![]));

        let s = sut.replay();

        assert_eq!(s, "0400  4C 11 47  JMP                             regs PPU:  0,  0 CYC:0                                       status\n1337  4C 11 47  JMP                             regs2 PPU:  0,  0 CYC:0                                      status2\n");
    }

    #[test]
    fn test_replay_capacity() {
        let mut sut = LogFormatter::new(2);
        sut.log(sut.log_str([0x4c, 0x11, 0x47], &format!("JMP"), 3, 0x4211, format!("regs"), 0, format!("status"), 0, 0, &vec![]));
        sut.log(sut.log_str([0x4c, 0x11, 0x47], &format!("JMP"), 3, 0x1337, format!("regs2"), 0, format!("status2"), 0, 0, &vec![]));
        sut.log(sut.log_str([0x4c, 0x11, 0x47], &format!("JMP"), 3, 0x42, format!("regs3"), 0, format!("status3"), 0, 0, &vec![]));

        let s = sut.replay();

        assert_eq!(s, "1337  4C 11 47  JMP                             regs2 PPU:  0,  0 CYC:0                                      status2\n0042  4C 11 47  JMP                             regs3 PPU:  0,  0 CYC:0                                      status3\n");
    }
}