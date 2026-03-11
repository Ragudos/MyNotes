#[cfg(test)]
mod tests {
    use mynotes_core::btree::{DATA_CAPACITY, MeasuredBTreeData, MeasuredBTreeNode};
    use mynotes_core::line_tracker::{LineChunk, LineTracker, LineTrackerSummary, MAX_CHUNK_LINES};
    use rand::RngExt;

    // Helper to create a standard chunk
    fn create_chunk(text: &str) -> LineChunk {
        LineChunk {
            byte_length: text.len() as u32,
            newlines: text
                .bytes()
                .enumerate()
                .filter(|&(_, b)| b == b'\n')
                .map(|(i, _)| i as u32) // UPGRADED: Local indices are now u32
                .collect(),
        }
    }

    /// The Ultimate Parity Checker:
    /// Verifies that the B-Tree's mathematical bounds perfectly match the real string.
    fn assert_tracker_matches_string(tracker: &LineTracker, real_text: &str) {
        let expected_lines: Vec<&str> = real_text.split('\n').collect();
        let total_lines = expected_lines.len();

        // 1. Verify Line Offsets
        #[allow(clippy::needless_range_loop)]
        for line_idx in 0..total_lines {
            // Cast the loop index to u64 for the tree query
            let start_byte = tracker
                .byte_offset_of_line(line_idx as u64)
                .unwrap_or_else(|| panic!("Tracker missing line {}!", line_idx))
                as usize; // Cast down for string slicing

            let end_byte = tracker
                .byte_offset_of_line((line_idx + 1) as u64)
                .map(|next| (next - 1) as usize)
                .unwrap_or(real_text.len());

            let extracted_line = &real_text[start_byte..end_byte];

            assert_eq!(
                extracted_line, expected_lines[line_idx],
                "Mismatch at line {}! Expected '{}', got '{}'",
                line_idx, expected_lines[line_idx], extracted_line
            );
        }

        // 2. Verify Total Measure Parity
        let root_idx = tracker.tree.root_idx.expect("Tree should not be empty");
        let root_measure = tracker.tree.pool[root_idx].measure();

        assert_eq!(
            root_measure.byte_count as usize, // Direct field access
            real_text.len(),
            "Tree total byte count diverges from string!"
        );

        assert_eq!(
            root_measure.line_count as usize, // Direct field access
            expected_lines.len() - 1,         // newlines = lines - 1
            "Tree total newline count diverges from string!"
        );
    }

    #[test]
    fn chunk_measure_is_correct() {
        let text = "Hello\nWorld\n!";
        let chunk = create_chunk(text);

        assert_eq!(chunk.get_measure().byte_count, 13);
        assert_eq!(chunk.get_measure().line_count, 2);
    }

    #[test]
    fn test_insert_at_middle_correctly_with_strings() {
        let mut tracker = LineTracker::new();
        let mut shadow_text = String::new();

        // 1. Insert base text
        let text1 = "Line 0\nLine 2";
        tracker.insert(LineTrackerSummary::new(0, 0), create_chunk(text1));
        shadow_text.insert_str(0, text1);

        // 2. Insert exactly in the middle (splicing at byte 7)
        let text2 = "Line 1\n";
        tracker.insert(LineTrackerSummary::new(7, 1), create_chunk(text2));
        shadow_text.insert_str(7, text2);

        assert_tracker_matches_string(&tracker, &shadow_text);

        let root_idx = tracker.tree.root_idx.unwrap();
        if let MeasuredBTreeNode::Leaf { data, .. } = &tracker.tree.pool[root_idx] {
            assert_eq!(
                data.len(),
                2,
                "Optimization: Left half and new chunk should merge!"
            );
            assert_eq!(data[0].byte_length, 14);
            assert_eq!(data[1].byte_length, 6);
        } else {
            panic!("Root should be a Leaf");
        }
    }

    #[test]
    fn deletes_middle_and_splits_correctly_with_strings() {
        let mut tracker = LineTracker::new();
        let mut shadow_text = String::new();

        let initial_text = "Line 0\nDeleteMe\nLine 2";
        tracker.insert(LineTrackerSummary::new(0, 0), create_chunk(initial_text));
        shadow_text.insert_str(0, initial_text);

        let del_start = 7;
        let del_len = 9;

        shadow_text.replace_range(del_start..(del_start + del_len), "");
        tracker.delete_range(del_start as u64, (del_start + del_len) as u64);

        assert_tracker_matches_string(&tracker, &shadow_text);
    }

