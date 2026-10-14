//! The encrypted [`Link`] between two paired Peers.
//!
//! One TCP connection per pair. The Peers run Noise `XX` (`snow`), derive the
//! same 6-digit Pairing code from the handshake hash, and refuse unknown
//! static keys before any frame is delivered. After Pairing, frames move with
//! [`Link::send_frame`] / [`Link::recv_frame`]; every frame on the wire is
//! Noise-encrypted, never plaintext.

use crate::proto::{decode_frame, encode_frame, Frame, MAX_FRAME_BYTES};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, ToSocketAddrs};

/// Noise pattern for the Link: mutually authenticated, no prior knowledge.
pub const NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

/// Bytes Noise appends per encrypted message (ChaChaPoly authentication tag).
pub const NOISE_TAG_LEN: usize = 16;

/// Largest Noise message, tag included (the spec's limit; `snow` enforces it).
pub const NOISE_MAX_MESSAGE: usize = 65535;

/// Frame body bytes carried by one full Noise message.
const NOISE_MAX_PLAINTEXT: usize = NOISE_MAX_MESSAGE - NOISE_TAG_LEN;

/// Largest accepted encrypted wire payload: the largest frame body plus one
/// tag per Noise message it takes.
pub const MAX_WIRE_BYTES: usize =
    MAX_FRAME_BYTES + NOISE_TAG_LEN * MAX_FRAME_BYTES.div_ceil(NOISE_MAX_PLAINTEXT);

/// Bytes of a serialized [`StaticKeypair`] (private key then public key).
pub const KEYPAIR_BYTES_LEN: usize = 64;

/// A Peer's long-term Noise static public key. Pinned after Pairing; unknown
/// keys are refused before any frame is delivered.
pub type PeerKey = [u8; 32];

/// This Peer's long-term Noise static keypair.
pub struct StaticKeypair {
    private: [u8; 32],
    public: PeerKey,
}

impl StaticKeypair {
    /// Generate a fresh static keypair.
    pub fn generate() -> Result<Self, LinkError> {
        let params: snow::params::NoiseParams = NOISE_PATTERN.parse()?;
        let keypair = snow::Builder::new(params).generate_keypair()?;
        Ok(Self {
            private: keypair
                .private
                .try_into()
                .map_err(|_| LinkError::KeyLength)?,
            public: keypair
                .public
                .try_into()
                .map_err(|_| LinkError::KeyLength)?,
        })
    }

    /// This Peer's static public key: what the other Peer pins.
    pub fn public_key(&self) -> PeerKey {
        self.public
    }

    /// Serialize for storage (private key first, 64 bytes total).
    pub fn to_bytes(&self) -> [u8; KEYPAIR_BYTES_LEN] {
        let mut bytes = [0u8; KEYPAIR_BYTES_LEN];
        bytes[..32].copy_from_slice(&self.private);
        bytes[32..].copy_from_slice(&self.public);
        bytes
    }

    /// Restore from [`StaticKeypair::to_bytes`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LinkError> {
        if bytes.len() != KEYPAIR_BYTES_LEN {
            return Err(LinkError::KeyLength);
        }
        let mut private = [0u8; 32];
        let mut public = [0u8; 32];
        private.copy_from_slice(&bytes[..32]);
        public.copy_from_slice(&bytes[32..]);
        Ok(Self { private, public })
    }
}

/// Derive the 6-digit Pairing code both Peers show: the first 20 bits of
/// SHA-256 over the Noise handshake hash, as zero-padded decimal. Both Peers
/// hold the same handshake hash, so both derive the same code.
pub fn pairing_code(handshake_hash: &[u8]) -> String {
    let digest = Sha256::digest(handshake_hash);
    let bits =
        (u32::from(digest[0]) << 12) | (u32::from(digest[1]) << 4) | (u32::from(digest[2]) >> 4);
    format!("{:06}", bits % 1_000_000)
}

