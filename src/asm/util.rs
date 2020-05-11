use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;

extern crate hex;

pub fn read_code_ascii(path: &str) -> Vec<u8> {
    if !Path::new(path).exists() {
        panic!("File does not exist: {}", path);
    }

    let mut code: Vec<u8> = vec![];

    if let Ok(lines) = read_lines(path) {
        for line in lines {
            if let Ok(content) = line {
                let content = content.trim();
                let content = content.split(';').nth(0).unwrap();
                if content.len() > 0 {
                    for byte in content.split(' ') {
                        let mut decoded = hex::decode(byte).expect("Decoding failed");
                        code.append(&mut decoded);
                    }
                }
            }
        }
    }

    code
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}
