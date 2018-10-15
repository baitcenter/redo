use std::collections::VecDeque;
use {Command, Error, History, Meta, Queue, Record};

/// An action that can be applied to a Record or History.
#[derive(Debug)]
enum Action<C> {
    Apply(VecDeque<Meta<C>>),
    Undo,
    Redo,
    GoTo(usize, usize),
}

/// A checkpoint wrapper.
///
/// Wraps a Record or History and gives it checkpoint functionality.
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
///     {
///         let mut cp = record.checkpoint();
///         cp.apply(Add('a'))?;
///         cp.apply(Add('b'))?;
///         cp.apply(Add('c'))?;
///         assert_eq!(cp.as_receiver(), "abc");
///         cp.cancel()?;
///     }
///     assert_eq!(record.as_receiver(), "");
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct Checkpoint<'a, T: 'a, C> {
    inner: &'a mut T,
    stack: Vec<Action<C>>,
}

impl<'a, T: 'a, C> From<&'a mut T> for Checkpoint<'a, T, C> {
    #[inline]
    fn from(inner: &'a mut T) -> Self {
        Checkpoint {
            inner,
            stack: Vec::new(),
        }
    }
}

impl<'a, R, C: Command<R>> Checkpoint<'a, Record<R, C>, C> {
    /// Calls the [`apply`] method.
    ///
    /// [`apply`]: struct.Record.html#method.apply
    #[inline]
    pub fn apply(&mut self, command: C) -> Result<(), Error<R, C>> {
        let (_, v) = self.inner.__apply(Meta::from(command))?;
        self.stack.push(Action::Apply(v));
        Ok(())
    }

    /// Calls the [`undo`] method.
    ///
    /// [`undo`]: struct.Record.html#method.undo
    #[inline]
    #[must_use]
    pub fn undo(&mut self) -> Option<Result<(), Error<R, C>>> {
        match self.inner.undo() {
            Some(Ok(_)) => {
                self.stack.push(Action::Undo);
                Some(Ok(()))
            }
            undo => undo,
        }
    }

    /// Calls the [`redo`] method.
    ///
    /// [`redo`]: struct.Record.html#method.redo
    #[inline]
    #[must_use]
    pub fn redo(&mut self) -> Option<Result<(), Error<R, C>>> {
        match self.inner.redo() {
            Some(Ok(_)) => {
                self.stack.push(Action::Redo);
                Some(Ok(()))
            }
            redo => redo,
        }
    }

    /// Calls the [`go_to`] method.
    ///
    /// [`go_to`]: struct.Record.html#method.go_to
    #[inline]
    #[must_use]
    pub fn go_to(&mut self, cursor: usize) -> Option<Result<(), Error<R, C>>> {
        let old = self.inner.cursor();
        match self.inner.go_to(cursor) {
            Some(Ok(_)) => {
                self.stack.push(Action::GoTo(0, old));
                Some(Ok(()))
            }
            go_to => go_to,
        }
    }

    /// Commits the changes and consumes the checkpoint.
    #[inline]
    pub fn commit(self) {}

    /// Cancels the changes and consumes the checkpoint.
    ///
    /// # Errors
    /// If an error occur when canceling the changes, the error is returned together with the command.
    #[inline]
    pub fn cancel(self) -> Result<(), Error<R, C>> {
        for action in self.stack.into_iter().rev() {
            match action {
                Action::Apply(mut v) => {
                    if let Some(Err(error)) = self.inner.undo() {
                        return Err(error);
                    }
                    let cursor = self.inner.cursor();
                    self.inner.commands.truncate(cursor);
                    self.inner.commands.append(&mut v);
                }
                Action::Undo => {
                    if let Some(Err(error)) = self.inner.redo() {
                        return Err(error);
                    }
                }
                Action::Redo => {
                    if let Some(Err(error)) = self.inner.undo() {
                        return Err(error);
                    }
                }
                Action::GoTo(_, cursor) => {
                    if let Some(Err(error)) = self.inner.go_to(cursor) {
                        return Err(error);
                    }
                }
            }
        }
        Ok(())
    }