/// Link failures.
#[derive(Debug)]
pub enum LinkError {
    Io(std::io::Error),
    Noise(snow::Error),
    Codec(crate::proto::CodecError),
    /// The Pairing confirm callback said no: connection closed, no frames flowed.
    PairingRejected,
    /// The Peer's static key is not pinned: refused before any frame.
    UnknownPeerKey,
    /// A wire length (handshake message or encrypted frame) exceeds its cap.
    FrameTooLarge(usize),
    /// A static key or serialized keypair has the wrong length.
    KeyLength,
    /// Peer's static key missing after the handshake (unreachable in `XX`).
    MissingPeerKey,
    /// The TCP connection closed.
    Closed,
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "link I/O failed: {err}"),
            Self::Noise(err) => write!(f, "noise failed: {err}"),
            Self::Codec(err) => write!(f, "frame codec failed: {err}"),
            Self::PairingRejected => write!(f, "pairing code rejected"),
            Self::UnknownPeerKey => write!(f, "peer key is not pinned"),
            Self::FrameTooLarge(len) => write!(f, "wire object too large: {len} bytes"),
            Self::KeyLength => write!(f, "static key has the wrong length"),
            Self::MissingPeerKey => write!(f, "handshake yielded no peer key"),
            Self::Closed => write!(f, "link closed"),
        }
    }
}

