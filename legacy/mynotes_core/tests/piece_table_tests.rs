#[cfg(test)]
mod piece_table_node_tests {
    use mynotes_core::btree::{
        DATA_CAPACITY, MeasuredBTree, MeasuredBTreeNode, NODE_CHILDREN_CAPACITY,
    };
    use mynotes_core::piece_table::{BufferKind, Piece};

    /// Creates a dummy piece of a specific length for testing.
    fn dummy_piece(len: u64) -> Piece {
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
            data: Vec::with_capacity(DATA_CAPACITY as usize), // Cast to usize
        };

        let internal: MeasuredBTreeNode<Piece> = MeasuredBTreeNode::Internal {
            measure: 0,
            children: Vec::with_capacity(NODE_CHILDREN_CAPACITY as usize), // Cast to usize
        };

        if let MeasuredBTreeNode::Leaf { data, measure } = leaf {
            assert_eq!(data.len(), 0);
            assert_eq!(data.capacity(), DATA_CAPACITY as usize); // Cast to usize
            assert_eq!(measure, 0);
        } else {
            panic!("Expected Leaf");
        }

        if let MeasuredBTreeNode::Internal { children, measure } = internal {
            assert_eq!(children.len(), 0);
            assert_eq!(children.capacity(), NODE_CHILDREN_CAPACITY as usize); // Cast to usize
            assert_eq!(measure, 0);
        } else {
            panic!("Expected Internal");
        }
    }

    #[test]
    fn test_get_location_routing() {
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
            children: vec![1, 2],
            measure: 15,
        };

        let mut tree = MeasuredBTree::new();
        tree.pool = vec![root, leaf1, leaf2];
        tree.root_idx = Some(0);

        let loc1 = tree.get_location(7).expect("Should find location");

        assert_eq!(loc1.0, 1, "Should have routed to Leaf 1 (pool index 1)");
        assert_eq!(loc1.1, 1, "Should be the second piece in the leaf");
        assert_eq!(loc1.2, 2, "Local offset should be 2");

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

        table.insert(0, 0, 5, BufferKind::Add).unwrap();
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

        table.insert(0, 0, 10, BufferKind::Original).unwrap();
        table.insert(5, 0, 1, BufferKind::Add).unwrap();

        let root_idx = table.tree.root_idx.unwrap();

        if let MeasuredBTreeNode::Leaf { data, measure } = &table.tree.pool[root_idx] {
            assert_eq!(*measure, 11);
            assert_eq!(data.len(), 3, "Splicing should result in 3 distinct pieces");

            assert_eq!(data[0].buffer_kind, BufferKind::Original);
            assert_eq!(data[0].start, 0);
            assert_eq!(data[0].end, 5);

            assert_eq!(data[1].buffer_kind, BufferKind::Add);
            assert_eq!(data[1].start, 0);
            assert_eq!(data[1].end, 1);

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
        let num_inserts = 1000;

        for i in 0..num_inserts {
            let insert_pos = i / 2;

            table
                .insert(insert_pos as u64, i as u64, 1, BufferKind::Add)
                .unwrap();

            let root_idx = table.tree.root_idx.expect("Tree should have a root");
            let actual_measure = table.tree.pool[root_idx].measure();

            assert_eq!(
                actual_measure,
                (i + 1) as u64,
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
        let cap = DATA_CAPACITY as usize; // Cast up front for easier looping

        for i in 0..=cap {
            let kind = if i % 2 == 0 {
                BufferKind::Add
            } else {
                BufferKind::Original
            };
            table.insert(i as u64, i as u64, 1, kind).unwrap();
        }

        let root_idx = table.tree.root_idx.unwrap();

        let (left_idx, right_idx) = match &table.tree.pool[root_idx] {
            MeasuredBTreeNode::Internal { children, .. } => {
                assert_eq!(children.len(), 2, "Tree should have split into 2 leaves");
                (children[0], children[1])
            }
            _ => panic!("Expected root to be Internal"),
        };

        table.insert(0, 100, 5, BufferKind::Add).unwrap();

        let total_measure = table.tree.pool[root_idx].measure();
        table
            .insert(total_measure, 200, 5, BufferKind::Add)
            .unwrap();

        if let MeasuredBTreeNode::Leaf { data, .. } = &table.tree.pool[left_idx] {
            assert_eq!(
                data[0].start, 100,
                "Leftmost piece should be the new insert"
            );
            assert_eq!(data[0].get_measure(), 5);
        } else {
            panic!("Left child is not a leaf");
        }

        if let MeasuredBTreeNode::Leaf { data, .. } = &table.tree.pool[right_idx] {
            let last_idx = data.len() - 1;
            assert_eq!(
                data[last_idx].start, 200,
                "Rightmost piece should be the new insert"
            );
            assert_eq!(data[last_idx].get_measure(), 5);
        } else {
            panic!("Right child is not a leaf");
        }
    }

    #[test]
    fn splits_nodes_correctly() {
        let mut table = PieceTable::new();
        let iterations = (DATA_CAPACITY as usize) + 2; // Cast here

        for i in 0..iterations {
            let kind = if i % 2 == 0 {
                BufferKind::Add
            } else {
                BufferKind::Original
            };
            table.insert(i as u64, i as u64, 1, kind).unwrap();
        }

        let root_idx = table.tree.root_idx.unwrap();

        match &table.tree.pool[root_idx] {
            MeasuredBTreeNode::Internal { children, measure } => {
                assert_eq!(
                    *measure, iterations as u64,
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

        let loc = table.tree.get_location((iterations - 1) as u64);
        assert!(
            loc.is_some(),
            "Should be able to find the last inserted character"
        );
    }

    #[test]
    fn deletes_middle_and_splits_correctly() {
        let mut table = PieceTable::new();

        table.insert(0, 0, 10, BufferKind::Original).unwrap();
        table.delete(4, 2).unwrap();

        let root_idx = table.tree.root_idx.unwrap();

        if let MeasuredBTreeNode::Leaf { data, measure } = &table.tree.pool[root_idx] {
            assert_eq!(*measure, 8, "Total measure should drop to 8");
            assert_eq!(
                data.len(),
                2,
                "Deletion from middle should split the piece in two"
            );

            assert_eq!(data[0].start, 0);
            assert_eq!(data[0].end, 4);

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
        table.delete(0, 2).unwrap();

        let root_idx = table.tree.root_idx.unwrap();

        if let MeasuredBTreeNode::Leaf { data, measure } = &table.tree.pool[root_idx] {
            assert_eq!(*measure, 8);
            assert_eq!(
                data.len(),
                1,
                "Left trim should just modify the existing piece, not split it"
            );

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
        table.delete(8, 2).unwrap();

        let root_idx = table.tree.root_idx.unwrap();

        if let MeasuredBTreeNode::Leaf { data, measure } = &table.tree.pool[root_idx] {
            assert_eq!(*measure, 8);
            assert_eq!(
                data.len(),
                1,
                "Right trim should just modify the existing piece, not split it"
            );

            assert_eq!(data[0].start, 0);
            assert_eq!(data[0].end, 8);
        } else {
            panic!("Expected root to be a Leaf");
        }
    }

    #[test]
    fn deletes_spanning_multiple_pieces_correctly() {
        let mut table = PieceTable::new();

        table.insert(0, 0, 5, BufferKind::Original).unwrap();
        table.insert(5, 5, 5, BufferKind::Add).unwrap();
        table.insert(10, 10, 5, BufferKind::Original).unwrap();

        table.delete(3, 8).unwrap();

        let root_idx = table.tree.root_idx.unwrap();

        if let MeasuredBTreeNode::Leaf { data, measure } = &table.tree.pool[root_idx] {
            assert_eq!(*measure, 7, "15 total - 8 deleted = 7");
            assert_eq!(
                data.len(),
                2,
                "The middle piece should be completely removed, leaving 2"
            );

            assert_eq!(data[0].start, 0);
            assert_eq!(data[0].end, 3);
            assert_eq!(data[0].buffer_kind, BufferKind::Original);

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
        let cap = DATA_CAPACITY as usize; // Cast here
        let min_cap = cap / 2;

        for i in 0..cap {
            let kind = if i % 2 == 0 {
                BufferKind::Add
            } else {
                BufferKind::Original
            };
            table.insert(i as u64, i as u64, 1, kind).unwrap();
        }

        table
            .insert(cap as u64, cap as u64, 1, BufferKind::Add)
            .unwrap();

        for i in 0..min_cap {
            let kind = if i % 2 == 0 {
                BufferKind::Original
            } else {
                BufferKind::Add
            };
            table.insert(0, i as u64, 1, kind).unwrap();
        }

        let root_idx = table.tree.root_idx.expect("Tree should have a root");

        let right_child_idx =
            if let MeasuredBTreeNode::Internal { children, .. } = &table.tree.pool[root_idx] {
                children[1]
            } else {
                panic!("Root must be Internal");
            };

        let current_len = table.tree.pool[root_idx].measure();

        table.delete(current_len - 4, 4).unwrap();

        let right_len_after = match &table.tree.pool[right_child_idx] {
            MeasuredBTreeNode::Leaf { data, .. } => data.len(),
            _ => panic!("Expected leaf"),
        };

        assert!(
            right_len_after >= min_cap,
            "TARGET ACQUIRED: The right node dropped below min_cap (Count: {}). Borrowing failed or is unimplemented.",
            right_len_after
        );
    }

    #[test]
    fn underflow_merges_siblings_when_borrowing_fails() {
        let mut table = PieceTable::new();
        let cap = DATA_CAPACITY as usize; // Cast here

        for i in 0..=cap {
            let kind = if i % 2 == 0 {
                BufferKind::Add
            } else {
                BufferKind::Original
            };
            table.insert(i as u64, i as u64, 1, kind).unwrap();
        }

        table.delete(0, 3).unwrap();

        let new_root_idx = table.tree.root_idx.unwrap();

        match &table.tree.pool[new_root_idx] {
            MeasuredBTreeNode::Leaf { data, .. } => {
                assert_eq!(
                    data.len(),
                    cap - 2, // Adjusted to match the casted cap
                    "Siblings should have merged back into a single full leaf"
                );
            }
            MeasuredBTreeNode::Internal { .. } => {
                panic!("Root should have collapsed back into a Leaf after children merged");
            }
        }
    }
}
