use std::collections::hash_map::{HashMap, RandomState};
use std::fmt::{self, Debug, Display, Formatter};
use std::hash::{BuildHasher, Hash};
use std::marker::PhantomData;
use {Command, Error, Record};

/// A group of records.
pub struct Group<K: Hash + Eq, V, S = RandomState> {
    group: HashMap<K, V, S>,
    active: Option<K>,
    signals: Option<Box<FnMut(Option<&K>) + Send + Sync + 'static>>,
}

impl<K: Hash + Eq, V> Group<K, V, RandomState> {
    /// Returns a new group.
    #[inline]
    pub fn new() -> Group<K, V, RandomState> {
        Default::default()
    }
}

impl<K: Hash + Eq, V, S: BuildHasher> Group<K, V, S> {
    /// Returns a builder for a group.
    #[inline]
    pub fn builder() -> GroupBuilder<K, V, S> {
        GroupBuilder {
            group: PhantomData,
            capacity: 0,
            signals: None,
        }
    }

    /// Reserves capacity for at least `additional` more items.
    ///
    /// # Panics
    /// Panics if the new capacity overflows usize.
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.group.reserve(additional);
    }

    /// Returns the capacity of the group.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.group.capacity()
    }

    /// Returns the number of items in the group.
    #[inline]
    pub fn len(&self) -> usize {
        self.group.len()
    }

    /// Returns `true` if the group is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.group.is_empty()
    }

    /// Sets how different signals should be handled when the state changes.
    #[inline]
    pub fn set_signals<F>(&mut self, f: F)
        where
            F: FnMut(Option<&K>) + Send + Sync + 'static,
    {
        self.signals = Some(Box::new(f) as _);
    }

    /// Inserts an item into the group.
    #[inline]
    pub fn insert(&mut self, k: K, v: V) -> Option<V> {
        self.group.insert(k, v)
    }

    /// Removes an item from the group.
    #[inline]
    pub fn remove(&mut self, k: &K) -> Option<V> {
        self.group.remove(k)
    }

    /// Gets a reference to the current active item in the group.
    #[inline]
    pub fn get(&self) -> Option<&V> {
        self.active.as_ref().and_then(|active| self.group.get(active))
    }

    /// Gets a mutable reference to the current active item in the group.
    #[inline]
    pub fn get_mut(&mut self) -> Option<&mut V> {
        let group = &mut self.group;
        self.active.as_ref().and_then(move |active| group.get_mut(active))
    }

    /// Sets the current active item in the group.
    ///
    /// Returns `None` if the item was successfully set, otherwise `k` is returned.
    #[inline]
    pub fn set(&mut self, k: K) -> Option<K> {
        if self.group.contains_key(&k) {
            if self.active.as_ref().map_or(false, |a| a != &k) {
                if let Some(ref mut f) = self.signals {
                    f(Some(&k))
                }
            }
            self.active = Some(k);
            None
        } else {
            Some(k)
        }
    }

    /// Unsets the current active item in the group.
    #[inline]
    pub fn unset(&mut self) {
        if self.active.is_some() {
            self.active = None;
            if let Some(ref mut f) = self.signals {
                f(None);
            }
        }
    }
}

impl<K: Hash + Eq, R, C: Command<R>, S: BuildHasher> Group<K, Record<R, C>, S> {
    /// Calls the [`set_saved`] method on the active record.
    ///
    /// [`set_saved`]: record/struct.Record.html#method.set_saved
    #[inline]
    pub fn set_saved(&mut self) -> Option<()> {
        self.get_mut().map(|record| record.set_saved())
    }

    /// Calls the [`set_unsaved`] method on the active record.
    ///
    /// [`set_unsaved`]: record/struct.Record.html#method.set_unsaved
    #[inline]
    pub fn set_unsaved(&mut self) -> Option<()> {
        self.get_mut().map(|record| record.set_unsaved())
    }

    /// Calls the [`is_saved`] method on the active record.
    ///
    /// [`is_saved`]: record/struct.Record.html#method.is_saved
    #[inline]
    pub fn is_saved(&self) -> Option<bool> {
        self.get().map(|record| record.is_saved())
    }