    /// Returns a checkpoint.
    #[inline]
    pub fn checkpoint(&mut self) -> Checkpoint<Record<R, C>, C> {
        self.inner.checkpoint()
    }

    /// Returns a queue.
    #[inline]
    pub fn queue(&mut self) -> Queue<Record<R, C>, C> {
        self.inner.queue()
    }

    /// Returns a reference to the `receiver`.
    #[inline]
    pub fn as_receiver(&self) -> &R {
        self.inner.as_receiver()
    }

    /// Returns a mutable reference to the `receiver`.
    ///
    /// This method should **only** be used when doing changes that should not be able to be undone.
    #[inline]
    pub fn as_mut_receiver(&mut self) -> &mut R {
        self.inner.as_mut_receiver()
    }
}

impl<'a, R, C: Command<R>> AsRef<R> for Checkpoint<'a, Record<R, C>, C> {
    #[inline]
    fn as_ref(&self) -> &R {
        self.inner.as_ref()
    }
}

impl<'a, R, C: Command<R>> AsMut<R> for Checkpoint<'a, Record<R, C>, C> {
    #[inline]
    fn as_mut(&mut self) -> &mut R {
        self.inner.as_mut()
    }
}

impl<'a, R, C: Command<R>> Checkpoint<'a, History<R, C>, C> {
    /// Calls the [`apply`] method.
    ///
    /// [`apply`]: struct.History.html#method.apply
    #[inline]
    pub fn apply(&mut self, command: C) -> Result<(), Error<R, C>> {
        let root = self.inner.root();
        let old = self.inner.cursor();
        self.inner.__apply(Meta::from(command))?;
        self.stack.push(Action::GoTo(root, old));
        Ok(())
    }

    /// Calls the [`undo`] method.
    ///
    /// [`undo`]: struct.History.html#method.undo
    #[inline]
    #[must_use]
    pub fn undo(&mut self) -> Option<Result<(), Error<R, C>>> {
        match self.inner.undo() {
            Some(Ok(_)) => {
                self.stack.push(Action::Undo);
                Some(Ok(()))
            }
            undo => undo,
        }
    }

    /// Calls the [`redo`] method.
    ///
    /// [`redo`]: struct.History.html#method.redo
    #[inline]
    #[must_use]
    pub fn redo(&mut self) -> Option<Result<(), Error<R, C>>> {
        match self.inner.redo() {
            Some(Ok(_)) => {
                self.stack.push(Action::Redo);
                Some(Ok(()))
            }
            redo => redo,
        }
    }

    /// Calls the [`go_to`] method.
    ///
    /// [`go_to`]: struct.History.html#method.go_to
    #[inline]
    #[must_use]
    pub fn go_to(&mut self, branch: usize, cursor: usize) -> Option<Result<(), Error<R, C>>> {
        let root = self.inner.root();
        let old = self.inner.cursor();
        match self.inner.go_to(branch, cursor) {
            Some(Ok(_)) => {
                self.stack.push(Action::GoTo(root, old));
                Some(Ok(()))
            }
            go_to => go_to,
        }
    }

    /// Commits the changes and consumes the checkpoint.
    #[inline]
    pub fn commit(self) {}

    /// Cancels the changes and consumes the checkpoint.
    ///
    /// # Errors
    /// If an error occur when canceling the changes, the error is returned together with the command.
    #[inline]
    pub fn cancel(self) -> Result<(), Error<R, C>> {
        for action in self.stack.into_iter().rev() {
            match action {
                Action::Apply(_) => unreachable!(),
                Action::Undo => {
                    if let Some(Err(error)) = self.inner.redo() {
                        return Err(error);
                    }
                }
                Action::Redo => {
                    if let Some(Err(error)) = self.inner.undo() {
                        return Err(error);
                    }
                }
                Action::GoTo(branch, cursor) => {
                    if let Some(Err(error)) = self.inner.go_to(branch, cursor) {
                        return Err(error);
                    }
                }
            }
        }
        Ok(())
    }

