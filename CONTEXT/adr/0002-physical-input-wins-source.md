# The Source is whichever Peer sees physical input

The mouse/keyboard may be plugged into either machine. Instead of a manual "this machine has the peripherals" toggle, both Peers capture all the time, and the one that observes non-injected input claims Source. Injected events are tagged and ignored, so the app never feeds its own output back into itself.

Consequences: a claim-arbitration rule is needed for simultaneous physical input on both machines. The most recent claim wins, and the loser releases held keys. The injected-event tag becomes a load-bearing invariant on both OSes.
