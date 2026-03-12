use crate::types::{INITIAL_OBJECT_POOL_CAPACITY, ObjectPoolIndex};

pub(crate) struct PoolableWrapper<T> {
    pub object: T,
    pub is_alive: bool,
}

pub trait Poolable {
    #[must_use]
    fn new() -> Self;
    fn reset(&mut self);
}

pub struct ObjectPool<T>
where
    T: Poolable,
{
    pool: Vec<PoolableWrapper<T>>,
    free_object_indices: Vec<ObjectPoolIndex>,
}

impl<T> ObjectPool<T>
where
    T: Poolable,
{
    pub fn new() -> Self {
        ObjectPool {
            pool: Vec::with_capacity(INITIAL_OBJECT_POOL_CAPACITY),
            free_object_indices: Vec::with_capacity(INITIAL_OBJECT_POOL_CAPACITY),
        }
    }

    /// # Purpose
    ///
    /// Allocates an object from the pool and returns its index.
    /// If there are free indices available, it reuses them; otherwise,
    /// it creates a new object and adds it to the pool.
    ///
    /// # Returns
    ///
    /// The index of the allocated object in the pool.
    ///
    /// # Examples
    ///
    /// ```
    /// use utils::data_structures::object_pool::{ObjectPool, Poolable};
    ///
    /// struct MyObject {
    ///     value: i32,
    /// }
    ///
    /// impl Poolable for MyObject {
    ///     fn new() -> Self {
    ///        MyObject { value: 0 }
    ///    }
    ///
    ///   fn reset(&mut self) {
    ///       self.value = 0;
    ///   }
    /// }
    ///
    /// let mut pool = ObjectPool::<MyObject>::new();
    /// let index = pool.allocate();
    ///
    /// assert_eq!(pool.get_object(index).unwrap().value, 0);
    /// assert_eq!(pool.len(), 1);
    /// ```
    pub fn allocate(&mut self) -> ObjectPoolIndex {
        let Some(index) = self.free_object_indices.pop() else {
            let index = self.pool.len();

            self.pool.push(PoolableWrapper {
                object: T::new(),
                is_alive: true,
            });

            return index;
        };

        self.pool[index].is_alive = true;

        index
    }

    /// # Purpose
    ///
    /// Deallocates an object by its index, resetting it and
    /// marking it as free and available for future allocations.
    /// If the index is out of bounds, it's a no-op.
    ///
    /// # Examples
    ///
    /// ```
    /// use utils::data_structures::object_pool::{ObjectPool, Poolable};
    ///
    /// struct MyObject {
    ///     value: i32,
    /// }
    ///
    /// impl Poolable for MyObject {
    ///     fn new() -> Self {
    ///        MyObject { value: 0 }
    ///    }
    ///
    ///     fn reset(&mut self) {
    ///         self.value = 0;
    ///     }
    /// }
    ///
    /// let mut pool = ObjectPool::<MyObject>::new();
    /// let index = pool.allocate();
    ///
    /// assert_eq!(index, 0);
    ///
    /// pool.deallocate(index);
    ///
    /// assert!(pool.get_object(index).is_none());
    /// assert_eq!(pool.len(), 0);
    /// assert_eq!(pool.allocate(), index);
    /// ```
    pub fn deallocate(&mut self, index: ObjectPoolIndex) {
        if index >= self.pool.len() {
            return;
        }

        self.pool[index].is_alive = false;

        self.pool[index].object.reset();
        self.free_object_indices.push(index);
    }

    /// # Purpose
    ///
    /// Retrieves an immutable reference to an object by its index if it's alive;
    /// otherwise, returns `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use utils::data_structures::object_pool::{ObjectPool, Poolable};
    ///
    /// struct MyObject {
    ///     value: i32,
    /// }
    ///
    /// impl Poolable for MyObject {
    ///     fn new() -> Self {
    ///         MyObject { value: 0 }
    ///     }
    ///
    ///    fn reset(&mut self) {
    ///        self.value = 0;
    ///    }
    /// }
    ///
    /// let mut pool = ObjectPool::<MyObject>::new();
    /// let index = pool.allocate();
    ///
    /// assert_eq!(pool.get_object(index).unwrap().value, 0);
    /// ```
    pub fn get_object(&self, index: ObjectPoolIndex) -> Option<&T> {
        self.pool.get(index).map(|wrapper| {
            if wrapper.is_alive {
                Some(&wrapper.object)
            } else {
                None
            }
        })?
    }

    /// # Purpose
    ///
    /// Retrieves a mutable reference to an object by its index if it's alive;
    /// otherwise, returns `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use utils::data_structures::object_pool::{ObjectPool, Poolable};
    ///
    /// struct MyObject {
    ///     value: i32,
    /// }
    ///
    /// impl Poolable for MyObject {
    ///     fn new() -> Self {
    ///         MyObject { value: 0 }
    ///     }
    ///
    ///    fn reset(&mut self) {
    ///        self.value = 0;
    ///    }
    /// }
    ///
    /// let mut pool = ObjectPool::<MyObject>::new();
    /// let index = pool.allocate();
    ///
    /// assert_eq!(pool.get_object_mut(index).unwrap().value, 0);
    ///
    /// {
    ///     let obj = pool.get_object_mut(index).unwrap();
    ///
    ///     obj.value = 42;
    /// }
    /// assert_eq!(pool.get_object(index).unwrap().value, 42);
    /// ```
    pub fn get_object_mut(&mut self, index: ObjectPoolIndex) -> Option<&mut T> {
        self.pool.get_mut(index).map(|wrapper| {
            if wrapper.is_alive {
                Some(&mut wrapper.object)
            } else {
                None
            }
        })?
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.pool.len() - self.free_object_indices.len()
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod object_pool_tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct MyObject {
        value: i32,
    }

    impl Poolable for MyObject {
        fn new() -> Self {
            MyObject { value: 0 }
        }

        fn reset(&mut self) {
            self.value = 0;
        }
    }

    #[test]
    fn test_allocate() {
        let mut pool = ObjectPool::<MyObject>::new();
        let index = pool.allocate();

        assert_eq!(pool.get_object(index).unwrap().value, 0);
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_deallocate() {
        let mut pool = ObjectPool::<MyObject>::new();
        let index = pool.allocate();

        assert_eq!(index, 0);

        pool.deallocate(index);

        assert!(pool.get_object(index).is_none());
        assert_eq!(pool.len(), 0);
        assert_eq!(pool.allocate(), index);
    }

    #[test]
    fn test_get_object() {
        let mut pool = ObjectPool::<MyObject>::new();
        let index = pool.allocate();

        assert_eq!(pool.get_object(index).unwrap().value, 0);
    }

    #[test]
    fn test_get_object_mut() {
        let mut pool = ObjectPool::<MyObject>::new();
        let index = pool.allocate();

        assert_eq!(pool.get_object_mut(index).unwrap().value, 0);

        {
            let obj = pool.get_object_mut(index).unwrap();

            obj.value = 42;
        }

        assert_eq!(pool.get_object(index).unwrap().value, 42);
    }

    #[test]
    fn test_len_and_is_empty() {
        let mut pool = ObjectPool::<MyObject>::new();

        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);

        let index1 = pool.allocate();
        let index2 = pool.allocate();

        assert!(!pool.is_empty());
        assert_eq!(pool.len(), 2);

        pool.deallocate(index1);

        assert!(!pool.is_empty());
        assert_eq!(pool.len(), 1);

        pool.deallocate(index2);

        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn test_deallocate_out_of_bounds_noop() {
        let mut pool = ObjectPool::<MyObject>::new();

        pool.deallocate(999);

        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn test_deallocate_resets_object() {
        let mut pool = ObjectPool::<MyObject>::new();

        // 1. Allocate an object
        let index = pool.allocate();

        // 2. Modify the object inside a scope block.
        // Once this block ends, the mutable borrow on `pool` is released.
        {
            let obj = pool.get_object_mut(index).unwrap();
            obj.value = 42;
        }

        // 3. Now we can safely mutate the pool to deallocate
        pool.deallocate(index);

        // 4. Verify the parallel `is_alive` array is working
        assert!(pool.get_object(index).is_none());

        // 5. Re-allocate. Our free list should hand us the exact same index.
        let new_index = pool.allocate();
        assert_eq!(index, new_index);

        // 6. Verify that `deallocate` actually called `reset()` on the object
        let reused_obj = pool.get_object(new_index).unwrap();
        assert_eq!(reused_obj.value, 0);
    }
}
