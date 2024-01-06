use core::{fmt, ptr};
use core::fmt::Write;
use lazy_static::lazy_static;

const VGA_BUFFER_PTR: usize = 0xb8000;

const BUFFER_WIDTH: usize = 80;
const BUFFER_HEIGHT: usize = 25;

lazy_static! {
    static ref WRITER: spin::Mutex<VGAWriter> = spin::Mutex::new(VGAWriter::new(ColorCode::new(Color::White, Color::Black)));
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        WRITER.lock().write_fmt(args).unwrap();
    });
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct ColorCode(u8);

impl ColorCode {
    pub fn new(foregound: Color, background: Color) -> Self {
        ColorCode((background as u8) << 4 | (foregound as u8))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
struct VGAChar {
    character: u8,
    color: ColorCode
}

struct VGABuffer {
    chars: [ [ VGAChar; BUFFER_WIDTH ]; BUFFER_HEIGHT ]
}

struct VGAWriter {
    cursor_x: usize,
    buffer: &'static mut VGABuffer,
    default_color: ColorCode
}

impl VGAWriter {
    pub fn new(default_color: ColorCode) -> Self {
        VGAWriter {
            cursor_x: 0,
            buffer: unsafe { &mut *(VGA_BUFFER_PTR as *mut VGABuffer) },
            default_color
        }
    }

    pub fn write_char(&mut self, c: char, color: ColorCode) {
        match c {
            '\n' => self.new_line(),
            character => {
                if self.cursor_x >= BUFFER_WIDTH {
                    self.new_line();
                }

                let col = self.cursor_x;
                let row = BUFFER_HEIGHT - 1;

                self.buffer.chars[row][col] = VGAChar {
                    character: character as u8,
                    color
                };

                self.cursor_x += 1;
            }
        }
    }

    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_char(byte as char, self.default_color),
                _ => self.write_char(0xfe as char, self.default_color)
            }
        }
    }

    fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT { // No idea why but if this doesn't start at 1 then new lines doesn't work
            for col in 0..BUFFER_WIDTH {
                let c = self.buffer.chars[row][col];
                self.buffer.chars[row - 1][col] = c;
            }
        }

        self.clear_row(BUFFER_HEIGHT - 1);
        self.cursor_x = 0;
    }

    fn clear_row(&mut self, row: usize) {
        let vga_ptr = VGA_BUFFER_PTR as *mut u8;

        // Use BUFFER_WIDTH * 2 because each character is two bytes (one for the char and one for the color)
        let row_length = BUFFER_WIDTH * 2;

        unsafe {
            let row_ptr = vga_ptr.offset((row * row_length) as isize);
            ptr::write_bytes(row_ptr, 0, row_length);
        }
    }
}

impl Write for VGAWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}