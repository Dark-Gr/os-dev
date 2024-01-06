use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use crate::println;
use crate::utils::Mutex;

/// These are all the different block sizes this allocator will create when initialized.
/// The distribution of said blocks is based on [`BLOCK_DISTRIBUTIONS`]
///
/// ## Note
///
/// The smallest block must always be 8 bytes to make sure all blocks can hold a [`MemoryNode`] when free
const BLOCK_SIZES: &[usize] = &[ 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096 ];

/// These are the distributions of the [`BLOCK_SIZES`]
///
/// ## Note
///
/// This distribution is not checked so make sure it always sums up to 100% or else memory can be
/// waste or in the worst case collisions can happen
const BLOCK_DISTRIBUTIONS: &[f32] = &[10.0 / 100.0; 10]; // Each block size has 10% of the total memory

/// The fixed size allocator rely on a linked list to know the addresses of all the free (unused)
/// memory blocks.
///
/// This `struct` represents a single node in this list and holds a pointer to the next node in the list
#[derive(Debug)]
struct MemoryNode {
    next: Option<&'static mut MemoryNode>
}

/// A heap allocator that works by dividing the given memory into blocks of different sizes
/// and returning the smallest possible block for an allocation.
///
/// This is a fast method of allocation but can also cause waste of memory because, for example, an
/// allocation that requires only one byte would cause the allocator to dedicate an 8 byte block for
/// this, even though the request only asks for one byte.
///
/// For more information refer to [this post](https://os.phil-opp.com/allocator-designs/#fixed-size-block-allocator)
pub struct FixedSizeAllocator {
    heads: [ Option<&'static mut MemoryNode>; BLOCK_SIZES.len() ]
}

impl FixedSizeAllocator {
    pub const fn new() -> Self {
        const EMPTY: Option<&'static mut MemoryNode> = None;

        FixedSizeAllocator {
            heads: [ EMPTY; BLOCK_SIZES.len() ]
        }
    }

    /// Initializes all the memory blocks, following the [`BLOCK_SIZES`] and [`BLOCK_DISTRIBUTIONS`]
    /// to determine which blocks sizes should be created and how much memory will be dedicated
    /// to each block size
    ///
    /// ## Safety
    ///
    /// This method is unsafe because the caller must guarantee that `heap_address` and `heap_size`
    /// point to a mapped region in memory and that `memory_size` perfectly fits the distributions
    /// of the blocks
    pub unsafe fn init(&mut self, heap_address: usize, heap_size: usize) {
        let mut current_memory_offset = heap_address;

        for (index, &block_size) in BLOCK_SIZES.iter().enumerate() {
            let memory_share = (BLOCK_DISTRIBUTIONS[index] * heap_size as f32) as usize;
            let block_count = memory_share / block_size;

            unsafe {
                self.create_blocks(block_size, block_count, current_memory_offset)
            }

            current_memory_offset += memory_share;
        }
    }

    /// Creates `count` blocks os `block_size` bytes and builds a linked list between then, where the
    /// first block is placed in `start_address`, with the subsequent blocks right after
    ///
    /// ## Safety
    ///
    /// This method is unsafe because the caller must guarantee that `start_address` doesn't
    /// collide with any other allocated memory and the memory necessary for this operation is
    /// available
    unsafe fn create_blocks(&mut self, block_size: usize, count: usize, start_address: usize) {
        let mut previous_block: &mut Option<&mut MemoryNode> = &mut None;

        for i in 0..count {
            // Calculate the next address for the this node based on how much blocks were already created
            let addr = start_address + i * block_size;
            let node = MemoryNode { next: None };

            // Get a pointer to the address and write the node there
            let node_ptr = addr as *mut MemoryNode;
            node_ptr.write(node);

            // If no head exists yet then set this node as the head and continue to the next iteration
            if previous_block.is_none() {
                let head_index = BLOCK_SIZES.iter().position(|&size| size.eq(&block_size)).unwrap();
                self.heads[head_index] = Some(&mut *node_ptr);
                previous_block = &mut self.heads[head_index];

                continue;
            }

            let previous_node = previous_block.as_mut().unwrap();

            // Link this node with the ancestor and prepare for next iteration
            previous_node.next = Some(&mut *node_ptr);
            previous_block = &mut previous_node.next;
        }
    }

    /// Finds out which block size is better for an allocation that follow the given `layout`.
    /// This method returns [`None`] if no existing block size satisfies the given `layout`
    pub fn block_size_for(layout: &Layout) -> Option<usize> {
        let required_block_size = layout.size().max(layout.align());
        return BLOCK_SIZES.iter().position(|&s| s >= required_block_size);
    }
}

unsafe impl GlobalAlloc for Mutex<FixedSizeAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = self.lock();

        match FixedSizeAllocator::block_size_for(&layout) {
            Some(index) => {
                let head = &mut allocator.heads[index];
                let head = head.take();

                if let Some(node) = head {
                    let next_node = node.next.take();
                    allocator.heads[index] = next_node;

                    return node as *mut MemoryNode as *mut u8;
                } else {
                    println!("No block available for size {}", BLOCK_SIZES[index]);
                }
            },
            None => {
                println!("Failed to allocate heap for layout {:?}: No memory block available", layout);
            }
        }

        return ptr::null_mut();
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut allocator = self.lock();

        match FixedSizeAllocator::block_size_for(&layout) {
            Some(index) => {
                let head = &mut allocator.heads[index];
                let head = head.take();

                let new_node = MemoryNode {
                    next: head
                };

                let new_node_ptr = ptr as *mut MemoryNode;
                new_node_ptr.write(new_node);

                allocator.heads[index] = Some(&mut *new_node_ptr);
            },
            None => {
                println!("Attempt to deallocate a block that doesn't exist, this shouldn't be possible")
            }
        }
    }
}