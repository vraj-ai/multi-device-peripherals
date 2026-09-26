# Build our own KVM instead of wrapping or forking Deskflow

Deskflow, Input Leap, and Mouse Without Borders already do edge-crossing KVM. We build a lean Rust app scoped to exactly one Mac + one Windows pair, so every behaviour (role arbitration, HID passthrough, pairing UX) is ours to shape and the codebase stays small enough to own.

Consequences: we own the input-hook edge cases (stuck keys, injected-event loops, macOS permissions) that Deskflow already solved. If v1 stalls on those, falling back to wrapping Deskflow is the escape route.
