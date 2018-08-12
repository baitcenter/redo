use std::collections::VecDeque;
use std::fmt::{self, Debug, Display, Formatter};
use std::marker::PhantomData;
use {Command, Error, Signal};

/// A record of commands.
///
/// The record can roll the receivers state backwards and forwards by using
/// the undo and redo methods. In addition, the record can notify the user
/// about changes to the stack or the receiver through [signal]. The user
/// can give the record a function that is called each time the state changes
/// by using the [`builder`].
///
/// # Examples
/// ```
/// # use std::error;
/// # use redo::*;
/// #[derive(Debug)]
/// struct Add(char);
///
/// impl Command<String> for Add {
///     type Error = Box<dyn error::Error>;
///
///     fn apply(&mut self, s: &mut String) -> Result<(), Self::Error> {
///         s.push(self.0);
///         Ok(())
///     }
///
///     fn undo(&mut self, s: &mut String) -> Result<(), Self::Error> {
///         self.0 = s.pop().ok_or("`s` is empty")?;
///         Ok(())
///     }
/// }
///
/// fn main() -> Result<(), Error<String, Add>> {
///     let mut record = Record::default();
///
///     record.apply(Add('a'))?;
///     record.apply(Add('b'))?;
///     record.apply(Add('c'))?;
///     assert_eq!(record.as_receiver(), "abc");
///
///     record.undo().unwrap()?;
///     record.undo().unwrap()?;
///     record.undo().unwrap()?;
///     assert_eq!(record.as_receiver(), "");
///
///     record.redo().unwrap()?;
///     record.redo().unwrap()?;
///     record.redo().unwrap()?;
///     assert_eq!(record.as_receiver(), "abc");
///     Ok(())
/// }
/// ```
///
/// [`builder`]: struct.RecordBuilder.html
/// [signal]: enum.Signal.html
pub struct Record<R, C: Command<R>> {
    pub(crate) commands: VecDeque<C>,
    receiver: R,
    cursor: usize,
    limit: usize,
    pub(crate) saved: Option<usize>,
    pub(crate) signal: Option<Box<dyn FnMut(Signal) + Send + Sync + 'static>>,
}

impl<R, C: Command<R>> Record<R, C> {
    /// Returns a new record.
    #[inline]
    pub fn new(receiver: impl Into<R>) -> Record<R, C> {
        Record {
            commands: VecDeque::new(),
            receiver: receiver.into(),
            cursor: 0,
            limit: usize::max_value(),
            saved: Some(0),
            signal: None,
        }
    }

    /// Returns a builder for a record.
    #[inline]
    pub fn builder() -> RecordBuilder<R, C> {
        RecordBuilder {
            commands: PhantomData,
            receiver: PhantomData,
            capacity: 0,
            limit: usize::max_value(),
            saved: true,
            signal: None,
        }
    }

