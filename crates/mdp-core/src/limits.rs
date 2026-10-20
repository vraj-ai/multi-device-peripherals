//! One owner for the wire cap and the clipboard cap.
//!
//! `1024 * 1024` lives only in this file. Callers use [`MAX_FRAME_BYTES`] or
//! [`CLIPBOARD_MAX_BYTES`]. Copying the literal is how the two caps drifted:
//! a clipboard of [`MAX_FRAME_BYTES`] bytes encodes to 4 extra bytes and the
//! frame codec rejects it.

/// Largest accepted encoded frame body, in bytes (1 MiB).
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Postcard bytes in front of a [`crate::proto::Frame::Clipboard`] string:
/// a one-byte variant index plus a three-byte length, for text of this size.
/// `max_clipboard_frame_fits_the_wire_cap` fails when the codec no longer
/// matches, and this constant is what to change.
pub const CLIPBOARD_FRAME_OVERHEAD: usize = 4;

/// Largest UTF-8 clipboard whose encoded frame still fits in [`MAX_FRAME_BYTES`].
pub const CLIPBOARD_MAX_BYTES: usize = MAX_FRAME_BYTES - CLIPBOARD_FRAME_OVERHEAD;