    /// Returns a checkpoint.
    #[inline]
    pub fn checkpoint(&mut self) -> Checkpoint<History<R, C>, C> {
        self.inner.checkpoint()
    }

    /// Returns a queue.
    #[inline]
    pub fn queue(&mut self) -> Queue<History<R, C>, C> {
        self.inner.queue()
    }

    /// Returns a reference to the `receiver`.
    #[inline]
    pub fn as_receiver(&self) -> &R {
        self.inner.as_receiver()
    }

    /// Returns a mutable reference to the `receiver`.
    ///
    /// This method should **only** be used when doing changes that should not be able to be undone.
    #[inline]
    pub fn as_mut_receiver(&mut self) -> &mut R {
        self.inner.as_mut_receiver()
    }
}

impl<'a, R, C: Command<R>> AsRef<R> for Checkpoint<'a, History<R, C>, C> {
    #[inline]
    fn as_ref(&self) -> &R {
        self.inner.as_ref()
    }
}

impl<'a, R, C: Command<R>> AsMut<R> for Checkpoint<'a, History<R, C>, C> {
    #[inline]
    fn as_mut(&mut self) -> &mut R {
        self.inner.as_mut()
    }
}

#[cfg(test)]
mod tests {
    use std::error;
    use {Command, Record};

    #[derive(Debug)]
    struct Add(char);

    impl Command<String> for Add {
        type Error = Box<dyn error::Error>;

        fn apply(&mut self, s: &mut String) -> Result<(), Self::Error> {
            s.push(self.0);
            Ok(())
        }

        fn undo(&mut self, s: &mut String) -> Result<(), Self::Error> {
            self.0 = s.pop().ok_or("`s` is empty")?;
            Ok(())
        }
    }

    #[test]
    fn commit() {
        let mut record = Record::default();
        {
            let mut cp = record.checkpoint();
            cp.apply(Add('a')).unwrap();
            cp.apply(Add('b')).unwrap();
            cp.apply(Add('c')).unwrap();
            assert_eq!(cp.as_receiver(), "abc");
            {
                let mut cp = cp.checkpoint();
                cp.apply(Add('d')).unwrap();
                cp.apply(Add('e')).unwrap();
                cp.apply(Add('f')).unwrap();
                assert_eq!(cp.as_receiver(), "abcdef");
                {
                    let mut cp = cp.checkpoint();
                    cp.apply(Add('g')).unwrap();
                    cp.apply(Add('h')).unwrap();
                    cp.apply(Add('i')).unwrap();
                    assert_eq!(cp.as_receiver(), "abcdefghi");
                    cp.commit();
                }
                cp.commit();
            }
            cp.commit();
        }
        assert_eq!(record.as_receiver(), "abcdefghi");
    }

    #[test]
    fn cancel() {
        let mut record = Record::default();
        {
            let mut cp = record.checkpoint();
            cp.apply(Add('a')).unwrap();
            cp.apply(Add('b')).unwrap();
            cp.apply(Add('c')).unwrap();
            {
                let mut cp = cp.checkpoint();
                cp.apply(Add('d')).unwrap();
                cp.apply(Add('e')).unwrap();
                cp.apply(Add('f')).unwrap();
                {
                    let mut cp = cp.checkpoint();
                    cp.apply(Add('g')).unwrap();
                    cp.apply(Add('h')).unwrap();
                    cp.apply(Add('i')).unwrap();
                    assert_eq!(cp.as_receiver(), "abcdefghi");
                    cp.cancel().unwrap();
                }
                assert_eq!(cp.as_receiver(), "abcdef");
                cp.cancel().unwrap();
            }
            assert_eq!(cp.as_receiver(), "abc");
            cp.cancel().unwrap();
        }
        assert_eq!(record.as_receiver(), "");
    }
}
