//! The encrypted [`Link`] between two paired Peers.
//!
//! One TCP connection per pair. The Peers run Noise `XX` (`snow`), derive the
//! same 6-digit Pairing code from the handshake hash, and refuse unknown
//! static keys before any frame is delivered. After Pairing, frames move with
//! [`Link::send_frame`] / [`Link::recv_frame`]; every frame on the wire is
//! Noise-encrypted, never plaintext.

use crate::proto::{decode_frame, encode_frame, Frame, MAX_FRAME_BYTES};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, ToSocketAddrs};

/// Noise pattern for the Link: mutually authenticated, no prior knowledge.
pub const NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

/// Bytes Noise appends per encrypted frame (ChaChaPoly authentication tag).
pub const NOISE_TAG_LEN: usize = 16;

/// Largest accepted encrypted wire payload: 1 MiB frame body plus tag.
pub const MAX_WIRE_BYTES: usize = MAX_FRAME_BYTES + NOISE_TAG_LEN;

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
    stream: TcpStream,
    transport: snow::TransportState,
    peer_key: PeerKey,
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
    pub async fn send_frame(&mut self, frame: &Frame) -> Result<(), LinkError> {
        let body = encode_frame(frame)?;
        let mut cipher = vec![0u8; body.len() + NOISE_TAG_LEN];
        let len = self.transport.write_message(&body, &mut cipher)?;
        self.stream.write_u32(len as u32).await?;
        self.stream.write_all(&cipher[..len]).await?;
        self.stream.flush().await?;
        Ok(())
    }

    /// Receive and decrypt one frame.
    pub async fn recv_frame(&mut self) -> Result<Frame, LinkError> {
        let len = check_wire_len(self.stream.read_u32().await?)?;
        let mut cipher = vec![0u8; len];
        self.stream.read_exact(&mut cipher).await?;
        let mut body = vec![0u8; len];
        let len = self.transport.read_message(&cipher, &mut body)?;
        Ok(decode_frame(&body[..len])?)
    }
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
    let mut read_buf = vec![0u8; 65535];
    let mut msg_buf = vec![0u8; 65535];
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
            stream,
            transport,
            peer_key,
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
            "ciphertext over 1 MiB + tag must fail"
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
