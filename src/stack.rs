use std::fmt;
#[cfg(feature = "no_state")] use std::marker::PhantomData;
use {Result, RedoCmd};

/// Maintains a stack of `RedoCmd`s.
///
/// `RedoStack` uses static dispatch so it can only hold one type of command at a given time.
///
/// It will notice when it's state changes to either dirty or clean, and call the user
/// defined methods set in [on_clean] and [on_dirty]. This is useful if you want to trigger some
/// event when the state changes, eg. enabling and disabling buttons in an ui.
///
/// The `PopCmd` given in the examples below is defined as:
///
/// ```
/// # use redo::{self, RedoCmd};
/// #[derive(Clone, Copy)]
/// struct PopCmd {
///     vec: *mut Vec<i32>,
///     e: Option<i32>,
/// }
///
/// impl RedoCmd for PopCmd {
///     type Err = ();
///
///     fn redo(&mut self) -> redo::Result<()> {
///         self.e = unsafe {
///             let ref mut vec = *self.vec;
///             vec.pop()
///         };
///         Ok(())
///     }
///
///     fn undo(&mut self) -> redo::Result<()> {
///         unsafe {
///             let ref mut vec = *self.vec;
///             let e = self.e.ok_or(())?;
///             vec.push(e);
///         }
///         Ok(())
///     }
/// }
/// ```
///
/// [on_clean]: struct.RedoStack.html#method.on_clean
/// [on_dirty]: struct.RedoStack.html#method.on_dirty
#[derive(Default)]
pub struct RedoStack<'a, T> {
    // All commands on the stack.
    stack: Vec<T>,
    // Current position in the stack.
    idx: usize,
    // Max amount of commands allowed on the stack.
    limit: Option<usize>,
    // Called when the state changes from dirty to clean.
    #[cfg(not(feature = "no_state"))]
    on_clean: Option<Box<FnMut() + 'a>>,
    // Called when the state changes from clean to dirty.
    #[cfg(not(feature = "no_state"))]
    on_dirty: Option<Box<FnMut() + 'a>>,
    // Treat it the same when not using state.
    #[cfg(feature = "no_state")]
    phantom: PhantomData<FnMut() + 'a>
}

impl<'a, T> RedoStack<'a, T> {
    /// Creates a new `RedoStack`.
    ///
    /// # Examples
    /// ```
    /// # use redo::{self, RedoCmd, RedoStack};
    /// # struct A(u8);
    /// # impl RedoCmd for A {
    /// #   type Err = ();
    /// #   fn redo(&mut self) -> redo::Result<()> { Ok(()) }
    /// #   fn undo(&mut self) -> redo::Result<()> { Ok(()) }
    /// # }
    /// let mut stack = RedoStack::new();
    /// # stack.push(A(1)).unwrap();
    /// ```
    #[inline]
    pub fn new() -> Self {
        #[cfg(not(feature = "no_state"))]
        {
            RedoStack {
                stack: Vec::new(),
                idx: 0,
                limit: None,
                on_clean: None,
                on_dirty: None
            }
        }

        #[cfg(feature = "no_state")]
        {
            RedoStack {
                stack: Vec::new(),
                idx: 0,
                limit: None,
                phantom: PhantomData
            }
        }
    }

