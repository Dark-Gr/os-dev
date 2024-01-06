use lazy_static::lazy_static;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;
use crate::{print, println};
use crate::interrupts::pic::PICPair;

const DOUBLE_FAULT_IST_INDEX: u16 = 0;

const PIC_1_OFFSET: u8 = 32;
const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        idt.breakpoint.set_handler_fn(breakpoint_handler);

        unsafe {
            idt.double_fault.set_handler_fn(double_fault_handler).set_stack_index(DOUBLE_FAULT_IST_INDEX);
        }

        idt[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_handler);
        idt[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_handler);

        idt
    };
}

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();

        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 5;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = VirtAddr::from_ptr(unsafe { &STACK });
            let stack_end = stack_start + STACK_SIZE;

            stack_end
        };

        tss
    };
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();
        let code_selector = gdt.add_entry(Descriptor::kernel_code_segment());
        let tss_selector = gdt.add_entry(Descriptor::tss_segment(&TSS));

        (gdt, Selectors { code_selector, tss_selector })
    };
}

lazy_static! {
    static ref PICS: spin::Mutex<PICPair> = spin::Mutex::new(PICPair::new());
}

/// Loads a backup stack used in case of a stackoverflow exception is raised,
/// configures the PIC8259 to correctly forward hardware interrupts to the CPU
/// and configures the CPU to call the correct handles in case of an interrupt of exception
pub fn init() {
    use x86_64::instructions::tables::load_tss;
    use x86_64::instructions::segmentation::{CS, Segment};

    GDT.0.load();

    unsafe {
        CS::set_reg(GDT.1.code_selector);
        load_tss(GDT.1.tss_selector);
    }

    IDT.load();

    PICS.lock().initialize(PIC_1_OFFSET, PIC_2_OFFSET);
    x86_64::instructions::interrupts::enable()
}


/// Contains the IRQ indexes for the PIC8259, these IRQs are used to sent interrupts
/// to the CPU than can be sent from external hardware such as the keyboard
#[derive(Debug, Copy, Clone)]
#[repr(u8)]
enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard = PIC_1_OFFSET + 1
}

impl InterruptIndex {
    pub fn as_u8(self) -> u8 {
        return self as u8;
    }

    pub fn as_usize(self) -> usize {
        return usize::from(self.as_u8());
    }

    pub fn get_irq_line(self) -> u8 {
        return self.as_u8() - PIC_1_OFFSET;
    }
}

struct Selectors {
    code_selector: SegmentSelector,
    tss_selector: SegmentSelector
}

////////////////////////////////////////////////////////////////////////////
////////////////////////////// CPU EXCEPTIONS //////////////////////////////
////////////////////////////////////////////////////////////////////////////

/// Handler for the breakpoint exception
///
/// ## Cause
///
/// This handler is called by the CPU when it reaches a breakpoint (the `int3` instruction).
/// This handler probably won't be called unless the kernel explicitly uses this instruction
extern "x86-interrupt" fn breakpoint_handler(interrupt_stack_frame: InterruptStackFrame) {
    println!("\n\nEXCEPTION: [BREAKPOINT] \n{:#?}\n\n", interrupt_stack_frame);
}

/// Handler for the double fault exception
///
/// ## Cause
///
/// This handler is called by the CPU when an exception happens and no handlers are registered for that exception,
/// causing the CPU to fail trying to handle this error and thus rising the double fault exception
extern "x86-interrupt" fn double_fault_handler(interrupt_stack_frame: InterruptStackFrame, _error_code: u64) -> ! {
    panic!("\n\nEXCEPTION: [DOUBLE_FAULT] \n{:#?}\n\n", interrupt_stack_frame);
}

///////////////////////////////////////////////////////////////////////////////
////////////////////////////// EXTERNAL HARDWARE //////////////////////////////
///////////////////////////////////////////////////////////////////////////////

/// Handler for the timer interrupt
///
/// ## Cause
///
/// This handler is called ten times a second (every 100ms)
extern "x86-interrupt" fn timer_handler(_interrupt_stack_frame: InterruptStackFrame) {
    let mut pics = PICS.lock();
    if pics.check_for_spurious(InterruptIndex::Timer.get_irq_line()) {
        return;
    }

    pics.end_of_interrupt(InterruptIndex::Timer.as_u8());
}

/// Handler for the keyboard interrupt
///
/// ## Cause
///
/// This handler is called every time a key on the user keyboard is pressed or released
extern "x86-interrupt" fn keyboard_handler(_interrupt_stack_frame: InterruptStackFrame) {
    let mut pics = PICS.lock();

    // If the interrupt is proven to be a spurious IRQ then we just ignore it and don't send an EOI signal
    if pics.check_for_spurious(InterruptIndex::Keyboard.get_irq_line()) {
        return;
    }

    use x86_64::instructions::port::Port;

    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };

    print!("{}", scancode);

    pics.end_of_interrupt(InterruptIndex::Keyboard.as_u8());
}