#[cfg(test)]
mod piece_table_piece_tests {
    use mynotes_core::btree::MeasuredBTreeData;
    use mynotes_core::piece_table::{BufferKind, Piece};

    #[test]
    fn piece_measure_is_correct() {
        let piece = Piece {
            buffer_kind: BufferKind::Add,
            start: 4,
            end: 8,
        };

        assert_eq!(piece.measure(), 4);
    }
}

#[cfg(test)]
mod piece_table_node_tests {
    use mynotes_core::btree::{
        DATA_CAPACITY, MeasuredBTree, MeasuredBTreeNode, NODE_CHILDREN_CAPACITY,
    };
    use mynotes_core::piece_table::{BufferKind, Piece};

    /// Creates a dummy piece of a specific length for testing.
    fn dummy_piece(len: usize) -> Piece {
        Piece {
            buffer_kind: BufferKind::Add,
            start: 0,
            end: len,
        }
    }

    #[test]
    fn creates_node_correctly() {
        let leaf: MeasuredBTreeNode<Piece> = MeasuredBTreeNode::Leaf {
            measure: 0,
            data: Vec::with_capacity(DATA_CAPACITY),
        };

        let internal: MeasuredBTreeNode<Piece> = MeasuredBTreeNode::Internal {
            measure: 0,
            children: Vec::with_capacity(NODE_CHILDREN_CAPACITY),
        };

        if let MeasuredBTreeNode::Leaf { data, measure } = leaf {
            assert_eq!(data.len(), 0);
            assert_eq!(data.capacity(), DATA_CAPACITY);
            assert_eq!(measure, 0);
        } else {
            panic!("Expected Leaf");
        }

        if let MeasuredBTreeNode::Internal { children, measure } = internal {
            assert_eq!(children.len(), 0);
            assert_eq!(children.capacity(), NODE_CHILDREN_CAPACITY);
            assert_eq!(measure, 0);
        } else {
            panic!("Expected Internal");
        }
    }

    #[test]
    fn test_get_location_routing() {
        // --- Setup a Dummy Pool ---
        // Document of length 15:
        //
        //       [ Root (Internal, measure 15) ]
        //         /                   \
        // [ Leaf 1, measure 10 ]      [ Leaf 2, measure 5 ]
        //   |-- Piece A (measure 5)     |-- Piece C (measure 5)
        //   |-- Piece B (measure 5)

        let piece_a = dummy_piece(5);
        let piece_b = dummy_piece(5);
        let piece_c = dummy_piece(5);

        let leaf1 = MeasuredBTreeNode::Leaf {
            data: vec![piece_a, piece_b],
            measure: 10,
        };
        let leaf2 = MeasuredBTreeNode::Leaf {
            data: vec![piece_c],
            measure: 5,
        };
        let root = MeasuredBTreeNode::Internal {
            children: vec![1, 2], // Pointers to leaf1 (idx 1) and leaf2 (idx 2)
            measure: 15,
        };

        let mut tree = MeasuredBTree::new();
        tree.pool = vec![root, leaf1, leaf2];
        tree.root_idx = Some(0);

        // --- Scenario 1: Look for an index inside Piece B (Leaf 1) ---
        // Target: Absolute Index 7.
        // Expected: Should land in Leaf 1 (pool idx 1), Piece 1, with local offset 2 (7 - 5).
        let loc1 = tree.get_location(7).expect("Should find location");

        assert_eq!(loc1.0, 1, "Should have routed to Leaf 1 (pool index 1)");
        assert_eq!(loc1.1, 1, "Should be the second piece in the leaf");
        assert_eq!(loc1.2, 2, "Local offset should be 2");

        // --- Scenario 2: Look for an index inside Piece C (Leaf 2) ---
        // Target: Absolute Index 12.
        // Expected: Should land in Leaf 2 (pool idx 2), Piece 0, with local offset 2 (12 - 10).
        let loc2 = tree.get_location(12).expect("Should find location");

        assert_eq!(loc2.0, 2, "Should have routed to Leaf 2 (pool index 2)");
        assert_eq!(loc2.1, 0, "Should be the first piece in the leaf");
        assert_eq!(loc2.2, 2, "Local offset should be 2");
    }
}