    /// Creates a new `RedoStack` with a limit on how many `RedoCmd`s can be stored in the stack.
    /// If this limit is reached it will start popping of commands at the bottom of the stack when
    /// pushing new commands on to the stack. No limit is set by default which means it may grow
    /// indefinitely.
    ///
    /// The stack may remove multiple commands at a time to increase performance.
    ///
    /// # Panics
    /// Panics if `limit` is `0`.
    ///
    /// # Examples
    /// ```
    /// # use redo::{self, RedoCmd, RedoStack};
    /// # #[derive(Clone, Copy)]
    /// # struct PopCmd {
    /// #   vec: *mut Vec<i32>,
    /// #   e: Option<i32>,
    /// # }
    /// # impl RedoCmd for PopCmd {
    /// #   type Err = ();
    /// #   fn redo(&mut self) -> redo::Result<()> {
    /// #       self.e = unsafe {
    /// #           let ref mut vec = *self.vec;
    /// #           vec.pop()
    /// #       };
    /// #       Ok(())
    /// #   }
    /// #   fn undo(&mut self) -> redo::Result<()> {
    /// #       unsafe {
    /// #           let ref mut vec = *self.vec;
    /// #           let e = self.e.ok_or(())?;
    /// #           vec.push(e);
    /// #       }
    /// #       Ok(())
    /// #   }
    /// # }
    /// # fn foo() -> redo::Result<()> {
    /// let mut vec = vec![1, 2, 3];
    /// let mut stack = RedoStack::with_limit(2);
    /// let cmd = PopCmd { vec: &mut vec, e: None };
    ///
    /// stack.push(cmd)?;
    /// stack.push(cmd)?;
    /// stack.push(cmd)?; // Pops off the first cmd.
    ///
    /// assert!(vec.is_empty());
    ///
    /// stack.undo()?;
    /// stack.undo()?;
    /// stack.undo()?; // Does nothing.
    ///
    /// assert_eq!(vec, vec![1, 2]);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn with_limit(limit: usize) -> Self {
        assert_ne!(limit, 0);

        #[cfg(not(feature = "no_state"))]
        {
            RedoStack {
                stack: Vec::new(),
                idx: 0,
                limit: Some(limit),
                on_clean: None,
                on_dirty: None
            }
        }