    /// Reserves capacity for at least `additional` more commands.
    ///
    /// # Panics
    /// Panics if the new capacity overflows usize.
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.commands.reserve(additional);
    }

    /// Returns the capacity of the record.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.commands.capacity()
    }

    /// Returns the number of commands in the record.
    #[inline]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Returns `true` if the record is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Returns the limit of the record.
    #[inline]
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Sets the limit of the record and returns the new limit.
    ///
    /// If this limit is reached it will start popping of commands at the beginning
    /// of the record when new commands are applied. No limit is set by
    /// default which means it may grow indefinitely.
    ///
    /// If `0 < limit < len` the first commands will be removed until `len == limit`.
    /// However, if the current active command is going to be removed, the limit is instead
    /// adjusted to `len - active` so the active command is not removed.
    ///
    /// # Panics
    /// Panics if `limit` is `0`.
    #[inline]
    pub fn set_limit(&mut self, limit: usize) -> usize {
        assert_ne!(limit, 0);
        if limit < self.len() {
            let old = self.cursor;
            let could_undo = self.can_undo();
            let was_saved = self.is_saved();

            let begin = usize::min(self.cursor, self.len() - limit);
            self.commands = self.commands.split_off(begin);
            self.limit = self.len();
            self.cursor -= begin;

            // Check if the saved state has been removed.
            self.saved = self.saved.and_then(|saved| saved.checked_sub(begin));

            let new = self.cursor;
            let can_undo = self.can_undo();
            let is_saved = self.is_saved();
            if let Some(ref mut f) = self.signal {
                // Emit signal if the cursor has changed.
                if old != new {
                    f(Signal::Cursor { old, new });
                }
                // Check if the records ability to undo changed.
                if could_undo != can_undo {
                    f(Signal::Undo(can_undo));
                }
                // Check if the receiver went from saved to unsaved.
                if was_saved != is_saved {
                    f(Signal::Saved(is_saved));
                }
            }
        } else {
            self.limit = limit;
        }
        self.limit
    }

    /// Sets how the signal should be handled when the state changes.
    #[inline]
    pub fn set_signal(&mut self, f: impl FnMut(Signal) + Send + Sync + 'static) {
        self.signal = Some(Box::new(f));
    }

    /// Returns `true` if the record can undo.
    #[inline]
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    /// Returns `true` if the record can redo.
    #[inline]
    pub fn can_redo(&self) -> bool {
        self.cursor < self.len()
    }

    /// Marks the receiver as currently being in a saved or unsaved state.
    #[inline]
    pub fn set_saved(&mut self, saved: bool) {
        let was_saved = self.is_saved();
        if saved {
            self.saved = Some(self.cursor);
            if let Some(ref mut f) = self.signal {
                // Check if the receiver went from unsaved to saved.
                if !was_saved {
                    f(Signal::Saved(true));
                }
            }
        } else {
            self.saved = None;
            if let Some(ref mut f) = self.signal {
                // Check if the receiver went from saved to unsaved.
                if was_saved {
                    f(Signal::Saved(false));
                }
            }
        }
    }

    /// Returns `true` if the receiver is in a saved state, `false` otherwise.
    #[inline]
    pub fn is_saved(&self) -> bool {
        self.saved.map_or(false, |saved| saved == self.cursor)
    }

    /// Returns the position of the current command.
    #[inline]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Removes all commands from the record without undoing them.
    #[inline]
    pub fn clear(&mut self) {
        let old = self.cursor;
        let could_undo = self.can_undo();
        let could_redo = self.can_redo();

        self.commands.clear();
        self.saved = if self.is_saved() { Some(0) } else { None };
        self.cursor = 0;

        if let Some(ref mut f) = self.signal {
            // Emit signal if the cursor has changed.
            if old != 0 {
                f(Signal::Cursor { old, new: 0 });
            }
            // Record can never undo after being cleared, check if you could undo before.
            if could_undo {
                f(Signal::Undo(false));
            }
            // Record can never redo after being cleared, check if you could redo before.
            if could_redo {
                f(Signal::Redo(false));
            }
        }
    }

    /// Pushes the command on top of the record and executes its [`apply`] method.
    /// The command is merged with the previous top command if [`merge`] does not return `None`.
    ///
    /// All commands above the active one are removed and returned as an iterator.
    ///
    /// # Errors
    /// If an error occur when executing [`apply`] the error is returned together with the command.
    ///
    /// [`apply`]: trait.Command.html#tymethod.apply
    /// [`merge`]: trait.Command.html#method.merge
    #[inline]
    pub fn apply(&mut self, cmd: C) -> Result<impl Iterator<Item = C>, Error<R, C>> {
        self.__apply(cmd).map(|(_, v)| v.into_iter())
    }

    #[inline]
    pub(crate) fn __apply(&mut self, mut cmd: C) -> Result<(bool, VecDeque<C>), Error<R, C>> {
        if let Err(err) = cmd.apply(&mut self.receiver) {
            return Err(Error(cmd, err));
        }

        let old = self.cursor;
        let could_undo = self.can_undo();
        let could_redo = self.can_redo();
        let was_saved = self.is_saved();

        // Pop off all elements after len from record.
        let v = self.commands.split_off(self.cursor);
        debug_assert_eq!(self.cursor, self.len());

        // Check if the saved state was popped off.
        self.saved = self.saved.filter(|&saved| saved <= self.cursor);

        // Try to merge commands unless the receiver is in a saved state.
        let cmd = match self.commands.back_mut() {
            Some(ref mut last) if !was_saved => last.merge(cmd).err(),
            _ => Some(cmd),
        };
        let merged = cmd.is_none();
        // If commands are not merged push it onto the record.
        if let Some(cmd) = cmd {
            // If limit is reached, pop off the first command.
            if self.limit == self.cursor {
                self.commands.pop_front();
                self.saved = self.saved.and_then(|saved| saved.checked_sub(1));
            } else {
                self.cursor += 1;
            }
            self.commands.push_back(cmd);
        }

        debug_assert_eq!(self.cursor, self.len());
        if let Some(ref mut f) = self.signal {
            // We emit this signal even if the commands might have been merged.
            f(Signal::Cursor {
                old,
                new: self.cursor,
            });
            // Record can never redo after executing a command, check if you could redo before.
            if could_redo {
                f(Signal::Redo(false));
            }
            // Record can always undo after executing a command, check if you could not undo before.
            if !could_undo {
                f(Signal::Undo(true));
            }
            // Check if the receiver went from saved to unsaved.
            if was_saved {
                f(Signal::Saved(false));
            }
        }
        Ok((merged, v))
    }

    /// Calls the [`undo`] method for the active command and sets the previous one as the new active one.
    ///
    /// # Errors
    /// If an error occur when executing [`undo`] the error is returned together with the command.
    ///
    /// [`undo`]: ../trait.Command.html#tymethod.undo
    #[inline]
    #[must_use]
    pub fn undo(&mut self) -> Option<Result<(), Error<R, C>>> {
        if !self.can_undo() {
            return None;
        }

        if let Err(err) = self.commands[self.cursor - 1].undo(&mut self.receiver) {
            let cmd = self.commands.remove(self.cursor - 1).unwrap();
            return Some(Err(Error(cmd, err)));
        }

        let was_saved = self.is_saved();
        let old = self.cursor;
        self.cursor -= 1;
        let len = self.len();
        let is_saved = self.is_saved();
        if let Some(ref mut f) = self.signal {
            // Cursor has always changed at this point.
            f(Signal::Cursor {
                old,
                new: self.cursor,
            });
            // Check if the records ability to redo changed.
            if old == len {
                f(Signal::Redo(true));
            }
            // Check if the records ability to undo changed.
            if old == 1 {
                f(Signal::Undo(false));
            }
            // Check if the receiver went from saved to unsaved, or unsaved to saved.
            if was_saved != is_saved {
                f(Signal::Saved(is_saved));
            }
        }
        Some(Ok(()))
    }

    /// Calls the [`redo`] method for the active command and sets the next one as the new
    /// active one.
    ///
    /// # Errors
    /// If an error occur when applying [`redo`] the error is returned together with the command.
    ///
    /// [`redo`]: trait.Command.html#method.redo
    #[inline]
    #[must_use]
    pub fn redo(&mut self) -> Option<Result<(), Error<R, C>>> {
        if !self.can_redo() {
            return None;
        }

        if let Err(err) = self.commands[self.cursor].redo(&mut self.receiver) {
            let cmd = self.commands.remove(self.cursor).unwrap();
            return Some(Err(Error(cmd, err)));
        }

        let was_saved = self.is_saved();
        let old = self.cursor;
        self.cursor += 1;
        let len = self.len();
        let is_saved = self.is_saved();
        if let Some(ref mut f) = self.signal {
            // Cursor has always changed at this point.
            f(Signal::Cursor {
                old,
                new: self.cursor,
            });
            // Check if the records ability to redo changed.
            if old == len - 1 {
                f(Signal::Redo(false));
            }
            // Check if the records ability to undo changed.
            if old == 0 {
                f(Signal::Undo(true));
            }
            // Check if the receiver went from saved to unsaved, or unsaved to saved.
            if was_saved != is_saved {
                f(Signal::Saved(is_saved));
            }
        }
        Some(Ok(()))
    }

    /// Repeatedly calls [`undo`] or [`redo`] until the command at `cursor` is reached.
    ///
    /// # Errors
    /// If an error occur when executing [`undo`] or [`redo`] the error is returned together with the command.
    ///
    /// [`undo`]: trait.Command.html#tymethod.undo
    /// [`redo`]: trait.Command.html#method.redo
    #[inline]
    #[must_use]
    pub fn go_to(&mut self, cursor: usize) -> Option<Result<(), Error<R, C>>> {
        if cursor > self.len() {
            return None;
        }

        let was_saved = self.is_saved();
        let old = self.cursor;
        let len = self.len();
        // Temporarily remove signal so they are not called each iteration.
        let signal = self.signal.take();
        // Decide if we need to undo or redo to reach cursor.
        let redo = cursor > self.cursor;
        let f = if redo { Record::redo } else { Record::undo };
        while self.cursor != cursor {
            if let Err(err) = f(self).unwrap() {
                self.signal = signal;
                return Some(Err(err));
            }
        }
        // Add signal back.
        self.signal = signal;
        let is_saved = self.is_saved();
        if let Some(ref mut f) = self.signal {
            // Emit signal if the cursor has changed.
            if old != self.cursor {
                f(Signal::Cursor {
                    old,
                    new: self.cursor,
                });
            }
            // Check if the receiver went from saved to unsaved, or unsaved to saved.
            if was_saved != is_saved {
                f(Signal::Saved(is_saved));
            }
            if redo {
                // Check if the records ability to redo changed.
                if old == len - 1 {
                    f(Signal::Redo(false));
                }
                // Check if the records ability to undo changed.
                if old == 0 {
                    f(Signal::Undo(true));
                }
            } else {
                // Check if the records ability to redo changed.
                if old == len {
                    f(Signal::Redo(true));
                }
                // Check if the records ability to undo changed.
                if old == 1 {
                    f(Signal::Undo(false));
                }
            }
        }
        Some(Ok(()))
    }

    /// Jump directly to the command at `cursor` and executes its [`undo`] or [`redo`] method.
    ///
    /// This method can be used if the commands store the whole state of the receiver,
    /// and does not require the commands in between to be called to get the same result.
    /// Use [`go_to`] otherwise.
    ///
    /// # Errors
    /// If an error occur when executing [`undo`] or [`redo`] the error is returned together with the command.
    ///
    /// [`undo`]: trait.Command.html#tymethod.undo
    /// [`redo`]: trait.Command.html#method.redo
    /// [`go_to`]: struct.Record.html#method.go_to
    #[inline]
    #[must_use]
    pub fn jump_to(&mut self, cursor: usize) -> Option<Result<(), Error<R, C>>> {
        if cursor > self.len() {
            return None;
        }
        if cursor == self.cursor {
            return Some(Ok(()));
        }
        let was_saved = self.is_saved();
        let old = self.cursor;
        let len = self.len();
        // Temporarily remove signal so they are not called each iteration.
        let signal = self.signal.take();
        // Decide if we need to undo or redo to reach cursor.
        let redo = cursor > self.cursor;
        if redo {
            self.cursor = cursor - 1;
            if let Err(err) = self.redo().unwrap() {
                self.signal = signal;
                return Some(Err(err));
            }
        } else {
            self.cursor = cursor + 1;
            if let Err(err) = self.undo().unwrap() {
                self.signal = signal;
                return Some(Err(err));
            }
        }
        // Add signal back.
        self.signal = signal;
        let is_saved = self.is_saved();
        if let Some(ref mut f) = self.signal {
            // Emit signal if the cursor has changed.
            if old != self.cursor {
                f(Signal::Cursor {
                    old,
                    new: self.cursor,
                });
            }
            // Check if the receiver went from saved to unsaved, or unsaved to saved.
            if was_saved != is_saved {
                f(Signal::Saved(is_saved));
            }
            if redo {
                // Check if the records ability to redo changed.
                if old == len - 1 {
                    f(Signal::Redo(false));
                }
                // Check if the records ability to undo changed.
                if old == 0 {
                    f(Signal::Undo(true));
                }
            } else {
                // Check if the records ability to redo changed.
                if old == len {
                    f(Signal::Redo(true));
                }
                // Check if the records ability to undo changed.
                if old == 1 {
                    f(Signal::Undo(false));
                }
            }
        }
        Some(Ok(()))
    }

    /// Returns a reference to the `receiver`.
    #[inline]
    pub fn as_receiver(&self) -> &R {
        &self.receiver
    }

    /// Returns a mutable reference to the `receiver`.
    ///
    /// This method should **only** be used when doing changes that should not be able to be undone.
    #[inline]
    pub fn as_mut_receiver(&mut self) -> &mut R {
        &mut self.receiver
    }

    /// Consumes the record, returning the `receiver`.
    #[inline]
    pub fn into_receiver(self) -> R {
        self.receiver
    }

    /// Returns an iterator over the commands in the record.
    #[inline]
    pub fn commands(&self) -> impl Iterator<Item = &C> {
        self.commands.iter()
    }
}