#[cfg(test)]
mod piece_table_tests {
    use mynotes_core::btree::{DATA_CAPACITY, MeasuredBTreeData, MeasuredBTreeNode};
    use mynotes_core::piece_table::{BufferKind, PieceTable};
    // Note: These tests assume your PieceTable API has methods like:
    // `table.insert(doc_offset, buffer_start, length, buffer_kind)`
    // `table.delete(doc_offset, length)`
    // and a method `table.len()` which gets the root measure.

    #[test]
    fn inserts_empty_and_gets_location_correctly() {
        let mut table = PieceTable::new();

        assert!(table.insert(0, 0, 10, BufferKind::Add).is_ok());

        let Some((pool_idx, piece_idx, offset)) = table.tree.get_location(5) else {
            panic!("get_location(5) should return a location after a valid insertion");
        };

        assert_eq!(piece_idx, 0, "Should be in the first piece");
        assert_eq!(
            offset, 5,
            "Local offset should match the absolute index here"
        );
        assert_eq!(pool_idx, 0, "Should be the first pool");
    }

    #[test]
    fn detects_out_of_bounds_correctly() {
        let mut table = PieceTable::new();

        table.insert(0, 0, 10, BufferKind::Add).unwrap();
        assert!(
            table.tree.get_location(10).is_none(),
            "Absolute index equal to length should return None"
        );
        assert!(
            table.tree.get_location(999).is_none(),
            "Indices far beyond length should return None"
        );
    }

    #[test]
    fn merges_contiguous_piece_correctly() {
        let mut table = PieceTable::new();

        // Insert first chunk (doc_offset: 0, buf_start: 0, len: 5)
        table.insert(0, 0, 5, BufferKind::Add).unwrap();
        // Insert second chunk perfectly contiguous in both Document and Buffer
        table.insert(5, 5, 5, BufferKind::Add).unwrap();

        let root_idx = table.tree.root_idx.unwrap();

        if let MeasuredBTreeNode::Leaf { data, measure } = &table.tree.pool[root_idx] {
            assert_eq!(*measure, 10, "Total measure should update to 10");
            assert_eq!(
                data.len(),
                1,
                "Pieces should have MERGED into exactly 1 piece"
            );
            assert_eq!(data[0].start, 0);
            assert_eq!(data[0].end, 10, "Piece range should span the merged length");
        } else {
            panic!("Expected root to be a Leaf");
        }
    }

    #[test]
    fn insert_at_middle_correctly() {
        let mut table = PieceTable::new();

        // Insert "HelloWorld" (length 10)
        table.insert(0, 0, 10, BufferKind::Original).unwrap();
        // Insert " " (length 1) in the middle at doc_offset 5.
        table.insert(5, 0, 1, BufferKind::Add).unwrap();

        let root_idx = table.tree.root_idx.unwrap();

        if let MeasuredBTreeNode::Leaf { data, measure } = &table.tree.pool[root_idx] {
            assert_eq!(*measure, 11);
            assert_eq!(data.len(), 3, "Splicing should result in 3 distinct pieces");

            // Validate the left split ("Hello")
            assert_eq!(data[0].buffer_kind, BufferKind::Original);
            assert_eq!(data[0].start, 0);
            assert_eq!(data[0].end, 5);

            // Validate the middle insert (" ")
            assert_eq!(data[1].buffer_kind, BufferKind::Add);
            assert_eq!(data[1].start, 0);
            assert_eq!(data[1].end, 1);

            // Validate the right split ("World")
            assert_eq!(data[2].buffer_kind, BufferKind::Original);
            assert_eq!(data[2].start, 5);
            assert_eq!(data[2].end, 10);
        } else {
            panic!("Expected root to be a Leaf");
        }

        let (pool_idx, piece_idx, offset) = table.tree.get_location(8).unwrap();

        assert_eq!(piece_idx, 2, "Should route to the 3rd piece");
        assert_eq!(offset, 2, "Local offset should be 2 into the 3rd piece");
        assert_eq!(pool_idx, 0, "Should be the first pool");
    }

