# Clipboard

Clipboard sync copies UTF-8 text from one computer to the other while they are linked. Text that was already copied before the link came up stays put. Turning sync off, or copying more than `CLIPBOARD_MAX_BYTES` (4 bytes under 1 MiB, so the frame fits), sends nothing.

## Sub-features

- `sync-text`: text copied on one side appears on the other, once.
- `no-echo`: the copy does not bounce forever.
- `already-copied`: text on the clipboard at connect time is not pushed.
- `toggle-off`: with sync disabled, a new copy stays local.
- `oversize`: a payload bigger than `CLIPBOARD_MAX_BYTES` is not sent.

## How to get to it (user POV)

- Tray item **Sync clipboard text** leaves sync on. Copy text on one computer and paste on the other.
- Turn **Sync clipboard text** off and copy again. The other computer's clipboard stays as it was.
- Copy something, then start `mdp`. That earlier copy does not appear on the other computer. A copy made after the link is up does.

## Driving it with verify-mdp

Preconditions:

- `verify-mdp doctor` passed.
- This drive does not read or write the OS clipboard of the machine running it.

- **Sync once.** Run `.agents/skills/verify-mdp/verify-mdp drive clipboard`. Exit code 0. The log shows `clipboard_text_syncs_without_echo` as `ok`.
- **Already copied.** The log shows `connecting_does_not_push_what_was_already_copied` as `ok`.
- **Sync off and too big.** The log shows `clipboard_toggle_off_sends_nothing` and `oversize_clipboard_is_not_sent` as `ok`.

## Gotchas

- Images and rich clipboard data are out of scope. Only UTF-8 text moves.
- The cap is `CLIPBOARD_MAX_BYTES` in `crates/mdp-core/src/limits.rs`, 4 bytes under 1 MiB so the encoded frame fits `MAX_FRAME_BYTES`. A real desktop can hold a larger copy; that copy must not show up on the other side.
- An echo would keep the peers busy. The test waits two poll windows and requires both clipboards to still read the same text.
- Real OS clipboards on two machines are a two-machine check.
