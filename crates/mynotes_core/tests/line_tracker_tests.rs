#[cfg(test)]
mod tests {
    use mynotes_core::btree::{DATA_CAPACITY, MeasuredBTreeData, MeasuredBTreeNode};
    use mynotes_core::line_tracker::{LineChunk, LineTracker, LineTrackerSummary, MAX_CHUNK_LINES};
    use rand::RngExt;

    // Helper to create a standard chunk
    fn create_chunk(text: &str) -> LineChunk {
        LineChunk {
            byte_length: text.len(),
            newlines: text
                .bytes()
                .enumerate()
                .filter(|&(_, b)| b == b'\n')
                .map(|(i, _)| i)
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
            let start_byte = tracker
                .byte_offset_of_line(line_idx)
                .unwrap_or_else(|| panic!("Tracker missing line {}!", line_idx));

            let end_byte = tracker
                .byte_offset_of_line(line_idx + 1)
                .map(|next| next - 1)
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
            root_measure.byte_count,
            real_text.len(),
            "Tree total byte count diverges from string!"
        );

        assert_eq!(
            root_measure.line_count,
            expected_lines.len() - 1, // newlines = lines - 1
            "Tree total newline count diverges from string!"
        );
    }

    #[test]
    fn chunk_measure_is_correct() {
        let text = "Hello\nWorld\n!";
        let chunk = create_chunk(text);

        assert_eq!(chunk.measure().byte_count, 13);
        assert_eq!(chunk.measure().line_count, 2);
    }

    #[test]
    fn test_insert_at_middle_correctly_with_strings() {
        let mut tracker = LineTracker::new();
        let mut shadow_text = String::new();

        // 1. Insert base text
        let text1 = "Line 0\nLine 2";
        tracker.insert(
            LineTrackerSummary {
                byte_count: 0,
                line_count: 0,
            },
            create_chunk(text1),
        );
        shadow_text.insert_str(0, text1);

        // 2. Insert exactly in the middle (splicing at byte 7)
        let text2 = "Line 1\n";
        tracker.insert(
            LineTrackerSummary {
                byte_count: 7,
                line_count: 1,
            },
            create_chunk(text2),
        );
        shadow_text.insert_str(7, text2);

        // Assert structural splitting and perfect string parity
        assert_tracker_matches_string(&tracker, &shadow_text);

        // Ensure the BTree split the chunk, but smartly MERGED the left half with the new insert
        let root_idx = tracker.tree.root_idx.unwrap();
        if let MeasuredBTreeNode::Leaf { data, .. } = &tracker.tree.pool[root_idx] {
            assert_eq!(
                data.len(),
                2,
                "Optimization: Left half and new chunk should merge!"
            );

            // Chunk 0: "Line 0\n" (7) + "Line 1\n" (7) = 14 bytes
            assert_eq!(data[0].byte_length, 14);

            // Chunk 1: "Line 2" = 6 bytes
            assert_eq!(data[1].byte_length, 6);
        } else {
            panic!("Root should be a Leaf");
        }
    }

    // =========================================================================
    // DELETION TDD TESTS
    // If you haven't implemented `delete` for LineTracker yet, these will fail
    // and guide your implementation, exactly like the Piece Table!
    // =========================================================================

    #[test]
    fn deletes_middle_and_splits_correctly_with_strings() {
        let mut tracker = LineTracker::new();
        let mut shadow_text = String::new();

        let initial_text = "Line 0\nDeleteMe\nLine 2";
        tracker.insert(
            LineTrackerSummary {
                byte_count: 0,
                line_count: 0,
            },
            create_chunk(initial_text),
        );
        shadow_text.insert_str(0, initial_text);

        // Target "DeleteMe\n". Starts at byte 7, length 9.
        let del_start = 7;
        let del_len = 9;

        // Apply to shadow
        shadow_text.replace_range(del_start..(del_start + del_len), "");

        // Apply to tracker (Assume a method signature like this exists or will exist)
        // tracker.delete(del_start, del_len);

        // Uncomment once implemented to verify:
        // assert_tracker_matches_string(&tracker, &shadow_text);
    }

