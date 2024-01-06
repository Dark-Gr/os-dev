use x86_64::instructions::port::Port;

const PIC_1_COMMAND: u8 = 0x20;
const PIC_2_COMMAND: u8 = 0xA0;
const PIC_1_DATA: u8 = PIC_1_COMMAND + 1;
const PIC_2_DATA: u8 = PIC_2_COMMAND + 1;

const END_OF_INTERRUPT_COMMAND: u8 = 0x20;
const INITIALIZE_COMMAND: u8 = 0x11;
const READ_ISR_COMMAND: u8 = 0x0b;

struct PIC {
    command_port: Port<u8>,
    data_port: Port<u8>,
    masks: Option<u8>
}

impl PIC {
    pub fn new(command_port: u8, data_port: u8) -> Self {
        PIC {
            command_port: Port::new(command_port as u16),
            data_port: Port::new(data_port as u16),
            masks: None
        }
    }

    /// Writes the given command to this PIC
    ///
    /// ## Safety
    ///
    /// This function is unsafe because it can violate memory safety
    pub unsafe fn write_command(&mut self, command: u8) {
        self.command_port.write(command);
    }

    /// Writes the given data to this PIC
    ///
    /// ## Safety
    ///
    /// This function is unsafe because it can violate memory safety
    pub unsafe fn write_data(&mut self, data: u8) {
        self.data_port.write(data);
    }

    /// Reads the current masks for this PIC and saves it
    ///
    /// ## Safety
    ///
    /// This function is unsafe because the caller must guarantee the data port is valid
    pub unsafe fn save_masks(&mut self) {
        self.masks = Some(self.data_port.read());
    }

    /// Writes the previously saved masks to the current PIC
    ///
    /// ## Safety
    ///
    /// This function is unsafe because the caller must guarantee the data port is valid
    pub unsafe fn restore_masks(&mut self) {
        if let Some(masks) = self.masks {
            self.data_port.write(masks);
        }
    }
}

pub struct PICPair {
    master_pic: PIC,
    slave_pic: PIC
}

impl PICPair {
    pub fn new() -> Self {
        PICPair {
            master_pic: PIC::new(PIC_1_COMMAND, PIC_1_DATA),
            slave_pic: PIC::new(PIC_2_COMMAND, PIC_2_DATA)
        }
    }

    /// Properly initializes both PIC's with the given offsets
    pub fn initialize(&mut self, pic_1_offset: u8, pic_2_offset: u8) {
        let mut garbage_port = Port::new(0x80);
        let mut wait = || unsafe { garbage_port.write(0u8); };

        unsafe {
            self.master_pic.save_masks();
            self.slave_pic.save_masks();

            self.master_pic.write_command(INITIALIZE_COMMAND);
            wait();
            self.slave_pic.write_command(INITIALIZE_COMMAND);
            wait();

            // Tells both PIC's what offset each one should use
            self.master_pic.write_data(pic_1_offset);
            wait();
            self.slave_pic.write_data(pic_2_offset);
            wait();

            self.master_pic.write_data(4); // Tell the master PIC that there's a slave PIC at the port IRQ 2
            wait();
            self.slave_pic.write_data(2); // Tell the slave PIC it's cascade identity (Which IRQ the slave PIC is connected on the master PIC)
            wait();

            // Tell both PIC's to use 8086 mode, as opposed to the default 8080 mode
            self.master_pic.write_data(0x01);
            wait();
            self.slave_pic.write_data(0x01);
            wait();

            self.master_pic.restore_masks();
            self.slave_pic.restore_masks();
        }
    }

    /// Sets an IRQ mask, whatever enabling or disabling a mask
    #[allow(dead_code)]
    pub fn set_mask(&mut self, irq: u8, enable: bool) {
        let pic =  if irq < 8 { &mut self.master_pic } else { &mut self.slave_pic }; // Decide which PIC to operate on
        let line = if irq < 8 { irq } else { irq - 8 }; // Determine the local IRQ for the PIC

        unsafe { // Reads the current masks, sets the new mask and writes the new mask set to the PIC
            let current_masks = pic.data_port.read();
            let mask = 1 << line;

            let value = if enable { current_masks | mask } else { current_masks & !mask };
            pic.data_port.write(value);
        }
    }

    /// Reads the ISR (In Service Register) of both PIC's and returns it in a single u16 value,
    /// being the first 8 bits on the left the slave PIC ISR bits and the other 8 on the right the master ISR bits
    ///
    /// The ISR represents the interrupts currently being serviced, so checking if any of PICs are actually servicing
    /// any interrupts help determinate whatever the interrupt raised on the CPU is a real one or a spurious one (A fake one caused by various factors)
    fn read_isr(&mut self) -> u16 {
        unsafe {
            self.master_pic.write_command(READ_ISR_COMMAND);
            self.slave_pic.write_command(READ_ISR_COMMAND);

            let pic_1_irr = self.master_pic.command_port.read();
            let pic_2_irr = self.slave_pic.command_port.read();

            return ((pic_2_irr as u16) << 8) | pic_1_irr as u16;
        }
    }

    /// Checks whatever a given IRQ actually raised an interrupt or if a spurious IRQ was raised.
    ///
    /// An interrupt raised on the CPU that isn't registered in any of the PIC's ISR (In Service Register) is called a spurious IRQ,
    /// When the kernel receives one of these interrupts it should ignore the interrupt and NOT send an EOI (End Of Interrupt) signal to any PICs
    pub fn check_for_spurious(&mut self, irq: u8) -> bool {
        if irq >= 16 {
            return false;
        }

        let pics_irr = self.read_isr();
        let expected = (1u16) << irq;

        return pics_irr & expected != expected;
    }

    /// Sends an EOI (End Of Interrupt) signal only for the master PIC,
    /// this method should be used when a spurious IRQ comes from the slave PIC, since the master doesn't know the interrupt is spurious
    /// and will wait until the kernel acknowledge the interrupt
    pub fn end_of_interrupt_master_only(&mut self) {
        unsafe {
            self.master_pic.write_command(END_OF_INTERRUPT_COMMAND);
        }
    }

    /// Notifies the correct PIC about the end of the current interrupt handling and letting the PIC's know the kernel is ready for the next interrupt
    pub fn end_of_interrupt(&mut self, irq: u8) {
        unsafe {
            if irq >= 8 {
                self.slave_pic.write_command(END_OF_INTERRUPT_COMMAND);
            }

            self.end_of_interrupt_master_only();
        }
    }
}