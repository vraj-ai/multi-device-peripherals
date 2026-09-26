# Architecture Context

## Purpose

One mouse and one keyboard drive a Mac and a Windows laptop side by side. The
cursor crosses the shared screen edge like a second monitor, and the keyboard
follows the cursor. No re-pairing, no dongle swapping, no hotkeys needed.

## Locked Decisions

- **Build our own, lean.** Not a Deskflow/Input Leap wrapper or fork (ADR 0001).
- **Rust, one cargo workspace, one binary** (`mdp`) for both OSes. Platform code
  sits behind a `Platform` trait with `windows` and `macos` impls selected by
  `cfg`; pure logic (protocol, layout geometry, role arbitration, keymap) is
  platform-free and unit-tested on any host.
- **Symmetric peers.** Both machines run the same binary. The **Source** is
  chosen automatically: whichever peer sees physical (non-injected) input
  claims Source (ADR 0002). Injected events are always ignored by capture
  (Windows `LLMHF_INJECTED`/`LLKHF_INJECTED`; macOS event-source user-data tag).
- **Keys travel as physical USB HID usage codes.** The Sink injects them so its
  own OS layout and modifier conventions apply, as if the keyboard were plugged
  in there (Win key = Cmd on the Mac). No remap table in v1.
- **Transport:** TCP, one connection per pair, length-prefixed binary frames,
  encrypted with Noise `XX` (`snow` crate). First connect shows a 6-digit
  pairing code on both machines; after the user confirms, each side pins the
  peer's static key. Unknown keys are refused.
- **Discovery:** mDNS on the LAN (`_mdp._tcp`) plus a manual `host:port` in
  config (works with Tailscale MagicDNS names).
- **Layout:** each machine's **Desktop** is the bounding box of its monitors in
  logical points. The **Arrangement** (peer side + offset) is set by dragging
  in the GUI and saved to config. Both peers must hold the same Arrangement,
  mirrored; the last saved one wins and syncs over the link.
- **Clipboard:** UTF-8 text only, synced on change, capped at 1 MiB.
- **UI:** `eframe`/`egui` window plus a `tray-icon` tray/menu-bar icon. The
  visual design is locked in `CONTEXT/ui-design.md`.
- **Config:** one TOML file in the OS config dir (`directories` crate).
- **CI:** GitHub Actions runs `cargo test` + `cargo clippy -D warnings` on
  `windows-latest` and `macos-latest`. This is the only macOS build signal for
  agents working on Windows.

## Invariants

- **No stuck keys:** on every Crossing, disconnect, or Source change, the side
  losing focus releases every key and button it holds. Candidate check:
  `cargo test -p mdp-core stuck_keys`.
- **Disconnect returns control:** if the link drops while the cursor is remote,
  the cursor warps back to the local Desktop within 1 s of timeout and local
  input is un-suppressed.
- **Never loop injected input:** events the app injected are never re-captured
  and re-sent.
- **No plaintext input on the wire:** no input or clipboard frame is sent
  before the Noise handshake completes with a pinned peer.
- **Crossing latency:** local-LAN cursor motion added latency p95 < 10 ms,
  measured by the `latency` loopback bench.
- **Escape hatch:** a hard-coded chord (Ctrl+Alt+Shift+Esc on the Source)
  always returns the cursor and keyboard to the local machine.

## Non-goals

- More than two machines.
- Linux.
- File drag-and-drop, image/rich clipboard.
- Game-mode relative-mouse capture.
- Key remap tables or Ctrl<->Cmd swapping.
- Installers or auto-update. v1 ships a release binary plus documented
  login-item/startup setup.

## Accepted Boundaries

- macOS requires the user to grant Accessibility + Input Monitoring once. The
  app detects missing permission and shows a guided screen instead of failing
  silently.
- Real end-to-end Mac<->Windows behaviour can only be proven by the human on
  both machines. Agents prove logic with unit tests, Windows behaviour locally,
  and macOS compile+unit via CI.
- The app runs as a normal user process. It is not a service, so it can't
  control the Windows secure desktop (UAC/lock screen).

## Ownership

- `AGENTS.md` is the always-loaded project router.
- This file is human-owned and changes only when intent, decisions, invariants,
  non-goals, or accepted boundaries change.
- `CONTEXT/progress.md` is a bounded derived pointer, not a resume source.
- `goals` owns goal backlogs, handoffs, progress updates, and review verdicts.