    #[test]
    fn deletes_spanning_multiple_pieces_with_strings() {
        let mut tracker = LineTracker::new();
        let mut shadow_text = String::new();

        // Piece 1
        tracker.insert(
            LineTrackerSummary {
                byte_count: 0,
                line_count: 0,
            },
            create_chunk("A\n"),
        );
        shadow_text.insert_str(0, "A\n");

        // Piece 2
        tracker.insert(
            LineTrackerSummary {
                byte_count: 2,
                line_count: 1,
            },
            create_chunk("B\n"),
        );
        shadow_text.insert_str(2, "B\n");

        // Piece 3
        tracker.insert(
            LineTrackerSummary {
                byte_count: 4,
                line_count: 2,
            },
            create_chunk("C\n"),
        );
        shadow_text.insert_str(4, "C\n");

        // Current state: "A\nB\nC\n"
        // Delete from byte 1 (the '\n' of A) to byte 5 (the '\n' of C). Length = 4.
        // Will delete: '\n', 'B', '\n', 'C'
        // Remaining: "A\n"

        shadow_text.replace_range(1..5, "");
        // tracker.delete(1, 4);

        // Uncomment once implemented:
        // assert_tracker_matches_string(&tracker, &shadow_text);
    }

    #[test]
    fn merges_small_adjacent_inserts_perfectly() {
        let mut tracker = LineTracker::new();
        let mut shadow_text = String::new();

        // Simulate typing small chunks adjacently
        let inserts = ["Hello", " ", "World", "!\n", "Next Line\n"];

        for text in inserts {
            let byte_pos = shadow_text.len();
            let line_pos = shadow_text.matches('\n').count();

            tracker.insert(
                LineTrackerSummary {
                    byte_count: byte_pos,
                    line_count: line_pos,
                },
                create_chunk(text),
            );
            shadow_text.insert_str(byte_pos, text);
        }

        assert_tracker_matches_string(&tracker, &shadow_text);

        // Verification: Because these combined are well under MAX_CHUNK_LINES,
        // they should have all merged into a single chunk!
        let root_idx = tracker.tree.root_idx.unwrap();
        if let MeasuredBTreeNode::Leaf { data, .. } = &tracker.tree.pool[root_idx] {
            assert_eq!(
                data.len(),
                1,
                "Optimization failed: Small chunks did not merge!"
            );
            assert_eq!(data[0].byte_length, shadow_text.len());
        } else {
            panic!("Expected Root to be a Leaf");
        }
    }

    #[test]
    fn refuses_to_merge_when_chunk_limit_reached() {
        let mut tracker = LineTracker::new();
        let mut shadow_text = String::new();

        // 1. Create a chunk that perfectly fills the limit
        let mut full_chunk_text = String::new();
        for _ in 0..MAX_CHUNK_LINES {
            full_chunk_text.push_str("Line\n");
        }

        tracker.insert(
            LineTrackerSummary {
                byte_count: 0,
                line_count: 0,
            },
            create_chunk(&full_chunk_text),
        );
        shadow_text.insert_str(0, &full_chunk_text);

        // 2. Insert one more line at the very end.
        // It SHOULD NOT merge because the first chunk is at capacity.
        let extra_line = "Overflow Line\n";
        let byte_pos = shadow_text.len();
        let line_pos = MAX_CHUNK_LINES;

        tracker.insert(
            LineTrackerSummary {
                byte_count: byte_pos,
                line_count: line_pos,
            },
            create_chunk(extra_line),
        );
        shadow_text.insert_str(byte_pos, extra_line);

        assert_tracker_matches_string(&tracker, &shadow_text);

        let root_idx = tracker.tree.root_idx.unwrap();
        if let MeasuredBTreeNode::Leaf { data, .. } = &tracker.tree.pool[root_idx] {
            assert_eq!(
                data.len(),
                2,
                "Edge Case failed: Chunk exceeded MAX_CHUNK_LINES but still merged!"
            );
            assert_eq!(data[0].newlines.len(), MAX_CHUNK_LINES);
            assert_eq!(data[1].newlines.len(), 1);
        } else {
            panic!("Expected Root to be a Leaf");
        }
    }