impl<R, C: Command<R> + ToString> Record<R, C> {
    /// Returns the string of the command which will be undone in the next call to [`undo`].
    ///
    /// [`undo`]: struct.Record.html#method.undo
    #[inline]
    #[must_use]
    pub fn to_undo_string(&self) -> Option<String> {
        if self.can_undo() {
            Some(self.commands[self.cursor - 1].to_string())
        } else {
            None
        }
    }

    /// Returns the string of the command which will be redone in the next call to [`redo`].
    ///
    /// [`redo`]: struct.Record.html#method.redo
    #[inline]
    #[must_use]
    pub fn to_redo_string(&self) -> Option<String> {
        if self.can_redo() {
            Some(self.commands[self.cursor].to_string())
        } else {
            None
        }
    }
}

impl<R: Default, C: Command<R>> Default for Record<R, C> {
    #[inline]
    fn default() -> Record<R, C> {
        Record::new(R::default())
    }
}

impl<R, C: Command<R>> AsRef<R> for Record<R, C> {
    #[inline]
    fn as_ref(&self) -> &R {
        self.as_receiver()
    }
}

impl<R, C: Command<R>> AsMut<R> for Record<R, C> {
    #[inline]
    fn as_mut(&mut self) -> &mut R {
        self.as_mut_receiver()
    }
}

