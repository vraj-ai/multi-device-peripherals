//! Wire [`Frame`]s: the `postcard` enum every [`Link`](crate::link::Link) carries.
//!
//! Each frame is encoded with `postcard`, encrypted with Noise, and sent over
//! TCP as a big-endian `u32` length + payload. Encoded bodies over 1 MiB are
//! rejected, so the largest accepted wire frame is 1 MiB + header.

use crate::platform::{Desktop, MouseButton};
use serde::{Deserialize, Serialize};

/// Protocol version sent in [`Frame::Hello`].
pub const PROTOCOL_VERSION: u32 = 1;

/// Bytes of the `u32` length prefix on the wire.
pub const FRAME_HEADER_LEN: usize = 4;

/// Largest accepted encoded frame body in bytes (1 MiB, matching the
/// clipboard cap). With the length header, the largest wire frame is
/// 1 MiB + header.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Which side of this Peer's Desktop the other Peer's Desktop sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArrangementSide {
    Left,
    Right,
}

/// One message on the Link, per the v1 spec: input and clipboard from the
/// Source, Arrangement sync, Crossing handover, Source claims, heartbeats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Frame {
    /// Peer arrival: its Desktop bounds plus [`PROTOCOL_VERSION`].
    Hello {
        desktop: Desktop,
        version: u32,
    },
    /// Arrangement sync: where the other Peer's Desktop sits, plus offset.
    Arrangement {
        side: ArrangementSide,
        offset: f64,
    },
    /// Crossing into the Sink: proportional position on the shared edge.
    Enter {
        edge_pos: f64,
    },
    /// Cursor left back through the shared edge: Focus returns.
    Leave {
        edge_pos: f64,
    },
    /// Relative cursor motion while the cursor is remote.
    MouseMove {
        dx: f64,
        dy: f64,
    },
    MouseButton {
        button: MouseButton,
        down: bool,
    },
    Wheel {
        dx: f64,
        dy: f64,
    },
    /// Key by physical USB HID usage code; the Sink injects it natively.
    Key {
        hid_usage: u16,
        down: bool,
    },
    /// Release every held key/button on Focus change, Crossing, or disconnect.
    ReleaseAll,
    /// Physical input on a non-Source Peer claims Source (Lamport seq).
    ClaimSource {
        seq: u64,
    },
    /// Clipboard text sync (UTF-8, at most 1 MiB).
    Clipboard {
        text: String,
    },
    Ping {
        t: u64,
    },
    Pong {
        t: u64,
    },
}

/// Codec failures for [`encode_frame`] / [`decode_frame`].
#[derive(Debug)]
pub enum CodecError {
    Postcard(postcard::Error),
    FrameTooLarge(usize),
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Postcard(err) => write!(f, "frame codec failed: {err}"),
            Self::FrameTooLarge(len) => write!(f, "frame over 1 MiB + header: {len} bytes"),
        }
    }
}

impl std::error::Error for CodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Postcard(err) => Some(err),
            Self::FrameTooLarge(_) => None,
        }
    }
}

impl From<postcard::Error> for CodecError {
    fn from(err: postcard::Error) -> Self {
        Self::Postcard(err)
    }
}

/// Encode one frame; rejects bodies over 1 MiB ([`MAX_FRAME_BYTES`]).
pub fn encode_frame(frame: &Frame) -> Result<Vec<u8>, CodecError> {
    let body = postcard::to_allocvec(frame)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(CodecError::FrameTooLarge(body.len()));
    }
    Ok(body)
}

/// Decode one frame body; rejects inputs over 1 MiB before parsing.
pub fn decode_frame(body: &[u8]) -> Result<Frame, CodecError> {
    if body.len() > MAX_FRAME_BYTES {
        return Err(CodecError::FrameTooLarge(body.len()));
    }
    Ok(postcard::from_bytes(body)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_frame() -> Vec<Frame> {
        vec![
            Frame::Hello {
                desktop: Desktop::new(0.0, 0.0, 1920.0, 1080.0),
                version: PROTOCOL_VERSION,
            },
            Frame::Arrangement {
                side: ArrangementSide::Right,
                offset: 120.0,
            },
            Frame::Arrangement {
                side: ArrangementSide::Left,
                offset: -40.5,
            },
            Frame::Enter { edge_pos: 0.25 },
            Frame::Leave { edge_pos: 0.75 },
            Frame::MouseMove { dx: 3.0, dy: -2.0 },
            Frame::MouseButton {
                button: MouseButton::Left,
                down: true,
            },
            Frame::Wheel { dx: 0.0, dy: 1.0 },
            Frame::Key {
                hid_usage: 0x04,
                down: true,
            },
            Frame::ReleaseAll,
            Frame::ClaimSource { seq: 42 },
            Frame::Clipboard {
                text: "paste me".to_string(),
            },
            Frame::Ping { t: 7 },
            Frame::Pong { t: 7 },
        ]
    }

    #[test]
    fn round_trip_every_frame_variant() {
        for frame in every_frame() {
            let body = encode_frame(&frame).expect("encode frame");
            assert_eq!(decode_frame(&body).expect("decode frame"), frame);
        }
    }

    #[test]
    fn rejects_frames_over_1mib() {
        let big = Frame::Clipboard {
            text: "x".repeat(MAX_FRAME_BYTES + 1),
        };
        assert!(
            matches!(encode_frame(&big), Err(CodecError::FrameTooLarge(_))),
            "oversize encode must fail"
        );
        assert!(
            matches!(
                decode_frame(&vec![0u8; MAX_FRAME_BYTES + 1]),
                Err(CodecError::FrameTooLarge(_))
            ),
            "oversize decode must fail before parsing"
        );
    }
}
