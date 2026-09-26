# multi-device-peripherals

A software KVM for exactly two machines, a Mac and a Windows laptop, sharing one
physical mouse and keyboard over the network.

## Language

**Peer**:
One of the two machines running `mdp`. Peers are symmetric.
_Avoid_: client, server, host

**Source**:
The Peer whose physical mouse/keyboard is producing input right now.
_Avoid_: server, master

**Sink**:
The Peer receiving and injecting input from the Source.
_Avoid_: client, slave, target

**Desktop**:
The bounding box of all of one Peer's monitors, in logical points.
_Avoid_: screen (a Desktop may span several screens)

**Arrangement**:
Where the other Peer's Desktop sits relative to this one: side + offset.
_Avoid_: layout, position config

**Crossing**:
The moment the cursor leaves the Source's Desktop through the shared edge and
control moves to the other Peer (or back).
_Avoid_: switch, hop, transition

**Focus**:
Which Peer currently receives keyboard and mouse input. It follows the cursor.
_Avoid_: active machine

**Pairing**:
The one-time confirmation of a 6-digit code that pins each Peer's static key.
_Avoid_: login, handshake (the Noise handshake happens on every connect)

**Link**:
The encrypted TCP connection between the two paired Peers.
_Avoid_: session, socket
