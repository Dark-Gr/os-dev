mod fixed_size_heap;

use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use x86_64::structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB};
use x86_64::structures::paging::mapper::MapToError;
use x86_64::{PhysAddr, VirtAddr};
use crate::memory::fixed_size_heap::FixedSizeAllocator;
use crate::utils::Mutex;

/// Address where the mapped heap memory starts
pub const HEAP_START: usize = 0x_4444_4444_0000;

/// The size of the heap in bytes
pub const HEAP_SIZE: usize = 120 * 1024; // 120 KiB

#[global_allocator]
pub static ALLOCATOR: Mutex<FixedSizeAllocator> = Mutex::new(FixedSizeAllocator::new());

/// Maps the heap to [`HEAP_START`] address with the [`HEAP_SIZE`]. This function will also allocate any necessary frames
/// in order for the heap to be valid
pub fn init_heap(mapper: &mut impl Mapper<Size4KiB>, frame_allocator: &mut impl FrameAllocator<Size4KiB>) -> Result<(), MapToError<Size4KiB>> {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE - 1u64;

        let heap_start_page = Page::containing_address(heap_start);
        let head_end_page = Page::containing_address(heap_end);

        Page::range_inclusive(heap_start_page, head_end_page)
    };

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;

        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        unsafe {
            mapper.map_to(page, frame, flags, frame_allocator)?.flush();
        }
    }

    unsafe {
        ALLOCATOR.lock().init(HEAP_START, HEAP_SIZE);
    }

    Ok(())
}

/// Returns an [`OffsetPageTable`] object used to create mappings in memory
///
/// ## Safety
///
/// This function is unsafe because the caller must guarantee the entire physical memory is mapped at the given
/// `physical_memory_offset`. This function should be called only once to avoid `&mut` aliasing references
pub unsafe fn create_memory_mapper(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = get_level_4_active_table(physical_memory_offset);
    return OffsetPageTable::new(level_4_table, physical_memory_offset);
}

/// Returns a mutable reference to the current active level 4 table
///
/// ## Safety
///
/// This function is unsafe because the caller must guarantee the entire physical memory is mapped at the given
/// `physical_memory_offset`. This function should be called only once to avoid `&mut` aliasing references
unsafe fn get_level_4_active_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let physical_ptr = level_4_table_frame.start_address();
    let virtual_ptr = physical_memory_offset + physical_ptr.as_u64();
    let page_table_ptr: *mut PageTable = virtual_ptr.as_mut_ptr();

    return &mut *page_table_ptr;
}

/// General purpose frame allocator used by the kernel to allocate new physical frames when needed
pub struct InternalFrameAllocator {
    memory_map: &'static MemoryMap,
    next: usize
}

impl InternalFrameAllocator {

    /// Creates a frame allocators that uses the given memory map for allocations
    ///
    /// ## Safety
    ///
    /// This method is unsafe because the caller must guarantee that all frames marked as [`MemoryRegionType::Usable`]
    /// are really not being used
    pub unsafe  fn new(memory_map: &'static MemoryMap) -> Self {
        InternalFrameAllocator {
            memory_map,
            next: 0
        }
    }

    /// Returns an [`Iterator`] that only returns physical frames marked as [`MemoryRegionType::Usable`]
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        return  self.memory_map.iter()
            .filter(|r| r.region_type == MemoryRegionType::Usable)
            .map(|r| r.range.start_addr()..r.range.end_addr())
            .flat_map(|r| r.step_by(4096))
            .map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)));
    }
}

unsafe impl FrameAllocator<Size4KiB> for InternalFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        return frame;
    }
}