        #[cfg(feature = "no_state")]
        {
            RedoStack {
                stack: Vec::new(),
                idx: 0,
                limit: Some(limit),
                phantom: PhantomData
            }
        }
    }

    /// Creates a new `RedoStack` with the specified capacity.
    ///
    /// # Examples
    /// ```
    /// # use redo::{self, RedoCmd, RedoStack};
    /// # struct A(u8);
    /// # impl RedoCmd for A {
    /// #   type Err = ();
    /// #   fn redo(&mut self) -> redo::Result<()> { Ok(()) }
    /// #   fn undo(&mut self) -> redo::Result<()> { Ok(()) }
    /// # }
    /// let mut stack = RedoStack::with_capacity(10);
    /// assert_eq!(stack.capacity(), 10);
    /// # stack.push(A(0)).unwrap();
    /// ```
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        #[cfg(not(feature = "no_state"))]
        {
            RedoStack {
                stack: Vec::with_capacity(capacity),
                idx: 0,
                limit: None,
                on_clean: None,
                on_dirty: None
            }
        }

        #[cfg(feature = "no_state")]
        {
            RedoStack {
                stack: Vec::with_capacity(capacity),
                idx: 0,
                limit: None,
                phantom: PhantomData
            }
        }
    }

    /// Creates a new `RedoStack` with the specified capacity and limit.
    ///
    /// # Panics
    /// Panics if `limit` is `0`.
    ///
    /// # Examples
    /// ```
    /// # use redo::{self, RedoCmd, RedoStack};
    /// # struct A(u8);
    /// # impl RedoCmd for A {
    /// #   type Err = ();
    /// #   fn redo(&mut self) -> redo::Result<()> { Ok(()) }
    /// #   fn undo(&mut self) -> redo::Result<()> { Ok(()) }
    /// # }
    /// let mut stack = RedoStack::with_capacity_and_limit(10, 10);
    /// assert_eq!(stack.capacity(), 10);
    /// assert_eq!(stack.limit(), Some(10));
    /// # stack.push(A(0)).unwrap();
    /// ```
    #[inline]
    pub fn with_capacity_and_limit(capacity: usize, limit: usize) -> Self {
        assert_ne!(limit, 0);

        #[cfg(not(feature = "no_state"))]
        {
            RedoStack {
                stack: Vec::with_capacity(capacity),
                idx: 0,
                limit: Some(limit),
                on_clean: None,
                on_dirty: None
            }
        }

        #[cfg(feature = "no_state")]
        {
            RedoStack {
                stack: Vec::with_capacity(capacity),
                idx: 0,
                limit: Some(limit),
                phantom: PhantomData
            }
        }
    }

    /// Returns the limit of the `RedoStack`, or `None` if it has no limit.
    ///
    /// # Examples
    /// ```rust
    /// # use redo::{self, RedoCmd, RedoStack};
    /// # struct A(u8);
    /// # impl RedoCmd for A {
    /// #   type Err = ();
    /// #   fn redo(&mut self) -> redo::Result<()> { Ok(()) }
    /// #   fn undo(&mut self) -> redo::Result<()> { Ok(()) }
    /// # }
    /// # fn foo() -> redo::Result<()> {
    /// let mut stack = RedoStack::with_limit(10);
    /// assert_eq!(stack.limit(), Some(10));
    /// # stack.push(A(0))?;
    ///
    /// let mut stack = RedoStack::new();
    /// assert_eq!(stack.limit(), None);
    /// # stack.push(A(0))?;
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn limit(&self) -> Option<usize> {
        self.limit
    }

    /// Returns the capacity of the `RedoStack`.
    ///
    /// # Examples
    /// ```
    /// # use redo::{self, RedoCmd, RedoStack};
    /// # struct A(u8);
    /// # impl RedoCmd for A {
    /// #   type Err = ();
    /// #   fn redo(&mut self) -> redo::Result<()> { Ok(()) }
    /// #   fn undo(&mut self) -> redo::Result<()> { Ok(()) }
    /// # }
    /// let mut stack = RedoStack::with_capacity(10);
    /// assert_eq!(stack.capacity(), 10);
    /// # stack.push(A(0)).unwrap();
    /// ```
    #[inline]
    pub fn capacity(&self) -> usize {
        self.stack.capacity()
    }

    /// Reserves capacity for at least `additional` more commands to be inserted in the given stack.
    /// The stack may reserve more space to avoid frequent reallocations.
    ///
    /// # Panics
    /// Panics if the new capacity overflows usize.
    ///
    /// # Examples
    /// ```
    /// # use redo::{self, RedoCmd, RedoStack};
    /// # #[derive(Clone, Copy)]
    /// # struct PopCmd {
    /// #     vec: *mut Vec<i32>,
    /// #     e: Option<i32>,
    /// # }
    /// # impl RedoCmd for PopCmd {
    /// #     type Err = ();
    /// #     fn redo(&mut self) -> redo::Result<()> {
    /// #         self.e = unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             vec.pop()
    /// #         };
    /// #         Ok(())
    /// #     }
    /// #     fn undo(&mut self) -> redo::Result<()> {
    /// #         unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             let e = self.e.ok_or(())?;
    /// #             vec.push(e);
    /// #         }
    /// #         Ok(())
    /// #     }
    /// # }
    /// # fn foo() -> redo::Result<()> {
    /// let mut vec = vec![1, 2, 3];
    /// let mut stack = RedoStack::new();
    /// let cmd = PopCmd { vec: &mut vec, e: None };
    ///
    /// stack.push(cmd)?;
    /// stack.reserve(10);
    /// assert!(stack.capacity() >= 11);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.stack.reserve(additional);
    }

    /// Shrinks the capacity of the `RedoStack` as much as possible.
    ///
    /// # Examples
    /// ```
    /// # use redo::{self, RedoCmd, RedoStack};
    /// # #[derive(Clone, Copy)]
    /// # struct PopCmd {
    /// #     vec: *mut Vec<i32>,
    /// #     e: Option<i32>,
    /// # }
    /// # impl RedoCmd for PopCmd {
    /// #     type Err = ();
    /// #     fn redo(&mut self) -> redo::Result<()> {
    /// #         self.e = unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             vec.pop()
    /// #         };
    /// #         Ok(())
    /// #     }
    /// #     fn undo(&mut self) -> redo::Result<()> {
    /// #         unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             let e = self.e.ok_or(())?;
    /// #             vec.push(e);
    /// #         }
    /// #         Ok(())
    /// #     }
    /// # }
    /// # fn foo() -> redo::Result<()> {
    /// let mut vec = vec![1, 2, 3];
    /// let mut stack = RedoStack::with_capacity(10);
    /// let cmd = PopCmd { vec: &mut vec, e: None };
    ///
    /// stack.push(cmd)?;
    /// stack.push(cmd)?;
    /// stack.push(cmd)?;
    ///
    /// assert_eq!(stack.capacity(), 10);
    /// stack.shrink_to_fit();
    /// assert!(stack.capacity() >= 3);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.stack.shrink_to_fit();
    }

    /// Sets what should happen if the state changes from dirty to clean.
    /// By default the `RedoStack` does nothing when the state changes.
    ///
    /// Note: An empty stack is clean, so the first push will not trigger this method.
    ///
    /// # Examples
    /// ```
    /// # use std::cell::Cell;
    /// # use redo::{self, RedoCmd, RedoStack};
    /// # #[derive(Clone, Copy)]
    /// # struct PopCmd {
    /// #     vec: *mut Vec<i32>,
    /// #     e: Option<i32>,
    /// # }
    /// # impl RedoCmd for PopCmd {
    /// #     type Err = ();
    /// #     fn redo(&mut self) -> redo::Result<()> {
    /// #         self.e = unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             vec.pop()
    /// #         };
    /// #         Ok(())
    /// #     }
    /// #     fn undo(&mut self) -> redo::Result<()> {
    /// #         unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             let e = self.e.ok_or(())?;
    /// #             vec.push(e);
    /// #         }
    /// #         Ok(())
    /// #     }
    /// # }
    /// # fn foo() -> redo::Result<()> {
    /// let mut vec = vec![1, 2, 3];
    /// let x = Cell::new(0);
    /// let mut stack = RedoStack::new();
    /// stack.on_clean(|| x.set(1));
    /// let cmd = PopCmd { vec: &mut vec, e: None };
    ///
    /// stack.push(cmd)?;
    /// stack.undo()?;
    /// assert_eq!(x.get(), 0);
    /// stack.redo()?;
    /// assert_eq!(x.get(), 1);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[cfg(not(feature = "no_state"))]
    #[inline]
    pub fn on_clean<F>(&mut self, f: F)
        where F: FnMut() + 'a
    {
        self.on_clean = Some(Box::new(f));
    }

    /// Sets what should happen if the state changes from clean to dirty.
    /// By default the `RedoStack` does nothing when the state changes.
    ///
    /// # Examples
    /// ```
    /// # use std::cell::Cell;
    /// # use redo::{self, RedoCmd, RedoStack};
    /// # #[derive(Clone, Copy)]
    /// # struct PopCmd {
    /// #     vec: *mut Vec<i32>,
    /// #     e: Option<i32>,
    /// # }
    /// # impl RedoCmd for PopCmd {
    /// #     type Err = ();
    /// #     fn redo(&mut self) -> redo::Result<()> {
    /// #         self.e = unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             vec.pop()
    /// #         };
    /// #         Ok(())
    /// #     }
    /// #     fn undo(&mut self) -> redo::Result<()> {
    /// #         unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             let e = self.e.ok_or(())?;
    /// #             vec.push(e);
    /// #         }
    /// #         Ok(())
    /// #     }
    /// # }
    /// # fn foo() -> redo::Result<()> {
    /// let mut vec = vec![1, 2, 3];
    /// let x = Cell::new(0);
    /// let mut stack = RedoStack::new();
    /// stack.on_dirty(|| x.set(1));
    /// let cmd = PopCmd { vec: &mut vec, e: None };
    ///
    /// stack.push(cmd)?;
    /// assert_eq!(x.get(), 0);
    /// stack.undo()?;
    /// assert_eq!(x.get(), 1);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[cfg(not(feature = "no_state"))]
    #[inline]
    pub fn on_dirty<F>(&mut self, f: F)
        where F: FnMut() + 'a
    {
        self.on_dirty = Some(Box::new(f));
    }

    /// Returns `true` if the state of the stack is clean, `false` otherwise.
    ///
    /// # Examples
    /// ```
    /// # use redo::{self, RedoCmd, RedoStack};
    /// # #[derive(Clone, Copy)]
    /// # struct PopCmd {
    /// #     vec: *mut Vec<i32>,
    /// #     e: Option<i32>,
    /// # }
    /// # impl RedoCmd for PopCmd {
    /// #     type Err = ();
    /// #     fn redo(&mut self) -> redo::Result<()> {
    /// #         self.e = unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             vec.pop()
    /// #         };
    /// #         Ok(())
    /// #     }
    /// #     fn undo(&mut self) -> redo::Result<()> {
    /// #         unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             let e = self.e.ok_or(())?;
    /// #             vec.push(e);
    /// #         }
    /// #         Ok(())
    /// #     }
    /// # }
    /// # fn foo() -> redo::Result<()> {
    /// let mut vec = vec![1, 2, 3];
    /// let mut stack = RedoStack::new();
    /// let cmd = PopCmd { vec: &mut vec, e: None };
    ///
    /// assert!(stack.is_clean()); // An empty stack is always clean.
    /// stack.push(cmd)?;
    /// assert!(stack.is_clean());
    /// stack.undo()?;
    /// assert!(!stack.is_clean());
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[cfg(not(feature = "no_state"))]
    #[inline]
    pub fn is_clean(&self) -> bool {
        self.idx == self.stack.len()
    }

    /// Returns `true` if the state of the stack is dirty, `false` otherwise.
    ///
    /// # Examples
    /// ```
    /// # use redo::{self, RedoCmd, RedoStack};
    /// # #[derive(Clone, Copy)]
    /// # struct PopCmd {
    /// #     vec: *mut Vec<i32>,
    /// #     e: Option<i32>,
    /// # }
    /// # impl RedoCmd for PopCmd {
    /// #     type Err = ();
    /// #     fn redo(&mut self) -> redo::Result<()> {
    /// #         self.e = unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             vec.pop()
    /// #         };
    /// #         Ok(())
    /// #     }
    /// #     fn undo(&mut self) -> redo::Result<()> {
    /// #         unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             let e = self.e.ok_or(())?;
    /// #             vec.push(e);
    /// #         }
    /// #         Ok(())
    /// #     }
    /// # }
    /// # fn foo() -> redo::Result<()> {
    /// let mut vec = vec![1, 2, 3];
    /// let mut stack = RedoStack::new();
    /// let cmd = PopCmd { vec: &mut vec, e: None };
    ///
    /// assert!(!stack.is_dirty()); // An empty stack is always clean.
    /// stack.push(cmd)?;
    /// assert!(!stack.is_dirty());
    /// stack.undo()?;
    /// assert!(stack.is_dirty());
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[cfg(not(feature = "no_state"))]
    #[inline]
    pub fn is_dirty(&self) -> bool {
        !self.is_clean()
    }
}

