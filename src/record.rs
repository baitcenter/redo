use std::collections::vec_deque::{IntoIter, VecDeque};
use std::fmt::{self, Debug, Formatter};
use std::marker::PhantomData;
use {Command, Error};

/// A record of commands.
///
/// The `Record` works mostly like a `Stack`, but it stores the commands
/// instead of returning them when undoing. This means it can roll the
/// receivers state backwards and forwards by using the undo and redo methods.
/// In addition, the `Record` has an internal state that is either clean or dirty.
/// A clean state means that the `Record` does not have any `Command`s to redo,
/// while a dirty state means that it does. The user can give the `Record` a function
/// that is called each time the state changes by using the `config` constructor.
///
/// # Examples
/// ```
/// use std::error::Error;
/// use std::fmt::{self, Display, Formatter};
/// use redo::{Command, Record};
///
/// #[derive(Debug)]
/// struct StrErr(&'static str);
///
/// impl Display for StrErr {
///     fn fmt(&self, f: &mut Formatter) -> fmt::Result { f.write_str(self.0) }
/// }
///
/// impl Error for StrErr {
///     fn description(&self) -> &str { self.0 }
/// }
///
/// #[derive(Debug)]
/// struct Add(char);
///
/// impl From<char> for Add {
///     fn from(c: char) -> Add { Add(c) }
/// }
///
/// impl Command<String> for Add {
///     type Err = StrErr;
///
///     fn redo(&mut self, s: &mut String) -> Result<(), StrErr> {
///         s.push(self.0);
///         Ok(())
///     }
///
///     fn undo(&mut self, s: &mut String) -> Result<(), StrErr> {
///         self.0 = s.pop().ok_or(StrErr("`String` is unexpectedly empty"))?;
///         Ok(())
///     }
/// }
///
/// fn foo() -> Result<(), Box<Error>> {
///     let mut record = Record::<_, Add>::default();
///
///     record.push(Add('a'))?;
///     record.push(Add('b'))?;
///     record.push(Add('c'))?;
///
///     assert_eq!(record.as_receiver(), "abc");
///
///     record.undo().unwrap()?;
///     record.undo().unwrap()?;
///     record.undo().unwrap()?;
///
///     assert_eq!(record.as_receiver(), "");
///
///     record.redo().unwrap()?;
///     record.redo().unwrap()?;
///     record.redo().unwrap()?;
///
///     assert_eq!(record.into_receiver(), "abc");
///
///     Ok(())
/// }
/// # foo().unwrap();
/// ```
pub struct Record<'a, R, C: Command<R>> {
    commands: VecDeque<C>,
    receiver: R,
    idx: usize,
    limit: Option<usize>,
    state_handle: Option<Box<FnMut(bool) + Send + Sync + 'a>>,
}