impl<R, C: Command<R>> From<R> for Record<R, C> {
    #[inline]
    fn from(receiver: R) -> Self {
        Record::new(receiver)
    }
}

impl<R: Debug, C: Command<R> + Debug> Debug for Record<R, C> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Record")
            .field("commands", &self.commands)
            .field("receiver", &self.receiver)
            .field("cursor", &self.cursor)
            .field("limit", &self.limit)
            .field("saved", &self.saved)
            .finish()
    }
}

impl<R, C: Command<R> + Display> Display for Record<R, C> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        for (i, cmd) in self.commands.iter().enumerate().rev() {
            if i + 1 == self.cursor {
                writeln!(f, "* {}", cmd)?;
            } else {
                writeln!(f, "  {}", cmd)?;
            }
        }
        Ok(())
    }
}

/// Builder for a record.
pub struct RecordBuilder<R, C: Command<R>> {
    commands: PhantomData<C>,
    receiver: PhantomData<R>,
    capacity: usize,
    limit: usize,
    saved: bool,
    signal: Option<Box<dyn FnMut(Signal) + Send + Sync + 'static>>,
}

impl<R, C: Command<R>> RecordBuilder<R, C> {
    /// Sets the capacity for the record.
    #[inline]
    pub fn capacity(mut self, capacity: usize) -> RecordBuilder<R, C> {
        self.capacity = capacity;
        self
    }