    #[test]
    fn deletes_spanning_multiple_pieces_with_strings() {
        let mut tracker = LineTracker::new();
        let mut shadow_text = String::new();

        tracker.insert(LineTrackerSummary::new(0, 0), create_chunk("A\n"));
        shadow_text.insert_str(0, "A\n");

        tracker.insert(LineTrackerSummary::new(2, 1), create_chunk("B\n"));
        shadow_text.insert_str(2, "B\n");

        tracker.insert(LineTrackerSummary::new(4, 2), create_chunk("C\n"));
        shadow_text.insert_str(4, "C\n");

        shadow_text.replace_range(1..5, "");
        tracker.delete_range(1, 5);

        assert_tracker_matches_string(&tracker, &shadow_text);
    }

    #[test]
    fn merges_small_adjacent_inserts_perfectly() {
        let mut tracker = LineTracker::new();
        let mut shadow_text = String::new();

        let inserts = ["Hello", " ", "World", "!\n", "Next Line\n"];

        for text in inserts {
            let byte_pos = shadow_text.len() as u64;
            let line_pos = shadow_text.matches('\n').count() as u64;

            tracker.insert(
                LineTrackerSummary::new(byte_pos, line_pos),
                create_chunk(text),
            );
            shadow_text.insert_str(byte_pos as usize, text);
        }

        assert_tracker_matches_string(&tracker, &shadow_text);

        let root_idx = tracker.tree.root_idx.unwrap();
        if let MeasuredBTreeNode::Leaf { data, .. } = &tracker.tree.pool[root_idx] {
            assert_eq!(
                data.len(),
                1,
                "Optimization failed: Small chunks did not merge!"
            );
            assert_eq!(data[0].byte_length, shadow_text.len() as u32);
        } else {
            panic!("Expected Root to be a Leaf");
        }
    }

    #[test]
    fn refuses_to_merge_when_chunk_limit_reached() {
        let mut tracker = LineTracker::new();
        let mut shadow_text = String::new();

        let mut full_chunk_text = String::new();
        for _ in 0..MAX_CHUNK_LINES {
            full_chunk_text.push_str("Line\n");
        }

        tracker.insert(
            LineTrackerSummary::new(0, 0),
            create_chunk(&full_chunk_text),
        );
        shadow_text.insert_str(0, &full_chunk_text);

        let extra_line = "Overflow Line\n";
        let byte_pos = shadow_text.len() as u64;
        let line_pos = MAX_CHUNK_LINES as u64;

        tracker.insert(
            LineTrackerSummary::new(byte_pos, line_pos),
            create_chunk(extra_line),
        );
        shadow_text.insert_str(byte_pos as usize, extra_line);

        assert_tracker_matches_string(&tracker, &shadow_text);

        let root_idx = tracker.tree.root_idx.unwrap();
        if let MeasuredBTreeNode::Leaf { data, .. } = &tracker.tree.pool[root_idx] {
            assert_eq!(
                data.len(),
                2,
                "Chunk exceeded MAX_CHUNK_LINES but still merged!"
            );
            assert_eq!(data[0].newlines.len(), MAX_CHUNK_LINES as usize);
            assert_eq!(data[1].newlines.len(), 1);
        } else {
            panic!("Expected Root to be a Leaf");
        }
    }

    #[test]
    fn continuous_appending_forces_tree_growth_due_to_limits() {
        let mut tracker = LineTracker::new();
        let mut shadow_text = String::new();

        let capacity_trigger = (DATA_CAPACITY as u64) * (MAX_CHUNK_LINES as u64) + 5;

        for i in 0..capacity_trigger {
            let text = format!("Line {}\n", i);
            let byte_pos = shadow_text.len() as u64;
            let line_pos = i;

            tracker.insert(
                LineTrackerSummary::new(byte_pos, line_pos),
                create_chunk(&text),
            );
            shadow_text.push_str(&text);

            if i % 50 == 0 {
                assert_tracker_matches_string(&tracker, &shadow_text);
            }
        }

        assert_tracker_matches_string(&tracker, &shadow_text);

        let root_idx = tracker.tree.root_idx.unwrap();
        assert!(
            matches!(
                tracker.tree.pool[root_idx],
                MeasuredBTreeNode::Internal { .. }
            ),
            "Tree failed to grow! Uncapped merging likely prevented chunk creation."
        );
    }