impl std::error::Error for LinkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Noise(err) => Some(err),
            Self::Codec(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for LinkError {
    fn from(err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::UnexpectedEof {
            Self::Closed
        } else {
            Self::Io(err)
        }
    }
}

impl From<snow::Error> for LinkError {
    fn from(err: snow::Error) -> Self {
        Self::Noise(err)
    }
}

impl From<crate::proto::CodecError> for LinkError {
    fn from(err: crate::proto::CodecError) -> Self {
        Self::Codec(err)
    }
}

fn check_wire_len(len: u32) -> Result<usize, LinkError> {
    let len = len as usize;
    if len > MAX_WIRE_BYTES {
        return Err(LinkError::FrameTooLarge(len));
    }
    Ok(len)
}

/// The encrypted Link to one paired Peer: exactly one TCP connection.
pub struct Link {
    /// Committed frames not yet written ([`Self::queue_frame`]), drained by
    /// the cancel-safe [`Self::flush`]: a dropped write never leaves half a
    /// frame behind, and a frame once queued always goes out, in order.
    write_buf: VecDeque<u8>,
    stream: TcpStream,
    transport: snow::TransportState,
    peer_key: PeerKey,
    /// Bytes already taken from the socket. `recv_frame` is polled inside
    /// `select`, so a cancelled read must not drop a length prefix on the floor.
    read_buf: Vec<u8>,
}

impl Link {
    /// Dial the other Peer over TCP and pair as the Noise initiator.
    ///
    /// With an empty `pinned` list the `confirm` callback decides Pairing:
    /// it receives the 6-digit code and returns true to pin the Peer's key.
    /// With a non-empty list, pinned keys connect silently and unknown keys
    /// are refused. Returns the Link plus the Peer's static key for pinning.
    pub async fn connect<A, F>(
        addr: A,
        keypair: &StaticKeypair,
        pinned: &[PeerKey],
        confirm: F,
    ) -> Result<(Self, PeerKey), LinkError>
    where
        A: ToSocketAddrs,
        F: Fn(&str) -> bool + Send,
    {
        let stream = TcpStream::connect(addr).await?;
        let params: snow::params::NoiseParams = NOISE_PATTERN.parse()?;
        let session = snow::Builder::new(params)
            .local_private_key(&keypair.private)
            .build_initiator()?;
        finish_handshake(stream, session, pinned, confirm).await
    }

    /// Accept one inbound TCP stream and pair as the Noise responder.
    ///
    /// Same Pairing rules as [`Link::connect`]: empty `pinned` means confirm
    /// the code, non-empty means pinned keys pass and unknown keys are refused.
    pub async fn accept<F>(
        stream: TcpStream,
        keypair: &StaticKeypair,
        pinned: &[PeerKey],
        confirm: F,
    ) -> Result<(Self, PeerKey), LinkError>
    where
        F: Fn(&str) -> bool + Send,
    {
        let params: snow::params::NoiseParams = NOISE_PATTERN.parse()?;
        let session = snow::Builder::new(params)
            .local_private_key(&keypair.private)
            .build_responder()?;
        finish_handshake(stream, session, pinned, confirm).await
    }

    /// The other Peer's static key: pin it after confirming the Pairing code.
    pub fn peer_key(&self) -> PeerKey {
        self.peer_key
    }

    /// The local address of the underlying TCP connection.
    pub fn local_addr(&self) -> Result<SocketAddr, LinkError> {
        Ok(self.stream.local_addr()?)
    }

    /// Encrypt and send one frame. Nothing leaves unencrypted: the Noise
    /// handshake that precedes this carries no frames at all.
    ///
    /// Safe to cancel (a full socket under `select`). Once the frame is
    /// queued it is committed: its unsent bytes stay buffered and go out
    /// first on the next flush. Dropped before that (still flushing earlier
    /// frames), this frame was never sent.
    pub async fn send_frame(&mut self, frame: &Frame) -> Result<(), LinkError> {
        // Finish earlier frames before spending a nonce on this one, so a
        // stalled peer cannot grow the buffer one frame per call.
        self.flush().await?;
        self.queue_frame(frame)?;
        self.flush().await
    }

    /// Encrypt one frame into the outgoing buffer without waiting. From here
    /// it is committed and goes out, in order, on the next [`Self::flush`].
    /// Lets a caller commit several frames and its local effects as one
    /// unit, with no await (and so no cancellation point) in between.
    pub(crate) fn queue_frame(&mut self, frame: &Frame) -> Result<(), LinkError> {
        let body = encode_frame(frame)?;
        // Length prefix and ciphertext in one buffer: split writes let Nagle
        // hold the body back a delayed-ACK interval behind its prefix.
        let cipher = seal(&mut self.transport, &body)?;
        self.write_buf.extend((cipher.len() as u32).to_be_bytes());
        self.write_buf.extend(&cipher);
        Ok(())
    }

    /// Write out [`Self::write_buf`], reading into [`Self::read_buf`] while
    /// the socket is full: a frame can outgrow the socket buffers, and two
    /// Peers both blocked writing, neither reading, would wait forever.
    /// Cancel-safe: each branch either moves bytes between socket and
    /// buffer or does nothing.
    pub(crate) async fn flush(&mut self) -> Result<(), LinkError> {
        let (mut reader, mut writer) = self.stream.split();
        while !self.write_buf.is_empty() {
            let mut tmp = [0u8; 8192];
            tokio::select! {
                written = writer.write_buf(&mut self.write_buf) => {
                    if written? == 0 {
                        return Err(LinkError::Closed);
                    }
                }
                read = reader.read(&mut tmp) => {
                    let n = read?;
                    if n == 0 {
                        return Err(LinkError::Closed);
                    }
                    self.read_buf.extend_from_slice(&tmp[..n]);
                }
            }
        }
        Ok(())
    }

    /// Receive and decrypt one frame.
    ///
    /// Safe to cancel (a `select` branch losing to local input, or a test
    /// timeout). Bytes already read stay in [`Self::read_buf`]; the Noise
    /// nonce advances only once a whole ciphertext is in hand.
    pub async fn recv_frame(&mut self) -> Result<Frame, LinkError> {
        loop {
            if let Some(frame) = self.decode_buffered()? {
                return Ok(frame);
            }
            let mut tmp = [0u8; 8192];
            let n = self.stream.read(&mut tmp).await?;
            if n == 0 {
                return Err(LinkError::Closed);
            }
            self.read_buf.extend_from_slice(&tmp[..n]);
        }
    }

    /// Pop one frame from [`Self::read_buf`], or `Ok(None)` when it holds only
    /// a prefix. Decrypts only after the length and ciphertext are both buffered.
    fn decode_buffered(&mut self) -> Result<Option<Frame>, LinkError> {
        if self.read_buf.len() < 4 {
            return Ok(None);
        }
        let len_bytes: [u8; 4] = self.read_buf[..4].try_into().expect("4 bytes");
        let wire_len = check_wire_len(u32::from_be_bytes(len_bytes))?;
        if self.read_buf.len() < 4 + wire_len {
            return Ok(None);
        }
        let cipher = self.read_buf[4..4 + wire_len].to_vec();
        self.read_buf.drain(..4 + wire_len);
        let body = open(&mut self.transport, &cipher)?;
        Ok(Some(decode_frame(&body)?))
    }
}

/// Encrypt a frame body as back-to-back Noise messages, each at most
/// [`NOISE_MAX_MESSAGE`] bytes. A body that fits one message (every frame a
/// v1 peer can send) encrypts exactly as before; only the last message may
/// be short, so [`open`] can find the boundaries from the length alone.
fn seal(transport: &mut snow::TransportState, body: &[u8]) -> Result<Vec<u8>, LinkError> {
    let messages = body.len().div_ceil(NOISE_MAX_PLAINTEXT);
    let mut cipher = vec![0u8; body.len() + messages * NOISE_TAG_LEN];
    let mut written = 0;
    for chunk in body.chunks(NOISE_MAX_PLAINTEXT) {
        written += transport.write_message(chunk, &mut cipher[written..])?;
    }
    Ok(cipher)
}

/// Decrypt what [`seal`] produced: split every [`NOISE_MAX_MESSAGE`] bytes.
fn open(transport: &mut snow::TransportState, cipher: &[u8]) -> Result<Vec<u8>, LinkError> {
    let mut body = vec![0u8; cipher.len()];
    let mut read = 0;
    for message in cipher.chunks(NOISE_MAX_MESSAGE) {
        read += transport.read_message(message, &mut body[read..])?;
    }
    body.truncate(read);
    Ok(body)
}

async fn finish_handshake<F>(
    mut stream: TcpStream,
    mut session: snow::HandshakeState,
    pinned: &[PeerKey],
    confirm: F,
) -> Result<(Link, PeerKey), LinkError>
where
    F: Fn(&str) -> bool + Send,
{
    // Input frames are small and back to back; Nagle would hold each one
    // behind the previous frame's ACK, far past the crossing latency budget.
    stream.set_nodelay(true)?;
    let mut read_buf = vec![0u8; NOISE_MAX_MESSAGE];
    let mut msg_buf = vec![0u8; NOISE_MAX_MESSAGE];
    while !session.is_handshake_finished() {
        if session.is_my_turn() {
            let len = session.write_message(&[], &mut msg_buf)?;
            stream.write_u32(len as u32).await?;
            stream.write_all(&msg_buf[..len]).await?;
            stream.flush().await?;
        } else {
            let len = stream.read_u32().await? as usize;
            if len > read_buf.len() {
                return Err(LinkError::FrameTooLarge(len));
            }
            stream.read_exact(&mut read_buf[..len]).await?;
            session.read_message(&read_buf[..len], &mut msg_buf)?;
        }
    }
    let peer_key: PeerKey = session
        .get_remote_static()
        .and_then(|key| key.try_into().ok())
        .ok_or(LinkError::MissingPeerKey)?;
    let known = pinned.contains(&peer_key);
    if !pinned.is_empty() && !known {
        return Err(LinkError::UnknownPeerKey);
    }
    if !known && !confirm(&pairing_code(session.get_handshake_hash())) {
        return Err(LinkError::PairingRejected);
    }
    let transport = session.into_transport_mode()?;
    Ok((
        Link {
            write_buf: VecDeque::new(),
            stream,
            transport,
            peer_key,
            read_buf: Vec::new(),
        },
        peer_key,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::net::TcpListener;

    #[test]
    fn keypair_serialization_round_trip() {
        let keypair = StaticKeypair::generate().expect("generate");
        let bytes = keypair.to_bytes();
        assert_eq!(bytes.len(), KEYPAIR_BYTES_LEN);
        let restored = StaticKeypair::from_bytes(&bytes).expect("restore");
        assert_eq!(restored.public_key(), keypair.public_key());
        assert!(
            StaticKeypair::from_bytes(&bytes[..KEYPAIR_BYTES_LEN - 1]).is_err(),
            "short input must fail"
        );
    }

    #[test]
    fn pairing_code_is_six_digits_and_stable() {
        let first = pairing_code(b"handshake-hash");
        assert_eq!(first, pairing_code(b"handshake-hash"));
        assert_eq!(first.len(), 6);
        assert!(first.bytes().all(|byte| byte.is_ascii_digit()));
        assert_ne!(first, pairing_code(b"a-different-handshake-hash"));
    }

    #[test]
    fn wire_len_guard_rejects_oversize() {
        assert!(check_wire_len(0).is_ok());
        assert!(check_wire_len(MAX_WIRE_BYTES as u32).is_ok());
        assert!(
            matches!(
                check_wire_len(MAX_WIRE_BYTES as u32 + 1),
                Err(LinkError::FrameTooLarge(_))
            ),
            "ciphertext over MAX_WIRE_BYTES must fail"
        );
    }

    #[tokio::test]
    async fn loopback_pairing_exchanges_frames_both_ways() {
        let key_a = StaticKeypair::generate().expect("key A");
        let key_b = StaticKeypair::generate().expect("key B");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");

        let (code_tx_a, code_rx_a) = std::sync::mpsc::channel::<String>();
        let (code_tx_b, code_rx_b) = std::sync::mpsc::channel::<String>();
        let accept_side = async {
            let (stream, _) = listener.accept().await.expect("accept");
            Link::accept(stream, &key_b, &[], move |code| {
                code_tx_b.send(code.to_string()).expect("record code");
                true
            })
            .await
        };
        let connect_side = Link::connect(addr, &key_a, &[], move |code| {
            code_tx_a.send(code.to_string()).expect("record code");
            true
        });

        let (connect_res, accept_res) = tokio::join!(connect_side, accept_side);
        let (mut source, pinned_for_source) = connect_res.expect("connect pairs");
        let (mut sink, pinned_for_sink) = accept_res.expect("accept pairs");

        // Each side returns the other's static key for pinning.
        assert_eq!(pinned_for_source, key_b.public_key());
        assert_eq!(pinned_for_sink, key_a.public_key());
        // Both Peers saw the same 6-digit Pairing code.
        let code_a = code_rx_a.recv().expect("code A");
        assert_eq!(code_a, code_rx_b.recv().expect("code B"));
        assert_eq!(code_a.len(), 6);

        // Frames flow both ways over the encrypted Link.
        source
            .send_frame(&Frame::Ping { t: 7 })
            .await
            .expect("send ping");
        assert_eq!(
            sink.recv_frame().await.expect("recv ping"),
            Frame::Ping { t: 7 }
        );
        sink.send_frame(&Frame::ClaimSource { seq: 1 })
            .await
            .expect("send claim");
        assert_eq!(
            source.recv_frame().await.expect("recv claim"),
            Frame::ClaimSource { seq: 1 }
        );
    }

    /// Two Links paired over loopback (dialer, acceptor).
    async fn paired() -> (Link, Link) {
        let key_a = StaticKeypair::generate().expect("key A");
        let key_b = StaticKeypair::generate().expect("key B");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let accept_side = async {
            let (stream, _) = listener.accept().await.expect("accept");
            Link::accept(stream, &key_b, &[], |_| true).await
        };
        let (connect_res, accept_res) =
            tokio::join!(Link::connect(addr, &key_a, &[], |_| true), accept_side);
        let (source, _) = connect_res.expect("connect pairs");
        let (sink, _) = accept_res.expect("accept pairs");
        (source, sink)
    }

    #[tokio::test]
    async fn send_frame_wire_is_be_length_then_ciphertext() {
        // The on-wire format is shared with already-shipped peers: a u32
        // big-endian length, then that many Noise ciphertext bytes.
        let (mut source, mut sink) = paired().await;
        source
            .send_frame(&Frame::Ping { t: 7 })
            .await
            .expect("send ping");
        let len = sink.stream.read_u32().await.expect("length") as usize;
        let mut cipher = vec![0u8; len];
        sink.stream
            .read_exact(&mut cipher)
            .await
            .expect("ciphertext");
        let mut body = vec![0u8; len];
        let body_len = sink
            .transport
            .read_message(&cipher, &mut body)
            .expect("decrypt");
        assert_eq!(
            decode_frame(&body[..body_len]).expect("decode"),
            Frame::Ping { t: 7 }
        );
    }

    #[tokio::test]
    async fn send_frame_survives_cancellation_with_full_socket() {
        // The peer is not reading, so the socket buffers fill and a send
        // blocks partway; a select/timeout then drops it.
        let (mut source, mut sink) = paired().await;
        let big = |n: usize| Frame::Clipboard {
            text: format!("{n:06}{}", "x".repeat(60_000)),
        };
        let mut sent = 0;
        let mut cancelled = false;
        while sent < 10_000 {
            let frame = big(sent);
            let send = source.send_frame(&frame);
            let outcome = tokio::time::timeout(std::time::Duration::from_millis(20), send).await;
            sent += 1;
            match outcome {
                Ok(result) => result.expect("send before the buffer fills"),
                Err(_) => {
                    cancelled = true;
                    break;
                }
            }
        }
        assert!(cancelled, "socket buffers never filled");
        assert!(
            !source.write_buf.is_empty(),
            "the cancel must land mid-frame for this test to mean anything"
        );

        // The cancelled frame was committed: it goes out ahead of the next
        // one, and the peer reads every frame intact and in order.
        let last = source.send_frame(&Frame::Ping { t: 9 });
        let read_all = async {
            for n in 0..sent {
                assert_eq!(sink.recv_frame().await.expect("big frame"), big(n));
            }
            sink.recv_frame().await.expect("frame after cancelled send")
        };
        let both = async { tokio::join!(last, read_all) };
        let (last, after) = tokio::time::timeout(std::time::Duration::from_secs(10), both)
            .await
            .expect("stream desynced: the peer is stuck mid-frame");
        last.expect("send after cancelled send");
        assert_eq!(after, Frame::Ping { t: 9 });
    }

    #[tokio::test]
    async fn full_size_clipboard_frame_crosses_and_link_stays_up() {
        // Noise caps one message at 65535 bytes; a 1 MiB clipboard frame
        // must still arrive whole, and the Link must keep working after.
        let (mut source, mut sink) = paired().await;
        let big = Frame::Clipboard {
            text: "x".repeat(crate::platform::CLIPBOARD_MAX_BYTES),
        };
        let both = async { tokio::join!(source.send_frame(&big), sink.recv_frame()) };
        let (sent, received) = tokio::time::timeout(std::time::Duration::from_secs(10), both)
            .await
            .expect("1 MiB frame stuck");
        sent.expect("send 1 MiB clipboard");
        assert_eq!(received.expect("recv 1 MiB clipboard"), big);
        source
            .send_frame(&Frame::Ping { t: 3 })
            .await
            .expect("send after big frame");
        assert_eq!(
            sink.recv_frame().await.expect("recv after big frame"),
            Frame::Ping { t: 3 }
        );
    }

    /// A Clipboard frame whose encoded body is exactly `len` bytes.
    fn clipboard_with_body(len: usize) -> Frame {
        let mut text = "x".repeat(len);
        while encode_frame(&Frame::Clipboard { text: text.clone() })
            .expect("encode")
            .len()
            > len
        {
            text.pop();
        }
        let frame = Frame::Clipboard { text };
        assert_eq!(encode_frame(&frame).expect("encode").len(), len);
        frame
    }

    #[tokio::test]
    async fn frames_that_fit_one_noise_message_keep_the_v1_wire() {
        // A body up to 65519 bytes is one Noise message: a v1 receiver's
        // single read_message over the whole payload must decrypt it. Larger
        // bodies are full 65535-byte messages plus one shorter tail.
        let (mut source, mut sink) = paired().await;
        let full = NOISE_MAX_MESSAGE - NOISE_TAG_LEN;
        for (body_len, messages) in [(full, 1), (full + 1, 2), (2 * full, 2), (2 * full + 1, 3)] {
            let frame = clipboard_with_body(body_len);
            let send = source.send_frame(&frame);
            let read = async {
                let len = sink.stream.read_u32().await.expect("length") as usize;
                let mut cipher = vec![0u8; len];
                sink.stream
                    .read_exact(&mut cipher)
                    .await
                    .expect("ciphertext");
                cipher
            };
            let both = async { tokio::join!(send, read) };
            let (sent, cipher) = tokio::time::timeout(std::time::Duration::from_secs(10), both)
                .await
                .expect("frame stuck");
            sent.expect("send");
            assert_eq!(cipher.len(), body_len + messages * NOISE_TAG_LEN);
            let mut body = Vec::new();
            for message in cipher.chunks(NOISE_MAX_MESSAGE) {
                let mut plain = vec![0u8; message.len()];
                let n = sink
                    .transport
                    .read_message(message, &mut plain)
                    .expect("each message decrypts on its own");
                body.extend_from_slice(&plain[..n]);
            }
            assert_eq!(body, encode_frame(&frame).expect("encode"), "{body_len}");
        }
    }

    #[tokio::test]
    async fn both_sides_sending_large_frames_at_once_do_not_deadlock() {
        // Each side writes more than the socket buffers hold before it reads
        // anything (two clipboards copied in the same poll window).
        let (a, b) = paired().await;
        let frames: Vec<Frame> = (b'a'..b'e')
            .map(|letter| Frame::Clipboard {
                text: char::from(letter)
                    .to_string()
                    .repeat(crate::platform::CLIPBOARD_MAX_BYTES),
            })
            .collect();
        let swap = |mut link: Link| {
            let frames = frames.clone();
            async move {
                for frame in &frames {
                    link.send_frame(frame).await.expect("send");
                }
                for frame in &frames {
                    assert_eq!(&link.recv_frame().await.expect("recv"), frame);
                }
            }
        };
        let both = async { tokio::join!(swap(a), swap(b)) };
        tokio::time::timeout(std::time::Duration::from_secs(30), both)
            .await
            .expect("both senders blocked: nobody reads while writing");
    }

    #[tokio::test]
    async fn tampered_multi_message_frame_is_refused() {
        let (mut source, mut sink) = paired().await;
        let body = encode_frame(&clipboard_with_body(100_000)).expect("encode");
        let mut cipher = seal(&mut source.transport, &body).expect("seal");
        *cipher.last_mut().expect("non-empty") ^= 1;
        source
            .stream
            .write_all(&(cipher.len() as u32).to_be_bytes())
            .await
            .expect("write length");
        source
            .stream
            .write_all(&cipher)
            .await
            .expect("write cipher");
        assert!(matches!(sink.recv_frame().await, Err(LinkError::Noise(_))));
    }

    #[tokio::test]
    async fn queued_frames_survive_a_cancelled_flush_in_order() {
        // More than the socket buffers hold, committed at once (one engine
        // output, or several); the flush is dropped part way, then resumed.
        let (mut source, mut sink) = paired().await;
        let frames: Vec<Frame> = (0..200)
            .map(|n| Frame::Clipboard {
                text: format!("{n:03}{}", "q".repeat(60_000)),
            })
            .collect();
        for frame in &frames {
            source.queue_frame(frame).expect("queue");
        }
        let cancelled = tokio::time::timeout(std::time::Duration::from_millis(50), source.flush())
            .await
            .is_err();
        assert!(
            cancelled && !source.write_buf.is_empty(),
            "flush never blocked"
        );
        let read_all = async {
            let mut got = Vec::new();
            for _ in 0..frames.len() {
                got.push(sink.recv_frame().await.expect("recv"));
            }
            got
        };
        let both = async { tokio::join!(source.flush(), read_all) };
        let (flushed, got) = tokio::time::timeout(std::time::Duration::from_secs(30), both)
            .await
            .expect("resumed flush stuck");
        flushed.expect("flush");
        assert_eq!(got, frames);
    }

    #[tokio::test]
    async fn paired_links_disable_nagle() {
        // Back-to-back input frames must not wait on each other's ACK; a
        // loopback timing test cannot show this reliably, so check the option.
        let (source, sink) = paired().await;
        assert!(source.stream.nodelay().expect("dialer nodelay"));
        assert!(sink.stream.nodelay().expect("acceptor nodelay"));
    }

    #[tokio::test]
    async fn rejecting_pairing_code_closes_without_frames() {
        let key_a = StaticKeypair::generate().expect("key A");
        let key_b = StaticKeypair::generate().expect("key B");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");

        let accept_side = async {
            let (stream, _) = listener.accept().await.expect("accept");
            Link::accept(stream, &key_b, &[], |_| false).await
        };
        let connect_side = Link::connect(addr, &key_a, &[], |_| true);

        let (connect_res, accept_res) = tokio::join!(connect_side, accept_side);
        assert!(
            matches!(accept_res, Err(LinkError::PairingRejected)),
            "rejecting side reports PairingRejected"
        );
        // The confirmer's side is closed too: no frame is ever delivered.
        let (mut link, _) = connect_res.expect("confirmer keeps its side");
        assert!(
            matches!(link.recv_frame().await, Err(LinkError::Closed)),
            "no frames flow after rejection"
        );
    }

    #[tokio::test]
    async fn pinned_link_refuses_impostor_and_hides_plaintext() {
        const MARKER: &str = "mdp-plaintext-marker-9f3k2j7h";
        let owner = StaticKeypair::generate().expect("owner key");
        let trusted = StaticKeypair::generate().expect("trusted key");
        let impostor = StaticKeypair::generate().expect("impostor key");
        let pinned = [trusted.public_key()];

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let peer_addr = listener.local_addr().expect("addr");
        let tap_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind tap");
        let tap_addr = tap_listener.local_addr().expect("tap addr");

        // Byte-tap relay: records every raw byte the impostor sends.
        let tap = Arc::new(Mutex::new(Vec::<u8>::new()));
        let _relay = tokio::spawn({
            let tap = Arc::clone(&tap);
            async move {
                let (client, _) = tap_listener.accept().await.expect("tap accept");
                let server = TcpStream::connect(peer_addr).await.expect("tap dial");
                let (mut client_read, mut client_write) = client.into_split();
                let (mut server_read, mut server_write) = server.into_split();
                let forward = async {
                    let mut buf = [0u8; 8192];
                    loop {
                        let len = client_read.read(&mut buf).await.expect("tap read");
                        if len == 0 {
                            break;
                        }
                        tap.lock().expect("tap lock").extend_from_slice(&buf[..len]);
                        server_write
                            .write_all(&buf[..len])
                            .await
                            .expect("tap write");
                    }
                    server_write.shutdown().await.expect("tap shutdown");
                };
                let backward = async {
                    tokio::io::copy(&mut server_read, &mut client_write)
                        .await
                        .expect("tap back");
                };
                tokio::join!(forward, backward);
            }
        });

        let accept_side = async {
            let (stream, _) = listener.accept().await.expect("accept");
            Link::accept(stream, &owner, &pinned, |_| true).await
        };
        let impostor_side = async {
            let (mut link, _) = Link::connect(tap_addr, &impostor, &[], |_| true)
                .await
                .expect("impostor handshakes");
            link.send_frame(&Frame::Clipboard {
                text: MARKER.to_string(),
            })
            .await
            .expect("impostor sends");
        };

        let (accept_res, ()) = tokio::join!(accept_side, impostor_side);
        assert!(
            matches!(accept_res, Err(LinkError::UnknownPeerKey)),
            "pinned Link refuses the impostor key"
        );

        tokio::time::sleep(Duration::from_millis(200)).await;
        let raw = tap.lock().expect("tap lock");
        assert!(!raw.is_empty(), "tap saw the impostor's bytes");
        assert!(
            !raw.windows(MARKER.len())
                .any(|window| window == MARKER.as_bytes()),
            "no frame travels unencrypted on the wire"
        );
    }
}
