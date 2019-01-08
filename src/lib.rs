//! Provides undo-redo functionality with static dispatch and manual command merging.
//!
//! # Contents
//!
//! * [Record] provides a stack based undo-redo functionality.
//! * [History] provides a tree based undo-redo functionality that allows you to jump between different branches.
//! * [Queue] wraps a [Record] or [History] and extends them with queue functionality.
//! * [Checkpoint] wraps a [Record] or [History] and extends them with checkpoint functionality.
//! * Commands can be merged using the [merge] method.
//!   When two commands are merged, undoing and redoing them are done in a single step.
//! * Configurable display formatting is provided through the [Display] structure.
//! * Time stamps and time travel is provided when the `chrono` feature is enabled.
//! * Serialization and deserialization is provided when the `serde` feature is enabled.
//!
//! # Examples
//!
//! Add this to `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! redo = "0.30"
//! ```
//!
//! And this to `main.rs`:
//!
//! ```
//! use redo::{Command, Record};
//!
//! #[derive(Debug)]
//! struct Add(char);
//!
//! impl Command<String> for Add {
//!     type Error = &'static str;
//!
//!     fn apply(&mut self, s: &mut String) -> Result<(), Self::Error> {
//!         s.push(self.0);
//!         Ok(())
//!     }
//!
//!     fn undo(&mut self, s: &mut String) -> Result<(), Self::Error> {
//!         self.0 = s.pop().ok_or("`s` is empty")?;
//!         Ok(())
//!     }
//! }
//!
//! fn main() -> redo::Result<String, Add> {
//!     let mut record = Record::default();
//!     record.apply(Add('a'))?;
//!     record.apply(Add('b'))?;
//!     record.apply(Add('c'))?;
//!     assert_eq!(record.as_receiver(), "abc");
//!     record.undo().unwrap()?;
//!     record.undo().unwrap()?;
//!     record.undo().unwrap()?;
//!     assert_eq!(record.as_receiver(), "");
//!     record.redo().unwrap()?;
//!     record.redo().unwrap()?;
//!     record.redo().unwrap()?;
//!     assert_eq!(record.as_receiver(), "abc");
//!     Ok(())
//! }
//! ```
//!
//! [Record]: struct.Record.html
//! [History]: struct.History.html
//! [Queue]: struct.Queue.html
//! [Checkpoint]: struct.Checkpoint.html
//! [Display]: struct.Display.html
//! [merge]: trait.Command.html#method.merge

#![doc(html_root_url = "https://docs.rs/redo/0.30.0")]
#![deny(
    bad_style,
    bare_trait_objects,
    missing_debug_implementations,
    missing_docs,
    unused_import_braces,
    unused_qualifications,
    unsafe_code,
    unstable_features
)]

mod checkpoint;
mod display;
mod history;
mod queue;
mod record;

#[cfg(feature = "chrono")]
use chrono::{DateTime, Utc};
#[cfg(feature = "chrono")]
use serde::{Deserialize, Serialize};
use std::fmt;

pub use self::checkpoint::Checkpoint;
pub use self::display::Display;
pub use self::history::{History, HistoryBuilder};
pub use self::queue::Queue;
pub use self::record::{Record, RecordBuilder};

/// A specialized Result type for undo-redo operations.
pub type Result<R, C> = std::result::Result<(), Error<R, C>>;

/// Base functionality for all commands.
pub trait Command<R> {
    /// The error type.
    type Error;

    /// Applies the command on the receiver and returns `Ok` if everything went fine,
    /// and `Err` if something went wrong.
    fn apply(&mut self, receiver: &mut R) -> std::result::Result<(), Self::Error>;

    /// Restores the state of the receiver as it was before the command was applied
    /// and returns `Ok` if everything went fine, and `Err` if something went wrong.
    fn undo(&mut self, receiver: &mut R) -> std::result::Result<(), Self::Error>;

    /// Reapplies the command on the receiver and return `Ok` if everything went fine,
    /// and `Err` if something went wrong.
    ///
    /// The default implementation uses the [`apply`] implementation.
    ///
    /// [`apply`]: trait.Command.html#tymethod.apply
    #[inline]
    fn redo(&mut self, receiver: &mut R) -> std::result::Result<(), Self::Error> {
        self.apply(receiver)
    }

    /// Used for manual merging of two commands.
    ///
    /// # Examples
    /// ```
    /// # use redo::{Command, Merge, Record};
    /// #[derive(Debug)]
    /// struct Add(String);
    ///
    /// impl Command<String> for Add {
    ///     type Error = ();
    ///
    ///     fn apply(&mut self, s: &mut String) -> Result<(), ()> {
    ///         s.push_str(&self.0);
    ///         Ok(())
    ///     }
    ///
    ///     fn undo(&mut self, s: &mut String) -> Result<(), ()> {
    ///         let len = s.len() - self.0.len();
    ///         s.truncate(len);
    ///         Ok(())
    ///     }
    ///
    ///     fn merge(&mut self, Add(s): Self) -> Merge<Self> {
    ///         self.0.push_str(&s);
    ///         Merge::Yes
    ///     }
    /// }
    ///
    /// fn main() -> redo::Result<String, Add> {
    ///     let mut record = Record::default();
    ///     // The `a`, `b`, and `c` commands are merged.
    ///     record.apply(Add("a".into()))?;
    ///     record.apply(Add("b".into()))?;
    ///     record.apply(Add("c".into()))?;
    ///     assert_eq!(record.as_receiver(), "abc");
    ///     // Calling `undo` once will undo all the merged commands.
    ///     record.undo().unwrap()?;
    ///     assert_eq!(record.as_receiver(), "");
    ///     // Calling `redo` once will redo all the merged commands.
    ///     record.redo().unwrap()?;
    ///     assert_eq!(record.as_receiver(), "abc");
    ///     Ok(())
    /// }
    /// ```
    #[inline]
    fn merge(&mut self, command: Self) -> Merge<Self>
    where
        Self: Sized,
    {
        Merge::No(command)
    }
}

