# Pairing

Pairing is the one-time check that both computers show the same 6-digit code. After both sides accept, each pins the other's key. A different machine is refused, and nothing is sent before that pin exists.

## Sub-features

- `show-code`: both sides display the same 6-digit code.
- `confirm`: "Codes match — pair" (or `y` on the terminal) pins the peer.
- `reject`: "They don't match" (or `n`) closes the link and sends no frames.
- `expire`: a code with no answer for 60 seconds counts as a rejection.
- `refuse-stranger`: a machine that is not the pinned peer cannot open a link, and the bytes on the wire do not contain the plaintext marker the test sends.

## How to get to it (user POV)

- Start `mdp` on both computers and wait until both show the pairing window. Compare the code. Choose **Codes match — pair** or **They don't match**.
- From a terminal, run `mdp pair`, read `Pairing code for …`, and answer `Pair with this peer? [y/n]:`.
- Tray: **Pair a new device…** to pair again, **Unpair** to forget the pin.

## Driving it with verify-mdp

Preconditions:

- `verify-mdp doctor` passed.
- Do not run `mdp pair` or the app. Both would rewrite the real config and bind port 24800.

- **Same code, then frames.** Run `.agents/skills/verify-mdp/verify-mdp drive pairing`. Exit code 0. The log shows `pairing_code_is_six_digits_and_stable` and `loopback_pairing_exchanges_frames_both_ways` as `ok`.
- **Reject.** The log shows `rejecting_pairing_code_closes_without_frames`, `mismatch_rejects`, and `expiry_rejects` as `ok`.
- **Window.** The log shows `shows_code_in_two_groups_and_confirms` as `ok` (the button labeled `Codes match — pair`).
- **Stranger.** The log shows `pinned_link_refuses_impostor_and_hides_plaintext` as `ok`.

## Gotchas

- The code is valid for 60 seconds (`Expires in` on the window). An expired code is a rejection, not a retry inside the same handshake.
- `mdp pair` deletes the pinned peer in the config before it listens. Verification must not run it against a home directory a person uses.
- Confirming on only one side does not pin the pair. The loopback test accepts on both.
- A live pairing window on two machines is a two-machine check. This drive does not open that window on a desktop.
