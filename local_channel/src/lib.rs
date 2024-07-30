//! A collection of simple [`!Send`] and [`!Sync`] async channels with minimal dependencies.
//!
//! Currently, are two kinds of channels:
//!
//! 1. [`mpsc::channel()`] async channel with unlimited capacity.
//! 2. [`oneshot::oneshot()`] async oneshot channel.

use std::{
    error::Error,
    fmt,
};

pub mod mpsc;
pub mod oneshot;

/// Error returned by the `Sender`s in case the associated `Receiver` was dropped.
/// Contains the value that was tried to send.
pub struct SendError<T>(pub T);

impl<T> fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SendError")
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl<T> Error for SendError<T> {}

/// Receive error.
#[derive(PartialEq, Eq, Debug)]
pub enum ReceiveError {
    /// The channel is closed, all senders have been dropped.
    Closed,
}

impl fmt::Display for ReceiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Error for ReceiveError {}

/// Try receive error.
#[derive(PartialEq, Eq, Debug)]
pub enum TryReceiveError {
    /// The channel buffer is empty, but senders are still active.
    Empty,

    /// The channel is closed, all senders have been dropped.
    Closed,
}

impl fmt::Display for TryReceiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Error for TryReceiveError {}
