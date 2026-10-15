# Arrangement

Arrangement is where the other computer's screen sits: a side (left, right, top, bottom) and an offset along the shared edge. Dragging the tile and choosing Save stores it. The other computer adopts the mirror, and the link reconnects so both sides agree.

## Sub-features

- `drag-save`: dragging the other computer to the right and choosing Save stores the right side.
- `arrows`: arrow keys move the tile; the placement can be reverted.
- `mirror`: the other computer's saved side is the opposite side, and the offset flips sign.
- `cross-top`: a top edge crossing warps the cursor onto the other computer's bottom edge.
- `reconnect`: saving an arrangement ends the current session on both computers.

## How to get to it (user POV)

- Tray: **Arrange displays…**, or launch `mdp` / `mdp ui`, which opens the Arrange window.
- Drag the other computer's tile to the side it really sits on, or use the arrow keys.
- Choose **Save**.
- The **Start at login** checkbox is on this same window.

## Driving it with verify-mdp

Preconditions:

- `verify-mdp doctor` passed.
- Do not open the live Arrange window against the user's config.

- **Place and save.** Run `.cursor/skills/verify-mdp/verify-mdp drive arrangement`. Exit code 0. The log shows `drag_right_then_save_emits_right` and `arrow_keys_revert_and_receive` as `ok`.
- **Mirror and cross.** The log shows `mirror_flips_side_and_offset` and `arrangement_top_crosses_with_mirrored_wire_frames` as `ok`.
- **Session ends.** The log shows `saved_arrangement_ends_both_sessions_with_the_senders_view` as `ok`.

## Gotchas

- Both computers must store the mirrored placement. A mismatch prints a warning at hello time and crossings can miss the edge.
- The last Save wins and is what the next hello agrees on.
- Save ends the live session on purpose so both sides reconnect with the new edge.
- **Start at login** writes a Windows Run value or a macOS LaunchAgent. The headless drive does not toggle that checkbox on a real OS, and a Linux host returns an unsupported error for it. Reboot persistence is a two-machine (or at least one real Peer OS) check.