    #[test]
    fn internal_node_split_maintains_total_measure() {
        let mut table = PieceTable::new();

        // 1000 insertions is more than enough to force multiple leaves
        // AND internal nodes to overflow and split, regardless of your capacity limits.
        let num_inserts = 1000;

        for i in 0..num_inserts {
            // Always insert exactly in the middle of the current document.
            // This forces nodes to split right down the middle, triggering `split_internal`.
            let insert_pos = i / 2;

            table.insert(insert_pos, i, 1, BufferKind::Add).unwrap();

            // Grab the root node to check its weight
            let root_idx = table.tree.root_idx.expect("Tree should have a root");
            let actual_measure = table.tree.pool[root_idx].measure();

            // THE CATCH: The root's total measure MUST equal the number of items
            // we have inserted so far. If `split_internal` overwrites a branch
            // and orphans it, this assertion will fail on the exact iteration it happens!
            assert_eq!(
                actual_measure,
                i + 1,
                "CRITICAL B-TREE FAILURE: Tree lost weight after inserting item {}. \
             Expected total measure to be {}, but root reported {}. \
             An internal node split likely orphaned a branch!",
                i,
                i + 1,
                actual_measure
            );
        }
    }

    #[test]
    fn inserts_into_split_tree_correctly() {
        let mut table = PieceTable::new();
        let cap = DATA_CAPACITY;

        // 1. Force a split by inserting alternating buffers to prevent merging
        for i in 0..=cap {
            let kind = if i % 2 == 0 {
                BufferKind::Add
            } else {
                BufferKind::Original
            };
            table.insert(i, i, 1, kind).unwrap();
        }

        let root_idx = table.tree.root_idx.unwrap();

        // Verify we have an internal root with 2 children
        let (left_idx, right_idx) = match &table.tree.pool[root_idx] {
            MeasuredBTreeNode::Internal { children, .. } => {
                assert_eq!(children.len(), 2, "Tree should have split into 2 leaves");
                (children[0], children[1])
            }
            _ => panic!("Expected root to be Internal"),
        };

        // 2. Insert at the very beginning (doc_offset = 0). Should route to Left child.
        table.insert(0, 100, 5, BufferKind::Add).unwrap();

        // 3. Insert at the very end. Should route to Right child.
        let total_measure = table.tree.pool[root_idx].measure();
        table
            .insert(total_measure, 200, 5, BufferKind::Add)
            .unwrap();

        // Assertions
        if let MeasuredBTreeNode::Leaf { data, .. } = &table.tree.pool[left_idx] {
            assert_eq!(
                data[0].start, 100,
                "Leftmost piece should be the new insert"
            );
            assert_eq!(data[0].measure(), 5);
        } else {
            panic!("Left child is not a leaf");
        }

        if let MeasuredBTreeNode::Leaf { data, .. } = &table.tree.pool[right_idx] {
            let last_idx = data.len() - 1;
            assert_eq!(
                data[last_idx].start, 200,
                "Rightmost piece should be the new insert"
            );
            assert_eq!(data[last_idx].measure(), 5);
        } else {
            panic!("Right child is not a leaf");
        }
    }

    #[test]
    fn splits_nodes_correctly() {
        let mut table = PieceTable::new();
        let iterations = DATA_CAPACITY + 2;

        for i in 0..iterations {
            let kind = if i % 2 == 0 {
                BufferKind::Add
            } else {
                BufferKind::Original
            };
            table.insert(i, i, 1, kind).unwrap();
        }

        let root_idx = table.tree.root_idx.unwrap();

        match &table.tree.pool[root_idx] {
            MeasuredBTreeNode::Internal { children, measure } => {
                assert_eq!(
                    *measure, iterations,
                    "Internal node should track total measure"
                );
                assert_eq!(
                    children.len(),
                    2,
                    "Root should have split into exactly 2 children"
                );
            }
            MeasuredBTreeNode::Leaf { .. } => {
                panic!("Root should have become an Internal node after capacity was reached!")
            }
        }

        let loc = table.tree.get_location(iterations - 1);
        assert!(
            loc.is_some(),
            "Should be able to find the last inserted character"
        );
    }

