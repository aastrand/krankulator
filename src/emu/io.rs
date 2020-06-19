use super::memory;
use pancurses;
use std::{thread, time};

pub trait IOHandler {
    fn init(&self);
    fn log(&self, logline: &str);
    fn display(&self, mem: &memory::Memory);
    fn input(&mut self, mem: &mut memory::Memory) -> Option<char>;
    fn exit(&self, s: &str);
}

pub struct HeadlessIOHandler {}

impl IOHandler for HeadlessIOHandler {
    fn init(&self) {}

    fn log(&self, logline: &str) {
        println!("{}", logline);
    }

    #[allow(unused_variables)]
    fn input(&mut self, mem: &mut memory::Memory) -> Option<char> {
        None
    }

    #[allow(unused_variables)]
    fn display(&self, mem: &memory::Memory) {}

    fn exit(&self, s: &str) {
        self.log(s);
    }
}

pub struct CursesIOHandler {
    window: pancurses::Window,
}

impl CursesIOHandler {
    pub fn new() -> CursesIOHandler {
        CursesIOHandler {
            window: pancurses::initscr(),
        }
    }
}

impl IOHandler for CursesIOHandler {
    fn init(&self) {
        self.window.timeout(0);
    }

    fn log(&self, logline: &str) {
        self.window.mvaddstr(0, 0, logline);
    }

    fn input(&mut self, mem: &mut memory::Memory) -> Option<char> {
        // TODO: Map more input
        let c = match self.window.getch() {
            Some(pancurses::Input::Character(c)) => {
                mem.ram[0xff] = c.to_ascii_lowercase() as u8;
                Some(c.to_ascii_lowercase())
            }
            _ => None,
        };
        self.window.refresh();
        c
    }

    fn display(&self, mem: &memory::Memory) {
        // Display is stored as a 32x32 screen from 0x200 and onwards
        let base: u16 = 0x0200;
        let offset: i32 = 3;
        self.window.attron(pancurses::A_REVERSE);

        // Apple location is stored at 0x0100
        let apple_addr: u16 = mem.get_16b_addr(0x00);
        for y in 0..31 {
            for x in 0..31 {
                let addr: u16 = base + (y * 32) + x;
                let chr: char = if addr == apple_addr {
                    'O'
                } else {
                    let value: u8 = mem.ram[addr as usize];
                    if value == 1 {
                        '#'
                    } else {
                        ' '
                    }
                };
                self.window.mvaddch(offset + y as i32, x as i32, chr);
            }
        }
        self.window.attroff(pancurses::A_REVERSE);
        thread::sleep(time::Duration::from_micros(10));
    }

    fn exit(&self, s: &str) {
        self.window.mvaddstr(1, 0, s);
        pancurses::endwin();
    }
}
