// Adjust this import to match your crate!
use mynotes_core::piece_table::{BufferKind, PieceTable};

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

fn main() {
    // Start tracking every byte allocated on the heap
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    #[cfg(feature = "dhat-ad-hoc")]
    let _profiler = dhat::Profiler::new_ad_hoc();

    let mut table = PieceTable::new();

    // 100,000 random edits to simulate heavy fragmentation
    for (current_length, i) in (0u64..100_000u64).enumerate() {
        #[cfg(feature = "dhat-ad-hoc")]
        dhat::ad_hoc_event(1);

        let insert_pos = if current_length == 0 {
            0
        } else {
            (current_length / 2) as u64
        };
        table.insert(insert_pos, i, 1, BufferKind::Add).unwrap();
    }

    println!("Simulated 100,000 highly fragmented edits.");
}
