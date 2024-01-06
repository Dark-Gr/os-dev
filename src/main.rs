#![feature(abi_x86_interrupt)]
#![feature(const_mut_refs)]

#![no_std]
#![no_main]

extern crate alloc;

mod vga;
mod interrupts;
mod memory;
mod utils;

use core::panic::PanicInfo;
use bootloader::{BootInfo, entry_point};
use x86_64::VirtAddr;
use crate::memory::{create_memory_mapper, InternalFrameAllocator};

entry_point!(kernel_main);

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);

    hlt_loop();
}

fn kernel_main(info: &'static BootInfo) -> ! {
    unsafe {
        let mut memory_mapper = create_memory_mapper(VirtAddr::new(info.physical_memory_offset));
        let mut frame_allocator = InternalFrameAllocator::new(&info.memory_map);

        memory::init_heap(&mut memory_mapper, &mut frame_allocator).expect("Failed to initialize the heap");
    }

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