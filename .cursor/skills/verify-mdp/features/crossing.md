# Crossing

Crossing is the cursor leaving one computer through the shared edge and showing up on the other, with the keyboard following. Coming back, or losing the link, returns control to the computer that still has the physical mouse.

## Sub-features

- `cross-out`: the cursor leaves through the shared edge and the other computer gains Focus.
- `keys-follow`: keys typed after the cross inject on the computer that has Focus, not on both.
- `cross-back`: moving back through the shared edge restores local Focus and unsuppresses local input.
- `claim-source`: physical input on the computer that does not have the mouse makes that computer the Source and releases held keys.
- `link-drop`: if the link dies while Focus is remote, local Focus returns within 1 second and the cursor warps back.
- `escape`: Ctrl+Alt+Shift+Esc on the Source brings Focus home.
- `stuck-keys`: a crossing releases keys and buttons the computer is holding.

## How to get to it (user POV)

- Place the two computers in Arrange so they share an edge, then push the cursor through that edge.
- Type while the cursor is on the other computer.
- Push the cursor back through the same edge.
- Touch the mouse or keyboard plugged into the other computer.
- Press Ctrl+Alt+Shift+Esc on the computer that has the physical keyboard.
- Disconnect the network while the cursor is on the other computer.

## Driving it with verify-mdp

Preconditions:

- `verify-mdp doctor` passed.
- No live `mdp` is using the user's config. This drive does not start one.

- **Cross and type.** Push through the right edge and type. Run `.cursor/skills/verify-mdp/verify-mdp drive crossing`. Exit code 0. The log shows `cursor_crosses_right_and_back_with_remote_only_injection` as `ok`: Focus goes remote, the remote side records the key, the local side records no injection, then Focus returns local.
- **Link drop.** The same command's log shows `kill_link_while_remote_restores_focus_within_1s` as `ok` (local Focus restored inside 1 second) and `disconnect_while_remote_warps_back_and_unsuppresses` as `ok`.
- **Other computer takes over.** The log shows `physical_input_on_sink_claims_source` as `ok` and `stuck_keys_released_on_crossing` as `ok`.
- **Escape hatch.** The log shows `escape_chord_restores_local_focus` as `ok`.

## Gotchas

- A cursor outside the overlapping stretch of the shared edge does not cross.
- While Focus is remote the local cursor stays frozen on the edge. Motion sent across is the raw report minus that frozen point.
- `drive crossing` proves the loopback peers. It does not move a hardware cursor between two machines. That item stays in the two-machine list.
- The escape chord's Esc is consumed, not typed on the other computer.