    #[test]
    fn deletes_middle_and_splits_correctly() {
        let mut table = PieceTable::new();

        table.insert(0, 0, 10, BufferKind::Original).unwrap();
        // FIX: Delete from the middle, not the start.
        // Start at doc_offset 4, delete 2 chars.
        table.delete(4, 2).unwrap();

        let root_idx = table.tree.root_idx.unwrap();

        if let MeasuredBTreeNode::Leaf { data, measure } = &table.tree.pool[root_idx] {
            assert_eq!(*measure, 8, "Total measure should drop to 8");
            assert_eq!(
                data.len(),
                2,
                "Deletion from middle should split the piece in two"
            );

            // Left side
            assert_eq!(data[0].start, 0);
            assert_eq!(data[0].end, 4);

            // Right side (Original 0..10, minus 4..6, leaves 6..10)
            assert_eq!(data[1].start, 6);
            assert_eq!(data[1].end, 10);
        } else {
            panic!("Expected root to be a Leaf");
        }
    }

    #[test]
    fn deletes_start_correctly() {
        let mut table = PieceTable::new();

        table.insert(0, 0, 10, BufferKind::Original).unwrap();
        // Delete 2 chars from the very beginning. This should trigger a Left Trim.
        table.delete(0, 2).unwrap();

        let root_idx = table.tree.root_idx.unwrap();

        if let MeasuredBTreeNode::Leaf { data, measure } = &table.tree.pool[root_idx] {
            assert_eq!(*measure, 8);
            assert_eq!(
                data.len(),
                1,
                "Left trim should just modify the existing piece, not split it"
            );

            // The piece should now start at 2
            assert_eq!(data[0].start, 2);
            assert_eq!(data[0].end, 10);
        } else {
            panic!("Expected root to be a Leaf");
        }
    }

    #[test]
    fn deletes_end_correctly() {
        let mut table = PieceTable::new();

        table.insert(0, 0, 10, BufferKind::Original).unwrap();
        // Delete 2 chars from the very end. This should trigger a Right Trim.
        table.delete(8, 2).unwrap();

        let root_idx = table.tree.root_idx.unwrap();

        if let MeasuredBTreeNode::Leaf { data, measure } = &table.tree.pool[root_idx] {
            assert_eq!(*measure, 8);
            assert_eq!(
                data.len(),
                1,
                "Right trim should just modify the existing piece, not split it"
            );

            // The piece should now end at 8
            assert_eq!(data[0].start, 0);
            assert_eq!(data[0].end, 8);
        } else {
            panic!("Expected root to be a Leaf");
        }
    }

    #[test]
    fn deletes_spanning_multiple_pieces_correctly() {
        let mut table = PieceTable::new();

        table.insert(0, 0, 5, BufferKind::Original).unwrap(); // A: 0..5
        table.insert(5, 5, 5, BufferKind::Add).unwrap(); // B: 5..10
        table.insert(10, 10, 5, BufferKind::Original).unwrap(); // C: 10..15
        // We will delete 8 characters starting at doc_offset 3.
        // This will:
        // - Right trim A (delete 2 chars) -> leaving A as 0..3
        // - Completely delete B (delete 5 chars)
        // - Left trim C (delete 1 char) -> leaving C as 11..15
        table.delete(3, 8).unwrap();

        let root_idx = table.tree.root_idx.unwrap();

        if let MeasuredBTreeNode::Leaf { data, measure } = &table.tree.pool[root_idx] {
            assert_eq!(*measure, 7, "15 total - 8 deleted = 7");
            assert_eq!(
                data.len(),
                2,
                "The middle piece should be completely removed, leaving 2"
            );

            // Remainder of A
            assert_eq!(data[0].start, 0);
            assert_eq!(data[0].end, 3);
            assert_eq!(data[0].buffer_kind, BufferKind::Original);

            // Remainder of C
            assert_eq!(data[1].start, 11);
            assert_eq!(data[1].end, 15);
            assert_eq!(data[1].buffer_kind, BufferKind::Original);
        } else {
            panic!("Expected root to be a Leaf");
        }
    }