impl<'a, T: RedoCmd> RedoStack<'a, T> {
    /// Pushes `cmd` to the top of the stack and executes its [`redo`] method.
    /// This pops off all other commands above the active command from the stack.
    ///
    /// # Examples
    /// ```
    /// # use redo::{self, RedoCmd, RedoStack};
    /// # #[derive(Clone, Copy)]
    /// # struct PopCmd {
    /// #     vec: *mut Vec<i32>,
    /// #     e: Option<i32>,
    /// # }
    /// # impl RedoCmd for PopCmd {
    /// #     type Err = ();
    /// #     fn redo(&mut self) -> redo::Result<()> {
    /// #         self.e = unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             vec.pop()
    /// #         };
    /// #         Ok(())
    /// #     }
    /// #     fn undo(&mut self) -> redo::Result<()> {
    /// #         unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             let e = self.e.ok_or(())?;
    /// #             vec.push(e);
    /// #         }
    /// #         Ok(())
    /// #     }
    /// # }
    /// # fn foo() -> redo::Result<()> {
    /// let mut vec = vec![1, 2, 3];
    /// let mut stack = RedoStack::new();
    /// let cmd = PopCmd { vec: &mut vec, e: None };
    ///
    /// stack.push(cmd)?;
    /// stack.push(cmd)?;
    /// stack.push(cmd)?;
    ///
    /// assert!(vec.is_empty());
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    ///
    /// [`redo`]: trait.RedoCmd.html#tymethod.redo
    pub fn push(&mut self, mut cmd: T) -> Result<T::Err> {
        #[cfg(not(feature = "no_state"))]
        let is_dirty = self.is_dirty();
        let len = self.idx;
        // Pop off all elements after len from stack.
        self.stack.truncate(len);
        cmd.redo()?;

        match self.stack.last_mut().and_then(|last| last.merge(&cmd)) {
            Some(x) => x?,
            None => {
                match self.limit {
                    Some(limit) if len == limit => {
                        // Remove ~25% of the stack at once.
                        let x = len / 4 + 1;
                        self.stack.drain(..x);
                        self.idx -= x - 1;
                    }
                    _ => self.idx += 1
                }
                self.stack.push(cmd);
            }
        }

        debug_assert_eq!(self.idx, self.stack.len());
        #[cfg(not(feature = "no_state"))]
        {
            // State is always clean after a push, check if it was dirty before.
            if is_dirty {
                if let Some(ref mut f) = self.on_clean {
                    f();
                }
            }
        }
        Ok(())
    }