    #[test]
    fn slicing_a_full_chunk_allows_future_merges() {
        let mut tracker = LineTracker::new();
        let mut shadow_text = String::new();

        let mut full_text = String::new();
        for _ in 0..MAX_CHUNK_LINES {
            full_text.push_str("A\n");
        }

        tracker.insert(LineTrackerSummary::default(), create_chunk(&full_text));
        shadow_text.insert_str(0, &full_text);

        let mid_byte = (shadow_text.len() / 2) as u64;
        let mid_line = (MAX_CHUNK_LINES / 2) as u64;
        let splice_text = "SPLICE\n";

        tracker.insert(
            LineTrackerSummary::new(mid_byte, mid_line),
            create_chunk(splice_text),
        );
        shadow_text.insert_str(mid_byte as usize, splice_text);

        assert_tracker_matches_string(&tracker, &shadow_text);

        let root_idx = tracker.tree.root_idx.unwrap();
        if let MeasuredBTreeNode::Leaf { data, .. } = &tracker.tree.pool[root_idx] {
            assert_eq!(
                data.len(),
                2,
                "Splice didn't merge with the left half correctly"
            );
            assert_eq!(data[0].newlines.len(), ((MAX_CHUNK_LINES / 2) + 1) as usize);
            assert_eq!(data[1].newlines.len(), (MAX_CHUNK_LINES / 2) as usize);
        }
    }

    #[test]
    fn stress_test_random_inserts_and_fuzzing() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let mut tracker = LineTracker::new();
        let mut shadow_text = String::new();

        let mut rng = StdRng::seed_from_u64(1337);
        let iterations = 2_000;

        for i in 0..iterations {
            let word_len = rng.random_range(1..20);
            let mut text = String::with_capacity(word_len);
            for _ in 0..word_len {
                if rng.random_bool(0.2) {
                    text.push('\n');
                } else {
                    text.push(rng.random_range(b'a'..=b'z') as char);
                }
            }

            let current_len = shadow_text.len();
            let insert_byte = if current_len == 0 {
                0
            } else {
                rng.random_range(0..=current_len)
            };
            let insert_line = shadow_text[..insert_byte].matches('\n').count();

            tracker.insert(
                LineTrackerSummary::new(insert_byte as u64, insert_line as u64),
                create_chunk(&text),
            );
            shadow_text.insert_str(insert_byte, &text);

            if i % 100 == 0 {
                assert_tracker_matches_string(&tracker, &shadow_text);
            }
        }

        assert_tracker_matches_string(&tracker, &shadow_text);
    }

    #[test]
    fn test_reproduce_benchmark_delete_crash() {
        let mut tracker = LineTracker::new();
        let mut total_bytes = 0u64;

        for i in 0..10_000 {
            let text = "Line to be partially deleted\n";
            tracker.insert(
                LineTrackerSummary::new(total_bytes, i as u64),
                create_chunk(text),
            );
            total_bytes += text.len() as u64;
        }

        let start_byte = total_bytes / 4;
        let end_byte = total_bytes - (total_bytes / 4);

        tracker.delete_range(start_byte, end_byte);
    }

    #[test]
    fn test_line_chunk_split_out_of_bounds_overflow() {
        let mut chunk = LineChunk {
            byte_length: 20,
            newlines: vec![9, 19], // This is safely inferred as u32
        };

        let out_of_bounds_measure = LineTrackerSummary::new(50, 5);

        let right_chunk = chunk.split_off(out_of_bounds_measure);

        assert_eq!(chunk.byte_length, 20, "Left chunk should remain unchanged");
        assert_eq!(
            right_chunk.byte_length, 0,
            "Right chunk should be completely empty"
        );
        assert!(
            right_chunk.newlines.is_empty(),
            "Right chunk should have no newlines"
        );
    }
}
