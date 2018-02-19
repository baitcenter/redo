use std::collections::hash_map::{HashMap, RandomState};
use std::hash::{BuildHasher, Hash};
use record::Commands;
use {Command, Error, Record, Stack};

/// A group of either stacks or records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Group<K: Hash + Eq, V, S = RandomState> where S: BuildHasher {
    map: HashMap<K, V, S>,
    active: Option<K>,
}

impl<K: Hash + Eq, V, S: BuildHasher> Group<K, V, S> {
    /// Returns a new group.
    #[inline]
    pub fn new() -> Group<K, V, RandomState> {
        Default::default()
    }

    /// Returns a new group with the given capacity.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Group<K, V, RandomState> {
        Group::with_capacity_and_hasher(capacity, Default::default())
    }

    /// Returns a new group with the given hasher.
    #[inline]
    pub fn with_hasher(hash_builder: S) -> Group<K, V, S> {
        Group {
            map: HashMap::with_hasher(hash_builder),
            active: None,
        }
    }

    /// Returns a new group with the given capacity and hasher.
    #[inline]
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Group<K, V, S> {
        Group {
            map: HashMap::with_capacity_and_hasher(capacity, hash_builder),
            active: None,
        }
    }

    /// Returns the capacity of the group.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.map.capacity()
    }

    /// Returns the number of items in the group.
    #[inline]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if the group is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Inserts an item into the group.
    #[inline]
    pub fn insert(&mut self, k: K, v: V) -> Option<V> {
        self.map.insert(k, v)
    }

    /// Removes an item from the group.
    #[inline]
    pub fn remove(&mut self, k: &K) -> Option<V> {
        self.map.remove(k)
    }

    /// Gets the current active item in the group.
    #[inline]
    pub fn get(&self) -> Option<&V> {
        self.active.as_ref().and_then(|active| self.map.get(active))
    }

    /// Sets the current active item in the group.
    #[inline]
    pub fn set<T: Into<Option<K>>>(&mut self, k: T) -> bool {
        match k.into() {
            Some(ref k) if !self.map.contains_key(k) => false,
            k => {
                self.active = k;
                true
            }
        }
    }
}

impl<K: Hash + Eq, R, C: Command<R>, S: BuildHasher> Group<K, Stack<R, C>, S> {
    /// Calls the [`push`] method on the active stack.
    ///
    /// [`push`]: stack/struct.Stack.html#method.push
    #[inline]
    pub fn push(&mut self, cmd: C) -> Option<Result<(), Error<R, C>>> {
        let map = &mut self.map;
        self.active
            .as_ref()
            .and_then(|active| map.get_mut(active))
            .map(move |stack| stack.push(cmd))
    }

    /// Calls the [`pop`] method on the active stack.
    ///
    /// [`pop`]: stack/struct.Stack.html#method.pop
    #[inline]
    pub fn pop(&mut self) -> Option<Result<C, Error<R, C>>> {
        let map = &mut self.map;
        self.active
            .as_ref()
            .and_then(|active| map.get_mut(active))
            .and_then(|stack| stack.pop())
    }
}

impl<'a, K: Hash + Eq, R, C: Command<R>, S: BuildHasher> Group<K, Record<'a, R, C>, S> {
    /// Calls the [`is_clean`] method on the active record.
    ///
    /// [`is_clean`]: record/struct.Record.html#method.is_clean
    #[inline]
    pub fn is_clean(&self) -> Option<bool> {
        self.get().map(|record| record.is_clean())
    }

    /// Calls the [`is_dirty`] method on the active record.
    ///
    /// [`is_dirty`]: record/struct.Record.html#method.is_dirty
    #[inline]
    pub fn is_dirty(&self) -> Option<bool> {
        self.is_clean().map(|is_clean| !is_clean)
    }

    /// Calls the [`push`] method on the active record.
    ///
    /// [`push`]: record/struct.Record.html#method.push
    #[inline]
    pub fn push(&mut self, cmd: C) -> Option<Result<Commands<C>, Error<R, C>>> {
        let map = &mut self.map;
        self.active
            .as_ref()
            .and_then(|active| map.get_mut(active))
            .map(move |record| record.push(cmd))
    }

    /// Calls the [`redo`] method on the active record.
    ///
    /// [`redo`]: record/struct.Record.html#method.redo
    #[inline]
    pub fn redo(&mut self) -> Option<Result<(), C::Err>> {
        let map = &mut self.map;
        self.active
            .as_ref()
            .and_then(|active| map.get_mut(active))
            .and_then(|record| record.redo())
    }

    /// Calls the [`undo`] method on the active record.
    ///
    /// [`undo`]: record/struct.Record.html#method.undo
    #[inline]
    pub fn undo(&mut self) -> Option<Result<(), C::Err>> {
        let map = &mut self.map;
        self.active
            .as_ref()
            .and_then(|active| map.get_mut(active))
            .and_then(|record| record.undo())
    }
}

impl<K: Hash + Eq, V, S: BuildHasher + Default> Default for Group<K, V, S> {
    #[inline]
    fn default() -> Group<K, V, S> {
        Group::with_hasher(Default::default())
    }
}
