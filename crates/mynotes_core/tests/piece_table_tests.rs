#[cfg(test)]
mod piece_table_piece_tests {
    use mynotes_core::piece_table::{BufferKind, Piece};

    #[test]
    fn piece_len_is_correct() {
        let piece = Piece {
            buf_kind: BufferKind::Add,
            position: 4..8,
        };

        assert_eq!(piece.len(), 4);
        assert!(!piece.is_empty());
    }
}

#[cfg(test)]
mod piece_table_node_tests {
    use mynotes_core::piece_table::{
        BufferKind, NODE_CHILDREN_CAPACITY, Node, PIECES_CAPACITY, Piece,
    };

    /// Creates a dummy piece of a specific length for testing.
    fn dummy_piece(len: usize) -> Piece {
        Piece {
            // Assuming BufferKind::Add exists; adjust to match your enum
            buf_kind: BufferKind::Add,
            position: 0..len,
        }
    }

    #[test]
    fn creates_node_correctly() {
        let Node::Leaf {
            pieces,
            total_len: leaf_len,
        } = Node::new(true)
        else {
            panic!("Node::new(true) returns a non-leaf node")
        };
        let Node::Internal {
            children,
            total_len: internal_len,
        } = Node::new(false)
        else {
            panic!("Node::new(false) returns a non-leaf node")
        };

        assert_eq!(pieces.len(), 0);
        assert_eq!(children.len(), 0);
        assert_eq!(pieces.capacity(), PIECES_CAPACITY);
        assert_eq!(children.capacity(), NODE_CHILDREN_CAPACITY);
        assert_eq!(leaf_len, 0);
        assert_eq!(internal_len, 0);
    }

    #[test]
    fn clear_resets_state_correctly() {
        let mut leaf = Node::new(true);
        leaf.get_mut_pieces().push(dummy_piece(5));
        *leaf.mut_len() = 5;

        leaf.clear();
        assert!(leaf.is_empty());
        assert_eq!(leaf.len(), 0);

        if let Node::Leaf { pieces, .. } = leaf {
            assert!(pieces.is_empty());
            assert_eq!(pieces.capacity(), PIECES_CAPACITY);
        } else {
            unreachable!("Expected a Leaf node");
        }
    }

    #[test]
    fn detects_full_capacity_correctly() {
        let mut leaf = Node::new(true);

        // Fill the leaf up to its capacity
        for _ in 0..PIECES_CAPACITY {
            leaf.get_mut_pieces().push(dummy_piece(1));
        }

        assert!(leaf.is_full());

        let internal = Node::new(false);

        assert!(!internal.is_full());
    }

    #[test]
    #[should_panic(expected = "`get_mut_pieces` is called within a `Node::Internal`")]
    fn test_get_mut_pieces_panics_on_internal() {
        let mut internal = Node::new(false);
        let _ = internal.get_mut_pieces(); // This should panic
    }

    #[test]
    fn test_get_location_routing() {
        // --- Setup a Dummy Pool ---
        // We will build a small tree representing a document of length 15:
        //
        //       [ Root (Internal, len 15) ]
        //         /                   \
        // [ Leaf 1, len 10 ]      [ Leaf 2, len 5 ]
        //   |-- Piece A (len 5)     |-- Piece C (len 5)
        //   |-- Piece B (len 5)

        let piece_a = dummy_piece(5);
        let piece_b = dummy_piece(5);
        let piece_c = dummy_piece(5);

        let leaf1 = Node::Leaf {
            pieces: vec![piece_a, piece_b],
            total_len: 10,
        };
        let leaf2 = Node::Leaf {
            pieces: vec![piece_c],
            total_len: 5,
        };
        let root = Node::Internal {
            children: vec![1, 2], // Pointers to leaf1 (idx 1) and leaf2 (idx 2)
            total_len: 15,
        };

        let pool = vec![root, leaf1, leaf2];

        // --- Scenario 1: Look for an index inside Piece B (Leaf 1) ---
        // Target: Absolute Index 7.
        // Expected: Should land in Leaf 1 (pool idx 1), Piece 1, with local offset 2 (7 - 5).
        let mut abs_idx = 7;
        let mut current_idx = 0; // Start at root
        let piece_idx = pool[0].get_location(&mut abs_idx, &mut current_idx, &pool);

        assert_eq!(
            current_idx, 1,
            "Should have routed to Leaf 1 (pool index 1)"
        );
        assert_eq!(piece_idx, 1, "Should be the second piece in the leaf");
        assert_eq!(abs_idx, 2, "Local offset should be 2");

        // --- Scenario 2: Look for an index inside Piece C (Leaf 2) ---
        // Target: Absolute Index 12.
        // Expected: Should land in Leaf 2 (pool idx 2), Piece 0, with local offset 2 (12 - 10).
        let mut abs_idx = 12;
        let mut current_idx = 0; // Start at root
        let piece_idx = pool[0].get_location(&mut abs_idx, &mut current_idx, &pool);

        assert_eq!(
            current_idx, 2,
            "Should have routed to Leaf 2 (pool index 2)"
        );
        assert_eq!(piece_idx, 0, "Should be the first piece in the leaf");
        assert_eq!(abs_idx, 2, "Local offset should be 2");
    }
}