    /// Sets the `limit` of the record.
    ///
    /// # Panics
    /// Panics if `limit` is `0`.
    #[inline]
    pub fn limit(mut self, limit: usize) -> RecordBuilder<R, C> {
        assert_ne!(limit, 0);
        self.limit = limit;
        self
    }

    /// Sets if the receiver is initially in a saved state.
    /// By default the receiver is in a saved state.
    #[inline]
    pub fn saved(mut self, saved: bool) -> RecordBuilder<R, C> {
        self.saved = saved;
        self
    }

    /// Decides how different signals should be handled when the state changes.
    /// By default the record does not handle any signals.
    #[inline]
    pub fn signal(mut self, f: impl FnMut(Signal) + Send + Sync + 'static) -> RecordBuilder<R, C> {
        self.signal = Some(Box::new(f));
        self
    }

    /// Builds the record.
    #[inline]
    pub fn build(self, receiver: impl Into<R>) -> Record<R, C> {
        Record {
            commands: VecDeque::with_capacity(self.capacity),
            receiver: receiver.into(),
            cursor: 0,
            limit: self.limit,
            saved: if self.saved { Some(0) } else { None },
            signal: self.signal,
        }
    }
}

impl<R: Default, C: Command<R>> RecordBuilder<R, C> {
    /// Creates the record with a default `receiver`.
    #[inline]
    pub fn default(self) -> Record<R, C> {
        self.build(R::default())
    }
}

