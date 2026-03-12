/// # Purpose
///
/// Used to determine the initial capacity of the `ObjectPool`.
/// This is important to minimize the number of reallocations
/// needed as objects are added to the pool, which can improve performance.
pub(crate) const INITIAL_OBJECT_POOL_CAPACITY: usize = 1024;

pub(crate) const MINIMUM_BTREE_DEGREE: u8 = 16;
/// # Reasoning
///
/// The maximum amount of `Data` instances a `Node::Leaf` can hold. The formula is
/// `2 * T - 1` or twice the minimum B-Tree degree (branching factor)
/// because of B-Tree rules, which is, the tree should always be balanced.
///
/// During splitting, if we take one piece from the `Node` that's full,
/// we would be left with `(2 * T - 1) - 1` or `2 * T - 2`. If we were
/// to divide that between two `Node` instances (since we're splitting),
/// it would be equal to `T - 1`. With that, both `Node` instances
/// would have exactly `T - 1` `MeasuredBTreeData` instances.
///
/// This way, there would be no need to reallocate and deallocate the `MeasuredBTreeData` and
/// `children` vectors of a `Node` as often as it would.
///
/// # Example
///
/// Let T = 3
///
/// Minimum items: 3 - 1 = 2
/// Maximum items: 2 * 3 - 1 = 5
///
/// If a `Node` has 5 items `[A, B, C, D, E]`, it will split itself:
///
/// - **`[C]`:** The new node pushed to the parent
/// - **`[A, B]`:** The left node (exactly 2 items)
/// - **`[D, E]`:** The right node (exactly 2 items)
pub(crate) const BTREE_DATA_CAPACITY: u8 = MINIMUM_BTREE_DEGREE * 2 - 1;
/// # Purpose
///
/// Used to determine the maximum amount of children a `Node::Internal` can have. This is important for\
/// the B-Tree to make sure that it remains balanced and that all leaf nodes are at the same depth.
pub(crate) const BTREE_CHILD_CAPACITY: u8 = MINIMUM_BTREE_DEGREE * 2;
/// # Purpose
///
/// Used to determine the minimum amount of `Data` instances a `Node::Leaf` can hold. This is
/// important for the B-Tree to maintain its properties, especially during merging and borrowing operations.
pub(crate) const BTREE_DATA_MIN_CAP: u8 = MINIMUM_BTREE_DEGREE - 1;
/// # Purpose
///
/// Used to determine the minimum amount of children a `Node::Internal` can have. This is
/// important for the B-Tree to maintain its properties, especially during merging and borrowing
/// operations. In a B-Tree of minimum degree `T`, an internal node must have at least `T`
/// children (except for the root, which can have fewer). This ensures that the tree remains
/// balanced and that all leaf nodes are at the same depth.
pub(crate) const BTREE_CHILDREN_MIN_CAP: u8 = MINIMUM_BTREE_DEGREE;

pub(crate) const MAX_LINE_ENDING_SAMPLE_SIZE: usize = 8192; // 8 KB, which is a common buffer size for file I/O operations and should be sufficient to capture line ending patterns in most files.

pub const NEWLINE_BYTE: u8 = b'\n';
pub const CARRIAGE_RETURN_BYTE: u8 = b'\r';

pub(crate) type ObjectPoolIndex = usize;
