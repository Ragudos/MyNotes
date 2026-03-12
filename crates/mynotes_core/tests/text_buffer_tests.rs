#[cfg(test)]
mod text_buffer_tests {
    use std::fs;
    use std::sync::Arc;
    use std::sync::mpsc::channel;

    use mynotes_core::btree::MeasuredBTree;
    use mynotes_core::line_ending::LineEnding;
    use mynotes_core::line_tracker::LineTracker;
    use mynotes_core::piece_table::{BufferKind, Piece, PieceTable};
    use mynotes_core::text_buffer::TextBuffer;
    use mynotes_io::mmap::MmapFile;
    use mynotes_io::save::{SaveProgress, save_async};

    // Uncomment these based on where your save logic lives!
    // use crate::save_async;
    // use crate::SaveProgress;

    // --- 1. Test the B-Tree / Piece Table Iterator ---
    #[test]
    fn test_btree_iter() {
        let p1 = Piece {
            buffer_kind: BufferKind::Original,
            start: 0,
            end: 5,
        };
        let p2 = Piece {
            buffer_kind: BufferKind::Add,
            start: 0,
            end: 10,
        };

        let mut tree = MeasuredBTree::<Piece>::new();

        tree.insert(0, p1).unwrap();
        tree.insert(5, p2).unwrap();

        let iter_items: Vec<&Piece> = tree.iter().collect();
        assert_eq!(iter_items.len(), 2);
        assert_eq!(iter_items[0].end, 5);
        assert_eq!(iter_items[1].end, 10);
    }

    // --- 2. Test TextBuffer iterators (Borrowed vs Owned) ---
    #[test]
    fn test_text_buffer_iters() {
        // Now we use our clean test initializer!
        let buffer = TextBuffer::empty_for_test();

        // Note: You will need to call whatever your actual PieceTable insert method is here
        // to populate it with dummy data for the test.

        // Test 1: UI Iterator (Borrowed)
        let ui_chunks: Vec<_> = buffer.iter().collect();

        // Test 2: Save Iterator (Owned, 'static)
        let save_chunks: Vec<_> = buffer.into_save_iter().collect();

        // Ensure both iterators yield the exact same number of chunks
        assert_eq!(ui_chunks.len(), save_chunks.len());
    }

    // --- 3. Test the full Background Save Pipeline ---
    #[test]
    fn test_save_async_pipeline() {
        let target_path = std::env::temp_dir().join("test_save_output.txt");
        if target_path.exists() {
            fs::remove_file(&target_path).unwrap();
        }

        let chunk1: Vec<u8> = b"Hello ".to_vec();
        let chunk2: Vec<u8> = b"World!".to_vec();
        let save_iter = vec![chunk1, chunk2].into_iter();

        let (tx, rx) = channel();

        // Trigger the background save
        save_async(target_path.clone(), save_iter, tx);

        let mut finished = false;

        // Use a loop to catch `Err` (channel disconnected prematurely)
        loop {
            match rx.recv() {
                Ok(SaveProgress::Finished { path }) => {
                    assert_eq!(path, target_path);
                    finished = true;
                    break;
                }
                Ok(SaveProgress::Error(e)) => panic!("Save failed: {:?}", e),
                Ok(_) => {} // Ignore intermediate progress updates
                Err(e) => panic!("Channel disconnected before Finished was sent: {}", e),
            }
        }

        assert!(finished, "Save thread did not finish");

        let file_contents = fs::read_to_string(&target_path).unwrap();
        assert_eq!(file_contents, "Hello World!");

        fs::remove_file(target_path).unwrap();
    }
}
