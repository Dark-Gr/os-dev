#![feature(abi_x86_interrupt)]
#![no_std]
#![no_main]

mod vga;
mod interrupts;

use core::panic::PanicInfo;
use bootloader::{BootInfo, entry_point};

entry_point!(kernel_main);

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);

    hlt_loop();
}

fn kernel_main(_info: &BootInfo) -> ! {
    interrupts::interrupt_manager::init();

    println!("Hello, World!");
    println!("Approximation of PI: {}", 62832.0 / 20000.0);

    hlt_loop();
}

fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}