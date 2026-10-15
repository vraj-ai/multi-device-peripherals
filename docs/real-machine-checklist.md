# Real-machine acceptance checklist (v1)

Run on real hardware: one Mac, one Windows PC, same LAN. Check each box as it
passes. Source: issue #1 (2026-09-27 snapshot).

## Mac bring-up gotchas

- A release build that ran out of disk leaves a non-executable
  `target/release/mdp` (`permission denied`). Free space, `cargo clean -p mdp`,
  rebuild with `cargo build --release -p mdp`, check `ls -l target/release/mdp`.
- `mdp: command not found`: run from the repo folder as
  `./target/release/mdp selftest` (or by full path, or add `target/release` to PATH).

## Steps

- [ ] 1. Mac: `cargo build --release -p mdp` (or tag a release). Grant
  Accessibility and Input Monitoring on the permissions screen. Run
  `mdp selftest` (must exit 0).
  - Expected: permissions screen clears once both grants are seen; selftest
    injects a test event, confirms it isn't looped back, exits 0.
  - Evidence: terminal output + exit code (`echo $?`); screenshot of the
    cleared permissions screen.

- [ ] 2. Both: run `mdp`. Confirm the 6-digit codes match on both screens.
  - Expected: both machines derive the same code (first 20 bits of SHA-256
    over the handshake hash, `pairing_code` in `crates/mdp-core/src/link.rs`);
    confirm on both sides and the keys pin.
  - Evidence: screenshot/photo of both screens showing the same code.

- [ ] 3. Arrange: drag the Mac tile to its real side and Save. The other
  machine should follow.
  - Expected: the receiver adopts the mirror (side flips, offset negates,
    `mirror_wire` in `crates/mdp-core/src/peer.rs`); last save wins on both.
  - Evidence: screenshots of both Arrange windows after save.

- [ ] 4. Cross both ways. Type on each side, including Win key → ⌘ on the Mac.
  - Expected: keys travel as physical USB HID usage codes with no
    character/layout remap: the HID usage is forwarded as-is and the
    receiving side maps it to local keycodes, so the OS layout applies
    (Win key acts as ⌘ on the Mac, `crates/mdp-core/src/keymap.rs`).
  - Evidence: note which machine holds the physical keyboard; typed text
    visible on the peer, both directions.

- [ ] 5. Copy text on each side and paste on the other.
  - Expected: UTF-8 text syncs on change both ways (1 MiB cap,
    `crates/mdp-core/src/peer.rs`).
  - Evidence: pasted text on the peer, both directions.

- [ ] 6. While the cursor is on the peer, press Ctrl+Alt+Shift+Esc. Control
  should come back.
  - Expected: the escape chord returns cursor and keyboard instantly; the chord
    Esc is consumed, not forwarded, and `come_home`/`set_focus` release held
    keys (`crates/mdp-core/src/crossing.rs`).
  - Evidence: cursor back on the local screen; confirm no stuck keys.

- [ ] 7. While the cursor is on the peer, turn Wi-Fi off. Control should return
  within about 1 s.
  - Expected: link loss runs the disconnect actions via `Peer::finish` and the
    cursor warps to the pre-cross position, un-suppressed
    (`Peer::finish` in `crates/mdp-core/src/peer.rs`, link-drop path in
    `crates/mdp-core/src/crossing.rs`). ~1 s is observed timing on a
    local-interface drop; there is no code guarantee, so note which side
    dropped Wi-Fi.
  - Evidence: observed time until local control returns; note it down.
