# UI design (locked)

Visual source of truth: the mockups in `docs/design/*.dc.html` (open them in a browser, or the canvas
https://claude.ai/artifact/G2eTpZTY4qBvnJy8LYQWbM). Implement them in egui/eframe + tray-icon. Match the
tokens exactly; layout within ±4 px.

## Look: "instrument panel"

Dark graphite ground, one signal-amber accent that always means **Focus**, and teal that always means a
healthy **Link**. No gradients, no emoji. Icons are stroke glyphs.

## Tokens

| Token | Hex | Use |
|---|---|---|
| bg | #121416 | window ground |
| canvas | #16191C | arrange area (20 px dot grid #262A30) |
| panel | #1B1E22 | side panel, dialogs |
| raised | #23272C | menus, local Desktop tile |
| border | #30353C | hairlines |
| border-strong | #4A525C | secondary button, local tile outline |
| text | #ECE8E1 | primary text |
| text-2 | #C5CBD2 | body on panels |
| muted | #A0A6AE | labels, captions |
| accent | #F0A23B | Focus, primary button, shared edge |
| on-accent | #1A1206 | text on accent |
| accent-bg | #3A2A12 / #231D14 | Focus pill / attention card fill |
| link | #5CC2B5 | Link-up dot, granted state |
| peer tile | #394350 fill, #8A96A4 border | the other Peer's Desktop |

Type: IBM Plex Sans (400/500/600) for UI, IBM Plex Mono (400/500) for numbers, codes, and hosts.
Bundle both TTFs (OFL) via `egui::FontDefinitions`. Sizes: 12 caps label (0.08em tracking), 13 caption,
14 body, 15-16 emphasis, 22-24 titles, 52 pairing code. Radius: 6 controls, 8 cards, 10 menus.
Touch targets ≥ 44 px (menu rows 40 px).

## Screens

1. **Arrange** (960×620): header with the wordmark, a Link pill (dot + "Linked to <peer>" + mono
   "encrypted · N ms"), and a Focus pill. On the left, the canvas: the local Desktop tile is fixed and centered, and
   the peer tile is draggable. On release it snaps to the nearest side; the offset along the edge is clamped so the tiles
   overlap ≥ 24 px. Arrow keys set the side. An amber 4 px glowing bar marks the shared edge. On the right, a 272 px panel:
   the Arrangement readout ("Mac is LEFT of this PC", mono offset in pt), checkboxes for Share mouse & keyboard,
   Sync clipboard text, and Start at login, the escape-chord card, then Revert / Save (primary). Tile sizes are
   proportional to each Desktop's real logical size.
2. **Pairing** (480×620): caps label "New device found", title "Pair with <peer>?", the 6-digit
   code in two mono groups of 3 (second group amber), peer host + IP, a 60 s expiry countdown, primary
   "Codes match — pair", secondary "They don't match". Both machines show this screen at once.
3. **Tray**: a glyph of two overlapping rectangles with three states: dashed grey = no peer; left filled
   amber = Focus here; right filled amber = Focus on peer. Menu: status header (peer + mono latency +
   focus), Share mouse & keyboard, Sync clipboard text, Arrange displays…, Pair a new device…,
   Unpair <peer>, Quit.
4. **macOS permissions** (620×560): one card per permission (Accessibility, Input Monitoring). Granted =
   teal check. Missing = amber-bordered card with "Open Settings" (deep-links to the Privacy pane). Plus a
   "Check again" button and a privacy note. Shown instead of Arrange while any permission is missing.