    #[test]
    fn underflow_borrows_from_left_sibling() {
        let mut table = PieceTable::new();
        let cap = DATA_CAPACITY;
        let min_cap = cap / 2;

        // 1. Fill the first leaf exactly to capacity
        for i in 0..cap {
            let kind = if i % 2 == 0 {
                BufferKind::Add
            } else {
                BufferKind::Original
            };
            table.insert(i, i, 1, kind).unwrap();
        }

        // 2. Insert one more at the end to force a split.
        // NOTE: Because i=30 (if cap=31) was BufferKind::Add, this new piece
        // will merge with it into a single piece of length 2!
        table.insert(cap, cap, 1, BufferKind::Add).unwrap();

        // 3. Pad the Left child so it has plenty of pieces to spare.
        for i in 0..min_cap {
            let kind = if i % 2 == 0 {
                BufferKind::Original
            } else {
                BufferKind::Add
            };
            table.insert(0, i, 1, kind).unwrap();
        }

        let root_idx = table.tree.root_idx.expect("Tree should have a root");

        let right_child_idx =
            if let MeasuredBTreeNode::Internal { children, .. } = &table.tree.pool[root_idx] {
                children[1]
            } else {
                panic!("Root must be Internal");
            };

        // 4. Force ACTUAL piece removal!
        let current_len = table.tree.pool[root_idx].measure();

        // We delete 4 characters to slice through the merged piece at the end
        // and guarantee we remove enough pieces to drop below min_cap.
        table.delete(current_len - 4, 4).unwrap();

        let right_len_after = match &table.tree.pool[right_child_idx] {
            MeasuredBTreeNode::Leaf { data, .. } => data.len(),
            _ => panic!("Expected leaf"),
        };

        // We assert that the length should remain >= min_cap.
        // Since we know borrowing isn't implemented yet, this WILL fail,
        // giving us our exact test-driven development target!
        assert!(
            right_len_after >= min_cap,
            "TARGET ACQUIRED: The right node dropped below min_cap (Count: {}). Borrowing failed or is unimplemented.",
            right_len_after
        );
    }

    #[test]
    fn underflow_merges_siblings_when_borrowing_fails() {
        let mut table = PieceTable::new();

        // DATA_CAPACITY is 31, so this will insert 32 pieces.
        for i in 0..=DATA_CAPACITY {
            let kind = if i % 2 == 0 {
                BufferKind::Add
            } else {
                BufferKind::Original
            };
            table.insert(i, i, 1, kind).unwrap();
        }

        // Minimum capacity of a node is DATA_CAPACITY / 2, in this case, 15.
        // So, since left would have 15 and the right would have 17
        // since they split, we need to delete enough to merge them again.
        // We just delete 3 so 32 - 3 = 29. There wouldn't be enough
        // to satisfy the minimum of 15 for either nodes.
        table.delete(0, 3).unwrap();

        let new_root_idx = table.tree.root_idx.unwrap();

        match &table.tree.pool[new_root_idx] {
            MeasuredBTreeNode::Leaf { data, .. } => {
                assert_eq!(
                    data.len(),
                    DATA_CAPACITY - 2,
                    "Siblings should have merged back into a single full leaf"
                );
            }
            MeasuredBTreeNode::Internal { .. } => {
                panic!("Root should have collapsed back into a Leaf after children merged");
            }
        }
    }
}