impl<R: Debug, C: Command<R> + Debug> Debug for RecordBuilder<R, C> {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("RecordBuilder")
            .field("receiver", &self.receiver)
            .field("capacity", &self.capacity)
            .field("limit", &self.limit)
            .field("saved", &self.saved)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[derive(Debug)]
    struct Add(char);

    impl Command<String> for Add {
        type Error = Box<dyn Error>;

        fn apply(&mut self, s: &mut String) -> Result<(), Box<dyn Error>> {
            s.push(self.0);
            Ok(())
        }

        fn undo(&mut self, s: &mut String) -> Result<(), Box<dyn Error>> {
            self.0 = s.pop().ok_or("`s` is empty")?;
            Ok(())
        }
    }

    #[derive(Debug)]
    struct JumpAdd(char, String);

    impl From<char> for JumpAdd {
        fn from(c: char) -> JumpAdd {
            JumpAdd(c, Default::default())
        }
    }

    impl Command<String> for JumpAdd {
        type Error = Box<dyn Error>;

        fn apply(&mut self, s: &mut String) -> Result<(), Box<dyn Error>> {
            self.1 = s.clone();
            s.push(self.0);
            Ok(())
        }

        fn undo(&mut self, s: &mut String) -> Result<(), Box<dyn Error>> {
            *s = self.1.clone();
            Ok(())
        }

        fn redo(&mut self, s: &mut String) -> Result<(), Box<dyn Error>> {
            *s = self.1.clone();
            s.push(self.0);
            Ok(())
        }
    }