    /// Calls the [`apply`] method on the active record.
    ///
    /// [`apply`]: record/struct.Record.html#method.apply
    #[inline]
    pub fn apply(&mut self, cmd: C) -> Option<Result<impl Iterator<Item = C>, Error<R, C>>> {
        self.get_mut().map(move |record| record.apply(cmd))
    }

    /// Calls the [`undo`] method on the active record.
    ///
    /// [`undo`]: record/struct.Record.html#method.undo
    #[inline]
    pub fn undo(&mut self) -> Option<Result<(), C::Error>> {
        self.get_mut().and_then(|record| record.undo())
    }

    /// Calls the [`redo`] method on the active record.
    ///
    /// [`redo`]: record/struct.Record.html#method.redo
    #[inline]
    pub fn redo(&mut self) -> Option<Result<(), C::Error>> {
        self.get_mut().and_then(|record| record.redo())
    }

    /// Returns an iterator over the records.
    #[inline]
    pub fn records(&self) -> impl Iterator<Item = &Record<R, C>> {
        self.group.values()
    }
}

impl<K: Hash + Eq, R, C: Command<R> + ToString, S: BuildHasher> Group<K, Record<R, C>, S> {
    /// Calls the [`to_undo_string`] method on the active record.
    ///
    /// [`to_undo_string`]: record/struct.Record.html#method.to_undo_string
    #[inline]
    pub fn to_undo_string(&self) -> Option<String> {
        self.get().and_then(|record| record.to_undo_string())
    }

    /// Calls the [`to_redo_string`] method on the active record.
    ///
    /// [`to_redo_string`]: record/struct.Record.html#method.to_redo_string
    #[inline]
    pub fn to_redo_string(&self) -> Option<String> {
        self.get().and_then(|record| record.to_redo_string())
    }
}

impl<K: Hash + Eq + Debug, V: Debug, S: BuildHasher> Debug for Group<K, V, S> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Group")
            .field("group", &self.group)
            .field("active", &self.active)
            .finish()
    }
}

impl<K: Hash + Eq + Display, V: Display, S: BuildHasher> Display for Group<K, V, S> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        for (key, item) in &self.group {
            if self.active.as_ref().map_or(false, |a| a == key) {
                writeln!(f, "* {}:\n{}", key, item)?;
            } else {
                writeln!(f, "  {}:\n{}", key, item)?;
            }
        }
        Ok(())
    }
}

impl<K: Hash + Eq, V> Default for Group<K, V, RandomState> {
    #[inline]
    fn default() -> Group<K, V, RandomState> {
        Group {
            group: HashMap::new(),
            active: None,
            signals: None,
        }
    }
}

/// Builder for a group.
pub struct GroupBuilder<K: Hash + Eq, V, S: BuildHasher> {
    group: PhantomData<(K, V, S)>,
    capacity: usize,
    signals: Option<Box<FnMut(Option<&K>) + Send + Sync + 'static>>,
}

impl<K: Hash + Eq, V> GroupBuilder<K, V, RandomState> {
    /// Creates the group.
    #[inline]
    pub fn build(self) -> Group<K, V, RandomState> {
        self.build_with_hasher(Default::default())
    }
}

impl<K: Hash + Eq, V, S: BuildHasher> GroupBuilder<K, V, S> {
    /// Sets the specified [capacity] for the group.
    ///
    /// [capacity]: https://doc.rust-lang.org/std/vec/struct.Vec.html#capacity-and-reallocation
    #[inline]
    pub fn capacity(mut self, capacity: usize) -> GroupBuilder<K, V, S> {
        self.capacity = capacity;
        self
    }

    /// Decides what should happen when the active stack changes.
    #[inline]
    pub fn signals<F>(mut self, f: F) -> GroupBuilder<K, V, S>
        where
            F: FnMut(Option<&K>) + Send + Sync + 'static
    {
        self.signals = Some(Box::new(f));
        self
    }

    /// Creates the group with the given hasher.
    #[inline]
    pub fn build_with_hasher(self, hasher: S) -> Group<K, V, S> {
        Group {
            group: HashMap::with_capacity_and_hasher(self.capacity, hasher),
            active: None,
            signals: self.signals,
        }
    }
}

impl<K: Hash + Eq, V, S: BuildHasher> Debug for GroupBuilder<K, V, S> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("GroupBuilder")
            .field("group", &self.group)
            .field("capacity", &self.capacity)
            .finish()
    }
}