    /// Calls the [`redo`] method for the active `RedoCmd` and sets the next `RedoCmd` as the new
    /// active one.
    ///
    /// # Examples
    /// ```
    /// # use redo::{self, RedoCmd, RedoStack};
    /// # #[derive(Clone, Copy)]
    /// # struct PopCmd {
    /// #     vec: *mut Vec<i32>,
    /// #     e: Option<i32>,
    /// # }
    /// # impl RedoCmd for PopCmd {
    /// #     type Err = ();
    /// #     fn redo(&mut self) -> redo::Result<()> {
    /// #         self.e = unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             vec.pop()
    /// #         };
    /// #         Ok(())
    /// #     }
    /// #     fn undo(&mut self) -> redo::Result<()> {
    /// #         unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             let e = self.e.ok_or(())?;
    /// #             vec.push(e);
    /// #         }
    /// #         Ok(())
    /// #     }
    /// # }
    /// # fn foo() -> redo::Result<()> {
    /// let mut vec = vec![1, 2, 3];
    /// let mut stack = RedoStack::new();
    /// let cmd = PopCmd { vec: &mut vec, e: None };
    ///
    /// stack.push(cmd)?;
    /// stack.push(cmd)?;
    /// stack.push(cmd)?;
    ///
    /// assert!(vec.is_empty());
    ///
    /// stack.undo()?;
    /// stack.undo()?;
    /// stack.undo()?;
    ///
    /// assert_eq!(vec, vec![1, 2, 3]);
    ///
    /// stack.redo()?;
    /// stack.redo()?;
    /// stack.redo()?;
    ///
    /// assert!(vec.is_empty());
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    ///
    /// [`redo`]: trait.RedoCmd.html#tymethod.redo
    #[inline]
    pub fn redo(&mut self) -> Result<T::Err> {
        if self.idx < self.stack.len() {
            #[cfg(not(feature = "no_state"))]
            let is_dirty = self.is_dirty();
            unsafe {
                let cmd = self.stack.get_unchecked_mut(self.idx);
                cmd.redo()?;
            }
            self.idx += 1;
            #[cfg(not(feature = "no_state"))]
            {
                // Check if stack went from dirty to clean.
                if is_dirty && self.is_clean() {
                    if let Some(ref mut f) = self.on_clean {
                        f();
                    }
                }
            }
        }
        Ok(())
    }