    #[test]
    fn set_limit() {
        let mut record = Record::default();
        record.apply(Add('a')).unwrap();
        record.apply(Add('b')).unwrap();
        record.apply(Add('c')).unwrap();
        record.apply(Add('d')).unwrap();
        record.apply(Add('e')).unwrap();

        record.set_limit(3);
        assert_eq!(record.cursor(), 3);
        assert_eq!(record.limit(), 3);
        assert_eq!(record.len(), 3);
        assert!(record.can_undo());
        assert!(!record.can_redo());

        record.clear();
        assert_eq!(record.set_limit(5), 5);
        record.apply(Add('a')).unwrap();
        record.apply(Add('b')).unwrap();
        record.apply(Add('c')).unwrap();
        record.apply(Add('d')).unwrap();
        record.apply(Add('e')).unwrap();

        record.undo().unwrap().unwrap();
        record.undo().unwrap().unwrap();
        record.undo().unwrap().unwrap();

        record.set_limit(2);
        assert_eq!(record.cursor(), 0);
        assert_eq!(record.limit(), 3);
        assert_eq!(record.len(), 3);
        assert!(!record.can_undo());
        assert!(record.can_redo());

        record.redo().unwrap().unwrap();
        record.redo().unwrap().unwrap();
        record.redo().unwrap().unwrap();

        record.clear();
        assert_eq!(record.set_limit(5), 5);
        record.apply(Add('a')).unwrap();
        record.apply(Add('b')).unwrap();
        record.apply(Add('c')).unwrap();
        record.apply(Add('d')).unwrap();
        record.apply(Add('e')).unwrap();

        record.undo().unwrap().unwrap();
        record.undo().unwrap().unwrap();
        record.undo().unwrap().unwrap();
        record.undo().unwrap().unwrap();
        record.undo().unwrap().unwrap();

        record.set_limit(2);
        assert_eq!(record.cursor(), 0);
        assert_eq!(record.limit(), 5);
        assert_eq!(record.len(), 5);
        assert!(!record.can_undo());
        assert!(record.can_redo());

        record.redo().unwrap().unwrap();
        record.redo().unwrap().unwrap();
        record.redo().unwrap().unwrap();
        record.redo().unwrap().unwrap();
        record.redo().unwrap().unwrap();
    }

    #[test]
    fn go_to() {
        let mut record = Record::default();
        record.apply(Add('a')).unwrap();
        record.apply(Add('b')).unwrap();
        record.apply(Add('c')).unwrap();
        record.apply(Add('d')).unwrap();
        record.apply(Add('e')).unwrap();

        record.go_to(0).unwrap().unwrap();
        assert_eq!(record.cursor(), 0);
        assert_eq!(record.as_receiver(), "");
        record.go_to(5).unwrap().unwrap();
        assert_eq!(record.cursor(), 5);
        assert_eq!(record.as_receiver(), "abcde");
        record.go_to(1).unwrap().unwrap();
        assert_eq!(record.cursor(), 1);
        assert_eq!(record.as_receiver(), "a");
        record.go_to(4).unwrap().unwrap();
        assert_eq!(record.cursor(), 4);
        assert_eq!(record.as_receiver(), "abcd");
        record.go_to(2).unwrap().unwrap();
        assert_eq!(record.cursor(), 2);
        assert_eq!(record.as_receiver(), "ab");
        record.go_to(3).unwrap().unwrap();
        assert_eq!(record.cursor(), 3);
        assert_eq!(record.as_receiver(), "abc");
        assert!(record.go_to(6).is_none());
        assert_eq!(record.cursor(), 3);
    }

    #[test]
    fn jump_to() {
        let mut record = Record::default();
        record.apply(JumpAdd::from('a')).unwrap();
        record.apply(JumpAdd::from('b')).unwrap();
        record.apply(JumpAdd::from('c')).unwrap();
        record.apply(JumpAdd::from('d')).unwrap();
        record.apply(JumpAdd::from('e')).unwrap();

        record.jump_to(0).unwrap().unwrap();
        assert_eq!(record.cursor(), 0);
        assert_eq!(record.as_receiver(), "");
        record.jump_to(5).unwrap().unwrap();
        assert_eq!(record.cursor(), 5);
        assert_eq!(record.as_receiver(), "abcde");
        record.jump_to(1).unwrap().unwrap();
        assert_eq!(record.cursor(), 1);
        assert_eq!(record.as_receiver(), "a");
        record.jump_to(4).unwrap().unwrap();
        assert_eq!(record.cursor(), 4);
        assert_eq!(record.as_receiver(), "abcd");
        record.jump_to(2).unwrap().unwrap();
        assert_eq!(record.cursor(), 2);
        assert_eq!(record.as_receiver(), "ab");
        record.jump_to(3).unwrap().unwrap();
        assert_eq!(record.cursor(), 3);
        assert_eq!(record.as_receiver(), "abc");
        assert!(record.jump_to(6).is_none());
        assert_eq!(record.cursor(), 3);
    }
}