impl<'a, R, C: Command<R>> Record<'a, R, C> {
    /// Returns a new `Record`.
    #[inline]
    pub fn new<T: Into<R>>(receiver: T) -> Record<'a, R, C> {
        Record {
            commands: VecDeque::new(),
            receiver: receiver.into(),
            idx: 0,
            limit: None,
            state_handle: None,
        }
    }

    /// Returns a configurator for a `Record`.
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # use std::fmt::{self, Display, Formatter};
    /// # use redo::{Command, Record};
    /// # #[derive(Debug)]
    /// # struct StrErr(&'static str);
    /// # impl Display for StrErr {
    /// #     fn fmt(&self, f: &mut Formatter) -> fmt::Result { write!(f, "{}", self.0) }
    /// # }
    /// # impl Error for StrErr {
    /// #     fn description(&self) -> &str { self.0 }
    /// # }
    /// # #[derive(Debug)]
    /// # struct Add(char);
    /// # impl From<char> for Add {
    /// #   fn from(c: char) -> Add { Add(c) }
    /// # }
    /// # impl Command<String> for Add {
    /// #     type Err = StrErr;
    /// #     fn redo(&mut self, s: &mut String) -> Result<(), StrErr> {
    /// #         s.push(self.0);
    /// #         Ok(())
    /// #     }
    /// #     fn undo(&mut self, s: &mut String) -> Result<(), StrErr> {
    /// #         self.0 = s.pop().ok_or(StrErr("`String` is unexpectedly empty"))?;
    /// #         Ok(())
    /// #     }
    /// # }
    /// # fn foo() -> Result<(), Box<Error>> {
    /// let mut record = Record::<_, Add>::configure("")
    ///     .capacity(2)
    ///     .limit(2)
    ///     .create();
    ///
    /// record.push(Add('a'))?;
    /// record.push(Add('b'))?;
    /// record.push(Add('c'))?; // 'a' is removed from the record since limit is 2.
    ///
    /// assert_eq!(record.as_receiver(), "abc");
    ///
    /// record.undo().unwrap()?;
    /// record.undo().unwrap()?;
    /// assert!(record.undo().is_none());
    ///
    /// assert_eq!(record.into_receiver(), "a");
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn configure<T: Into<R>>(receiver: T) -> Config<'a, R, C> {
        Config {
            commands: PhantomData,
            receiver: receiver.into(),
            capacity: 0,
            limit: None,
            state_handle: None,
        }
    }

    /// Returns the limit of the `Record`, or `None` if it has no limit.
    #[inline]
    pub fn limit(&self) -> Option<usize> {
        self.limit
    }

    /// Returns the number of commands in the `Record`.
    #[inline]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Returns `true` if the `Record` is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Returns `true` if the state of the `Record` is clean, `false` otherwise.
    #[inline]
    pub fn is_clean(&self) -> bool {
        self.idx == self.len()
    }

    /// Returns `true` if the state of the `Record` is dirty, `false` otherwise.
    #[inline]
    pub fn is_dirty(&self) -> bool {
        !self.is_clean()
    }

    /// Returns a reference to the `receiver`.
    #[inline]
    pub fn as_receiver(&self) -> &R {
        &self.receiver
    }

    /// Consumes the `Record`, returning the `receiver`.
    #[inline]
    pub fn into_receiver(self) -> R {
        self.receiver
    }

    /// Pushes `cmd` on top of the `Record` and executes its [`redo`] method.
    /// The command is merged with the previous top command if [`merge`] does not return `None`.
    ///
    /// All commands above the active one are removed from the stack and returned as an iterator.
    ///
    /// # Errors
    /// If an error occur when executing [`redo`] or [merging commands][`merge`],
    /// the error is returned together with the command.
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # use std::fmt::{self, Display, Formatter};
    /// # use redo::{Command, Record};
    /// # #[derive(Debug)]
    /// # struct StrErr(&'static str);
    /// # impl Display for StrErr {
    /// #     fn fmt(&self, f: &mut Formatter) -> fmt::Result { write!(f, "{}", self.0) }
    /// # }
    /// # impl Error for StrErr {
    /// #     fn description(&self) -> &str { self.0 }
    /// # }
    /// # #[derive(Debug, Eq, PartialEq)]
    /// # struct Add(char);
    /// # impl From<char> for Add {
    /// #   fn from(c: char) -> Add { Add(c) }
    /// # }
    /// # impl Command<String> for Add {
    /// #     type Err = StrErr;
    /// #     fn redo(&mut self, s: &mut String) -> Result<(), StrErr> {
    /// #         s.push(self.0);
    /// #         Ok(())
    /// #     }
    /// #     fn undo(&mut self, s: &mut String) -> Result<(), StrErr> {
    /// #         self.0 = s.pop().ok_or(StrErr("`String` is unexpectedly empty"))?;
    /// #         Ok(())
    /// #     }
    /// # }
    /// # fn foo() -> Result<(), Box<Error>> {
    /// let mut record = Record::default();
    ///
    /// record.push(Add('a'))?;
    /// record.push(Add('b'))?;
    /// record.push(Add('c'))?;
    ///
    /// assert_eq!(record.as_receiver(), "abc");
    ///
    /// record.undo().unwrap()?;
    /// record.undo().unwrap()?;
    /// let mut bc = record.push(Add('e'))?;
    ///
    /// assert_eq!(record.into_receiver(), "ae");
    /// assert_eq!(bc.next(), Some(Add('b')));
    /// assert_eq!(bc.next(), Some(Add('c')));
    /// assert_eq!(bc.next(), None);
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    ///
    /// [`redo`]: ../trait.Command.html#tymethod.redo
    /// [`merge`]: ../trait.Command.html#method.merge
    #[inline]
    pub fn push(&mut self, mut cmd: C) -> Result<Commands<C>, Error<R, C>> {
        let is_dirty = self.is_dirty();
        let len = self.idx;
        match cmd.redo(&mut self.receiver) {
            Ok(_) => {
                // Pop off all elements after len from record.
                let iter = self.commands.split_off(len).into_iter();
                debug_assert_eq!(len, self.len());

                let cmd = match self.commands.back_mut() {
                    Some(last) => match last.merge(cmd) {
                        Ok(_) => None,
                        Err(cmd) => Some(cmd),
                    },
                    None => Some(cmd),
                };

                if let Some(cmd) = cmd {
                    match self.limit {
                        Some(limit) if len == limit => {
                            self.commands.pop_front();
                        }
                        _ => self.idx += 1,
                    }
                    self.commands.push_back(cmd);
                }

                debug_assert_eq!(self.idx, self.len());
                // Record is always clean after a push, check if it was dirty before.
                if is_dirty {
                    if let Some(ref mut f) = self.state_handle {
                        f(true);
                    }
                }
                Ok(Commands(iter))
            }
            Err(e) => Err(Error(cmd, e)),
        }
    }

    /// Calls the [`redo`] method for the active `Command` and sets the next one as the new
    /// active one.
    ///
    /// # Errors
    /// If an error occur when executing [`redo`] the
    /// error is returned and the state is left unchanged.
    ///
    /// [`redo`]: ../trait.Command.html#tymethod.redo
    #[inline]
    pub fn redo(&mut self) -> Option<Result<(), C::Err>> {
        if self.idx < self.len() {
            let is_dirty = self.is_dirty();
            match self.commands[self.idx].redo(&mut self.receiver) {
                Ok(_) => {
                    self.idx += 1;
                    // Check if record went from dirty to clean.
                    if is_dirty && self.is_clean() {
                        if let Some(ref mut f) = self.state_handle {
                            f(true);
                        }
                    }
                    Some(Ok(()))
                }
                Err(e) => Some(Err(e)),
            }
        } else {
            None
        }
    }

    /// Calls the [`undo`] method for the active `Command` and sets the previous one as the new
    /// active one.
    ///
    /// # Errors
    /// If an error occur when executing [`undo`] the
    /// error is returned and the state is left unchanged.
    ///
    /// [`undo`]: ../trait.Command.html#tymethod.undo
    #[inline]
    pub fn undo(&mut self) -> Option<Result<(), C::Err>> {
        if self.idx > 0 {
            let is_clean = self.is_clean();
            match self.commands[self.idx - 1].undo(&mut self.receiver) {
                Ok(_) => {
                    self.idx -= 1;
                    // Check if record went from clean to dirty.
                    if is_clean && self.is_dirty() {
                        if let Some(ref mut f) = self.state_handle {
                            f(false);
                        }
                    }
                    Some(Ok(()))
                }
                Err(e) => Some(Err(e)),
            }
        } else {
            None
        }
    }
}