    /// Calls the [`undo`] method for the active `RedoCmd` and sets the previous `RedoCmd` as the
    /// new active one.
    ///
    /// # Examples
    /// ```
    /// # use redo::{self, RedoCmd, RedoStack};
    /// # #[derive(Clone, Copy)]
    /// # struct PopCmd {
    /// #     vec: *mut Vec<i32>,
    /// #     e: Option<i32>,
    /// # }
    /// # impl RedoCmd for PopCmd {
    /// #     type Err = ();
    /// #     fn redo(&mut self) -> redo::Result<()> {
    /// #         self.e = unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             vec.pop()
    /// #         };
    /// #         Ok(())
    /// #     }
    /// #     fn undo(&mut self) -> redo::Result<()> {
    /// #         unsafe {
    /// #             let ref mut vec = *self.vec;
    /// #             let e = self.e.ok_or(())?;
    /// #             vec.push(e);
    /// #         }
    /// #         Ok(())
    /// #     }
    /// # }
    /// # fn foo() -> redo::Result<()> {
    /// let mut vec = vec![1, 2, 3];
    /// let mut stack = RedoStack::new();
    /// let cmd = PopCmd { vec: &mut vec, e: None };
    ///
    /// stack.push(cmd)?;
    /// stack.push(cmd)?;
    /// stack.push(cmd)?;
    ///
    /// assert!(vec.is_empty());
    ///
    /// stack.undo()?;
    /// stack.undo()?;
    /// stack.undo()?;
    ///
    /// assert_eq!(vec, vec![1, 2, 3]);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    ///
    /// [`undo`]: trait.RedoCmd.html#tymethod.undo
    #[inline]
    pub fn undo(&mut self) -> Result<T::Err> {
        if self.idx > 0 {
            #[cfg(not(feature = "no_state"))]
            let is_clean = self.is_clean();
            self.idx -= 1;
            debug_assert!(self.idx < self.stack.len());
            unsafe {
                let cmd = self.stack.get_unchecked_mut(self.idx);
                cmd.undo()?;
            }
            #[cfg(not(feature = "no_state"))]
            {
                // Check if stack went from clean to dirty.
                if is_clean && self.is_dirty() {
                    if let Some(ref mut f) = self.on_dirty {
                        f();
                    }
                }
            }
        }
        Ok(())
    }
}