/// The signal sent when the record, the history, or the receiver changes.
///
/// When one of these states changes, they will send a corresponding signal to the user.
/// For example, if the record can no longer redo any commands, it sends a `Redo(false)`
/// signal to tell the user.
///
/// # Examples
/// ```
/// # use redo::{Command, History, Signal};
/// # struct Add(char);
/// # impl Command<String> for Add {
/// #     type Error = ();
/// #     fn apply(&mut self, s: &mut String) -> Result<(), Self::Error> { Ok(()) }
/// #     fn undo(&mut self, s: &mut String) -> Result<(), Self::Error> { Ok(()) }
/// # }
/// # fn foo() -> History<String, Add> {
/// let history = History::builder()
///     .connect(|signal| match signal {
///         Signal::Undo(on) => println!("undo: {}", on),
///         Signal::Redo(on) => println!("redo: {}", on),
///         Signal::Saved(on) => println!("saved: {}", on),
///         Signal::Cursor { old, new } => println!("cursor: {} -> {}", old, new),
///         Signal::Root { old, new } => println!("root: {} -> {}", old, new),
///     })
///     .default();
/// # history
/// # }
/// ```
#[derive(Copy, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub enum Signal {
    /// Says if the record can undo.
    ///
    /// This signal will be emitted when the records ability to undo changes.
    Undo(bool),
    /// Says if the record can redo.
    ///
    /// This signal will be emitted when the records ability to redo changes.
    Redo(bool),
    /// Says if the receiver is in a saved state.
    ///
    /// This signal will be emitted when the record enters or leaves its receivers saved state.
    Saved(bool),
    /// Says if the current command has changed.
    ///
    /// This signal will be emitted when the cursor has changed. This includes
    /// when two commands have been merged, in which case `old == new`.
    Cursor {
        /// The old cursor.
        old: usize,
        /// The new cursor.
        new: usize,
    },
    /// Says if the current branch, or root, has changed.
    ///
    /// This is only emitted from `History`.
    Root {
        /// The old root.
        old: usize,
        /// The new root.
        new: usize,
    },
}

/// The result of merging two commands.
#[derive(Copy, Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub enum Merge<C> {
    /// The commands have been merged.
    Yes,
    /// The commands have not been merged.
    No(C),
    /// The two commands cancels each other out. This removes both commands.
    Annul,
}

/// A position in a history tree.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Copy, Clone, Debug, Default, Hash, Ord, PartialOrd, Eq, PartialEq)]
struct At {
    branch: usize,
    cursor: usize,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug)]
struct Meta<C> {
    command: C,
    #[cfg(feature = "chrono")]
    timestamp: DateTime<Utc>,
}

impl<C> From<C> for Meta<C> {
    #[inline]
    fn from(command: C) -> Self {
        Meta {
            command,
            #[cfg(feature = "chrono")]
            timestamp: Utc::now(),
        }
    }
}

impl<R, C: Command<R>> Command<R> for Meta<C> {
    type Error = C::Error;

    #[inline]
    fn apply(&mut self, receiver: &mut R) -> std::result::Result<(), <Self as Command<R>>::Error> {
        self.command.apply(receiver)
    }

    #[inline]
    fn undo(&mut self, receiver: &mut R) -> std::result::Result<(), <Self as Command<R>>::Error> {
        self.command.undo(receiver)
    }

    #[inline]
    fn redo(&mut self, receiver: &mut R) -> std::result::Result<(), <Self as Command<R>>::Error> {
        self.command.redo(receiver)
    }

    #[inline]
    fn merge(&mut self, command: Self) -> Merge<Self>
    where
        Self: Sized,
    {
        match self.command.merge(command.command) {
            Merge::Yes => Merge::Yes,
            Merge::No(command) => Merge::No(Meta::from(command)),
            Merge::Annul => Merge::Annul,
        }
    }
}

impl<C: fmt::Display> fmt::Display for Meta<C> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        (&self.command as &dyn fmt::Display).fmt(f)
    }
}

/// An error which holds the command that caused it.
pub struct Error<R, C: Command<R>> {
    meta: Meta<C>,
    error: C::Error,
}

impl<R, C: Command<R>> Error<R, C> {
    /// Returns a new error.
    #[inline]
    fn new(meta: Meta<C>, error: C::Error) -> Error<R, C> {
        Error { meta, error }
    }
}

impl<R, C: Command<R>> Error<R, C> {
    /// Returns a reference to the command that caused the error.
    #[inline]
    pub fn command(&self) -> &C {
        &self.meta.command
    }

    /// Returns the command that caused the error.
    #[inline]
    pub fn into_command(self) -> C {
        self.meta.command
    }
}

impl<R, C: Command<R> + fmt::Debug> fmt::Debug for Error<R, C>
where
    C::Error: fmt::Debug,
{
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Error")
            .field("meta", &self.meta)
            .field("error", &self.error)
            .finish()
    }
}

impl<R, C: Command<R>> fmt::Display for Error<R, C>
where
    C::Error: fmt::Display,
{
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        (&self.error as &dyn fmt::Display).fmt(f)
    }
}

impl<R, C: Command<R>> std::error::Error for Error<R, C>
where
    C: fmt::Debug,
    C::Error: std::error::Error,
{
    #[inline]
    fn description(&self) -> &str {
        self.error.description()
    }

    #[inline]
    fn cause(&self) -> Option<&dyn std::error::Error> {
        self.error.cause()
    }
}