impl<'a, R: Default, C: Command<R>> Default for Record<'a, R, C> {
    #[inline]
    fn default() -> Record<'a, R, C> {
        Record {
            commands: Default::default(),
            receiver: Default::default(),
            idx: 0,
            limit: None,
            state_handle: None,
        }
    }
}

impl<'a, R, C: Command<R>> AsRef<R> for Record<'a, R, C> {
    #[inline]
    fn as_ref(&self) -> &R {
        self.as_receiver()
    }
}

impl<'a, R: Debug, C: Command<R> + Debug> Debug for Record<'a, R, C> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Record")
            .field("commands", &self.commands)
            .field("receiver", &self.receiver)
            .field("idx", &self.idx)
            .field("limit", &self.limit)
            .finish()
    }
}

/// Iterator over `Command`s.
#[derive(Debug)]
pub struct Commands<C>(IntoIter<C>);

impl<C> Iterator for Commands<C> {
    type Item = C;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

/// Configurator for `Record`.
pub struct Config<'a, R, C: Command<R>> {
    commands: PhantomData<C>,
    receiver: R,
    capacity: usize,
    limit: Option<usize>,
    state_handle: Option<Box<FnMut(bool) + Send + Sync + 'a>>,
}

impl<'a, R, C: Command<R>> Config<'a, R, C> {
    /// Sets the `capacity` for the `Record`.
    #[inline]
    pub fn capacity(mut self, capacity: usize) -> Config<'a, R, C> {
        self.capacity = capacity;
        self
    }

    /// Sets the `limit` for the `Record`.
    #[inline]
    pub fn limit(mut self, limit: usize) -> Config<'a, R, C> {
        self.limit = if limit == 0 { None } else { Some(limit) };
        self
    }

    /// Sets what should happen when the state changes.
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # use std::fmt::{self, Display, Formatter};
    /// # use redo::{Command, Record};
    /// # #[derive(Debug)]
    /// # struct StrErr(&'static str);
    /// # impl Display for StrErr {
    /// #     fn fmt(&self, f: &mut Formatter) -> fmt::Result { write!(f, "{}", self.0) }
    /// # }
    /// # impl Error for StrErr {
    /// #     fn description(&self) -> &str { self.0 }
    /// # }
    /// # #[derive(Debug)]
    /// # struct Add(char);
    /// # impl From<char> for Add {
    /// #   fn from(c: char) -> Add { Add(c) }
    /// # }
    /// # impl Command<String> for Add {
    /// #     type Err = StrErr;
    /// #     fn redo(&mut self, s: &mut String) -> Result<(), StrErr> {
    /// #         s.push(self.0);
    /// #         Ok(())
    /// #     }
    /// #     fn undo(&mut self, s: &mut String) -> Result<(), StrErr> {
    /// #         self.0 = s.pop().ok_or(StrErr("`String` is unexpectedly empty"))?;
    /// #         Ok(())
    /// #     }
    /// # }
    /// # fn foo() -> Result<(), Box<Error>> {
    /// let mut x = 0;
    /// Record::<_, Add>::configure("")
    ///     .state_handle(|is_clean| {
    ///         if is_clean {
    ///             x = 1;
    ///         } else {
    ///             x = 2;
    ///         }
    ///     })
    ///     .create();
    /// # Ok(())
    /// # }
    /// # foo().unwrap();
    /// ```
    #[inline]
    pub fn state_handle<F>(mut self, f: F) -> Config<'a, R, C>
    where
        F: FnMut(bool) + Send + Sync + 'a,
    {
        self.state_handle = Some(Box::new(f));
        self
    }

    /// Creates the `Record`.
    #[inline]
    pub fn create(self) -> Record<'a, R, C> {
        Record {
            commands: VecDeque::with_capacity(self.capacity),
            receiver: self.receiver,
            idx: 0,
            limit: self.limit,
            state_handle: self.state_handle,
        }
    }
}

impl<'a, R: Debug, C: Command<R> + Debug> Debug for Config<'a, R, C> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Config")
            .field("receiver", &self.receiver)
            .field("capacity", &self.capacity)
            .field("limit", &self.limit)
            .finish()
    }
}