impl<'a, T: fmt::Debug> fmt::Debug for RedoStack<'a, T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("RedoStack")
            .field("stack", &self.stack)
            .field("idx", &self.idx)
            .field("limit", &self.limit)
            .finish()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Clone, Copy)]
    struct PopCmd {
        vec: *mut Vec<i32>,
        e: Option<i32>
    }

    impl RedoCmd for PopCmd {
        type Err = ();

        fn redo(&mut self) -> Result<()> {
            self.e = unsafe {
                let ref mut vec = *self.vec;
                vec.pop()
            };
            Ok(())
        }

        fn undo(&mut self) -> Result<()> {
            unsafe {
                let ref mut vec = *self.vec;
                let e = self.e.ok_or(())?;
                vec.push(e);
            }
            Ok(())
        }
    }

    #[cfg(not(feature = "no_state"))]
    #[test]
    fn state() {
        use std::cell::Cell;

        let x = Cell::new(0);
        let mut vec = vec![1, 2, 3];
        let mut stack = RedoStack::new();
        stack.on_clean(|| x.set(0));
        stack.on_dirty(|| x.set(1));

        let cmd = PopCmd { vec: &mut vec, e: None };
        for _ in 0..3 {
            stack.push(cmd).unwrap();
        }
        assert_eq!(x.get(), 0);
        assert!(vec.is_empty());

        for _ in 0..3 {
            stack.undo().unwrap();
        }
        assert_eq!(x.get(), 1);
        assert_eq!(vec, vec![1, 2, 3]);

        stack.push(cmd).unwrap();
        assert_eq!(x.get(), 0);
        assert_eq!(vec, vec![1, 2]);

        stack.undo().unwrap();
        assert_eq!(x.get(), 1);
        assert_eq!(vec, vec![1, 2, 3]);

        stack.redo().unwrap();
        assert_eq!(x.get(), 0);
        assert_eq!(vec, vec![1, 2]);
    }

    #[test]
    fn limit() {
        let mut vec = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let mut stack = RedoStack::with_limit(9);

        let cmd = PopCmd { vec: &mut vec, e: None };

        for _ in 0..10 {
            stack.push(cmd).unwrap();
        }

        assert!(vec.is_empty());
        assert_eq!(stack.stack.len(), 7);
    }
}
