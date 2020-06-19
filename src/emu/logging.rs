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

    pub fn log_stack(&self, mem: &memory::Memory, stack_ptr: u8) -> String {
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
                mem.value_at_addr(addr)
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

    pub fn log_monitor(
        &self,
        opcode: u8,
        opcode_name: &str,
        pc: u16,
        registers: String,
        status: String,
    ) -> String {
        let mut logline = String::with_capacity(80);

        logline.push_str(&format!("0x{:x}: {} (0x{:x})", pc, opcode_name, opcode));

        logline.push_str(&(1..(50 - logline.len())).map(|_| " ").collect::<String>());
        logline.push_str(&registers);

        logline.push_str(&(1..(85 - logline.len())).map(|_| " ").collect::<String>());
        logline.push_str(&status);

        logline
    }

    fn log_str(
        &self,
        opcode: u8,
        opcode_name: &str,
        registers: String,
        status: String,
        logdata: Vec<u16>,
    ) -> String {
        let mut logline = String::with_capacity(80);

        logline.push_str(&format!(
            "0x{:x}: {} (0x{:x})",
            logdata[0], opcode_name, opcode
        ));

        if logdata.len() > 1 {
            logline.push_str(&format!(" arg=0x{:x}", logdata[1]));
            if logdata.len() > 2 {
                logline.push_str(&format!("=>0x{:x}", logdata[2]));
            }
        }

        logline.push_str(&(1..(50 - logline.len())).map(|_| " ").collect::<String>());
        logline.push_str(&registers);

        logline.push_str(&(1..(85 - logline.len())).map(|_| " ").collect::<String>());
        logline.push_str(&status);

        logline
    }

    pub fn log_instruction(
        &mut self,
        opcode: u8,
        opcode_name: &str,
        registers: String,
        status: String,
        logdata: Vec<u16>,
    ) -> String {
        self.log_lines.push_back(self.log_str(opcode, opcode_name, registers, status, logdata));
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