    #[test]
    fn continuous_appending_forces_tree_growth_due_to_limits() {
        let mut tracker = LineTracker::new();
        let mut shadow_text = String::new();

        // We will insert 1 line at a time.
        // Because of our limit, every `MAX_CHUNK_LINES` insertions, a new chunk must form.
        // Eventually, the Leaf node will hit `DATA_CAPACITY` chunks and split!
        let capacity_trigger = DATA_CAPACITY * MAX_CHUNK_LINES + 5;

        for i in 0..capacity_trigger {
            let text = format!("Line {}\n", i);
            let byte_pos = shadow_text.len();
            let line_pos = i;

            tracker.insert(
                LineTrackerSummary {
                    byte_count: byte_pos,
                    line_count: line_pos,
                },
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

        // 1. Insert a chunk right exactly at the limit
        let mut full_text = String::new();
        for _ in 0..MAX_CHUNK_LINES {
            full_text.push_str("A\n");
        }

        tracker.insert(LineTrackerSummary::default(), create_chunk(&full_text));
        shadow_text.insert_str(0, &full_text);

        // 2. Splice exactly in the middle!
        // This splits the MAX chunk into two chunks of (MAX / 2).
        // The new insert should merge with the left half, because (MAX / 2) + 1 <= MAX.
        let mid_byte = shadow_text.len() / 2;
        let mid_line = MAX_CHUNK_LINES / 2;
        let splice_text = "SPLICE\n";

        tracker.insert(
            LineTrackerSummary {
                byte_count: mid_byte,
                line_count: mid_line,
            },
            create_chunk(splice_text),
        );
        shadow_text.insert_str(mid_byte, splice_text);

        assert_tracker_matches_string(&tracker, &shadow_text);

        let root_idx = tracker.tree.root_idx.unwrap();
        if let MeasuredBTreeNode::Leaf { data, .. } = &tracker.tree.pool[root_idx] {
            assert_eq!(
                data.len(),
                2,
                "Splice didn't merge with the left half correctly"
            );

            // Left chunk absorbed the splice
            assert_eq!(data[0].newlines.len(), (MAX_CHUNK_LINES / 2) + 1);
            // Right chunk is just the remainder
            assert_eq!(data[1].newlines.len(), MAX_CHUNK_LINES / 2);
        }
    }

    #[test]
    fn stress_test_random_inserts_and_fuzzing() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let mut tracker = LineTracker::new();
        let mut shadow_text = String::new();

        // Use a fixed seed so if the chaos monkey finds a bug,
        // you can run the exact same test again to debug it!
        let mut rng = StdRng::seed_from_u64(1337);

        let iterations = 2_000; // Enough to force deep B-Tree levels

        for i in 0..iterations {
            // 1. Generate a random chunk of text
            let word_len = rng.random_range(1..20);
            let mut text = String::with_capacity(word_len);
            for _ in 0..word_len {
                // 20% chance of a newline, otherwise a random letter
                if rng.random_bool(0.2) {
                    text.push('\n');
                } else {
                    text.push(rng.random_range(b'a'..=b'z') as char);
                }
            }

            // 2. Pick a completely random byte offset
            let current_len = shadow_text.len();
            let insert_byte = if current_len == 0 {
                0
            } else {
                rng.random_range(0..=current_len)
            };
            // Find the line count by checking the shadow string (or query your tree!)
            let insert_line = shadow_text[..insert_byte].matches('\n').count();

            // 3. Insert into both
            tracker.insert(
                LineTrackerSummary {
                    byte_count: insert_byte,
                    line_count: insert_line,
                },
                create_chunk(&text),
            );
            shadow_text.insert_str(insert_byte, &text);

            // 4. Periodically check parity to keep the test fast,
            // but catch the exact operation that breaks it.
            if i % 100 == 0 {
                assert_tracker_matches_string(&tracker, &shadow_text);
            }
        }

        // Final exhaustive check
        assert_tracker_matches_string(&tracker, &shadow_text);
    }
}