#[cfg(test)]
mod piece_table_tests {
    use mynotes_core::piece_table::{BufferKind, Node, PIECES_CAPACITY, PieceTable};

    #[test]
    fn inserts_empty_and_gets_location_correctly() {
        let mut table = PieceTable::default();

        assert!(table.insert(0, 0, 10, BufferKind::Add).is_ok());
        assert_eq!(table.len(), 10);

        let Some((pool_idx, piece_idx, offset)) = table.get_location(5) else {
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
        let mut table = PieceTable::default();

        table.insert(0, 0, 10, BufferKind::Add).unwrap();
        assert!(
            table.get_location(10).is_none(),
            "Absolute index equal to length should return None"
        );
        assert!(
            table.get_location(999).is_none(),
            "Indices far beyond length should return None"
        );
    }

    #[test]
    fn no_insert_on_out_of_bounds_correctly() {
        let mut table = PieceTable::default();

        // Cannot insert at offset 5 when the document is empty
        let err = table.insert(5, 0, 10, BufferKind::Add);
        assert!(err.is_err(), "Should return PieceTableError::OutOfBounds");

        table.insert(0, 0, 10, BufferKind::Add).unwrap();

        // Document is length 10. Cannot insert at offset 11.
        let err2 = table.insert(11, 10, 5, BufferKind::Add);
        assert!(err2.is_err(), "Should return PieceTableError::OutOfBounds");
    }

    #[test]
    fn merges_contiguous_piece_correctly() {
        let mut table = PieceTable::default();

        // Insert first chunk
        table.insert(0, 0, 5, BufferKind::Add).unwrap();
        // Insert second chunk perfectly contiguous in both Document and Buffer
        table.insert(5, 5, 5, BufferKind::Add).unwrap();

        let root_idx = table.root_idx.unwrap();

        if let Node::Leaf { pieces, total_len } = &table.pool[root_idx] {
            assert_eq!(*total_len, 10, "Total length should update to 10");
            assert_eq!(
                pieces.len(),
                1,
                "Pieces should have MERGED into exactly 1 piece"
            );
            assert_eq!(
                pieces[0].position,
                0..10,
                "Piece range should span the merged length"
            );
        } else {
            panic!("Expected root to be a Leaf");
        }
    }

    #[test]
    fn insert_at_middle_correctly() {
        let mut table = PieceTable::default();

        // Insert "HelloWorld" (length 10)
        table.insert(0, 0, 10, BufferKind::Original).unwrap();
        // Insert " " (length 1) in the middle at doc_offset 5.
        // This should split the original piece into THREE pieces: "Hello", " ", "World"
        table.insert(5, 0, 1, BufferKind::Add).unwrap();

        let root_idx = table.root_idx.unwrap();

        if let Node::Leaf { pieces, total_len } = &table.pool[root_idx] {
            assert_eq!(*total_len, 11);
            assert_eq!(
                pieces.len(),
                3,
                "Splicing should result in 3 distinct pieces"
            );
            // Validate the left split ("Hello")
            assert_eq!(pieces[0].buf_kind, BufferKind::Original);
            assert_eq!(pieces[0].position, 0..5);
            // Validate the middle insert (" ")
            assert_eq!(pieces[1].buf_kind, BufferKind::Add);
            assert_eq!(pieces[1].position, 0..1);
            // Validate the right split ("World")
            assert_eq!(pieces[2].buf_kind, BufferKind::Original);
            assert_eq!(pieces[2].position, 5..10);
        } else {
            panic!("Expected root to be a Leaf");
        }

        // Test routing to the right split
        let (pool_idx, piece_idx, offset) = table.get_location(8).unwrap(); // Index 8 should be 'r' in "World"

        assert_eq!(piece_idx, 2, "Should route to the 3rd piece");
        assert_eq!(offset, 2, "Local offset should be 2 into the 3rd piece");
        assert_eq!(pool_idx, 0, "Should be the first pool");
    }

    #[test]
    fn inserts_at_start_without_merge_correctly() {
        let mut table = PieceTable::default();

        table.insert(0, 0, 5, BufferKind::Original).unwrap();
        // Insert at the very beginning
        table.insert(0, 0, 5, BufferKind::Add).unwrap();

        let root_idx = table.root_idx.unwrap();

        if let Node::Leaf { pieces, .. } = &table.pool[root_idx] {
            assert_eq!(pieces.len(), 2);
            assert_eq!(
                pieces[0].buf_kind,
                BufferKind::Add,
                "New piece should be pushed to the front"
            );
            assert_eq!(
                pieces[1].buf_kind,
                BufferKind::Original,
                "Old piece should be shifted right"
            );
        }
    }

    #[test]
    fn splits_nodes_correctly() {
        let mut table = PieceTable::default();
        // We will insert alternating buffers at the end to prevent contiguous merging,
        // forcing the piece array to grow until it exceeds PIECES_CAPACITY.
        let iterations = PIECES_CAPACITY + 2;

        for i in 0..iterations {
            let kind = if i % 2 == 0 {
                BufferKind::Add
            } else {
                BufferKind::Original
            };

            // Append 1 byte at a time
            table.insert(i, i, 1, kind).unwrap();
        }

        let root_idx = table.root_idx.unwrap();

        // After exceeding PIECES_CAPACITY, the root MUST have split into an Internal node
        match &table.pool[root_idx] {
            Node::Internal {
                children,
                total_len,
            } => {
                assert_eq!(
                    *total_len, iterations,
                    "Internal node should track total length"
                );
                assert_eq!(
                    children.len(),
                    2,
                    "Root should have split into exactly 2 children"
                );
            }
            Node::Leaf { .. } => {
                panic!("Root should have become an Internal node after capacity was reached!")
            }
        }

        // Validate routing still works across the split
        // The last character inserted should be found in the right-side child.
        let loc = table.get_location(iterations - 1);

        assert!(
            loc.is_some(),
            "Should be able to find the last inserted character"
        );
    }

    #[test]
    fn deletes_middle_and_splits_correctly() {
        let mut table = PieceTable::default();

        // Insert "HelloWorld" (len 10)
        table.insert(0, 0, 10, BufferKind::Original).unwrap();
        // Delete 2 chars starting at index 4 ("oW") -> Leaves "Hellorld" (len 8)
        table.delete(4, 2).unwrap();

        let root_idx = table.root_idx.unwrap();

        if let Node::Leaf { pieces, total_len } = &table.pool[root_idx] {
            assert_eq!(*total_len, 8);
            assert_eq!(
                pieces.len(),
                2,
                "Deletion from middle should split the piece in two"
            );
            // Piece 1: "Hell"
            assert_eq!(pieces[0].position, 0..4);
            // Piece 2: "orld"
            // Original was 0..10. We deleted 4..6. So remainder is 6..10.
            assert_eq!(pieces[1].position, 6..10);
        } else {
            panic!("Expected root to be a Leaf");
        }
    }

    #[test]
    fn deletes_exact_piece_correctly() {
        let mut table = PieceTable::default();

        table.insert(0, 0, 5, BufferKind::Original).unwrap(); // "Hello"
        table.insert(5, 5, 5, BufferKind::Add).unwrap(); // "World"
        // Delete exactly the first piece
        table.delete(0, 5).unwrap();

        let root_idx = table.root_idx.unwrap();

        if let Node::Leaf { pieces, total_len } = &table.pool[root_idx] {
            assert_eq!(*total_len, 5);
            assert_eq!(
                pieces.len(),
                1,
                "The first piece should be completely removed"
            );
            assert_eq!(pieces[0].buf_kind, BufferKind::Add);
            assert_eq!(pieces[0].position, 5..10);
        }
    }

    #[test]
    fn deletes_spanning_multiple_pieces_correctly() {
        let mut table = PieceTable::default();

        table.insert(0, 0, 5, BufferKind::Original).unwrap(); // "Hello" (0..5)
        table.insert(5, 5, 5, BufferKind::Add).unwrap(); // "World" (5..10)
        table.insert(10, 10, 5, BufferKind::Original).unwrap(); // "Today" (10..15)

        // Delete "loWor" (Index 3, Length 5)
        // - Trims right of "Hello" (leaves "Hel")
        // - Removes "loWor"
        table.delete(3, 5).unwrap();

        let root_idx = table.root_idx.unwrap();

        if let Node::Leaf { pieces, total_len } = &table.pool[root_idx] {
            assert_eq!(*total_len, 10);
            assert_eq!(pieces.len(), 3);

            assert_eq!(pieces[0].position, 0..3); // "Hel"
            assert_eq!(pieces[1].position, 8..10); // "ld"
            assert_eq!(pieces[2].position, 10..15); // "Today"
        }
    }
}
