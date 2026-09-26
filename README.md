# mdp — one mouse and keyboard, two computers

`mdp` lets a Mac and a Windows PC sitting side by side share one mouse and one keyboard.
Push the cursor off the edge of one screen and it appears on the other, like a second
monitor. The keyboard follows the cursor. There's nothing to re-pair and no dongle to swap.

- **Plug the peripherals into either machine.** The machine that sees real input becomes
  the Source automatically.
- **Keys feel native on each OS.** They travel as physical keys, so on the Mac the Windows key
  acts as ⌘ and your Mac keyboard layout applies, exactly as if you'd plugged the keyboard in there.
- **Encrypted and paired.** Everything travels over a Noise-encrypted link. The first
  connection shows a 6-digit code on both screens, and after you confirm it no other device can connect.
- **Clipboard text syncs** both ways.
- **Always an escape hatch.** Press **Ctrl + Alt + Shift + Esc** on the machine with the
  keyboard to take the cursor and keyboard back instantly. If the network drops, control
  returns to the local machine on its own.

## Install

Download the latest build from
[Releases](https://github.com/vraj-ai/multi-device-peripherals/releases):

| OS | File |
|---|---|
| Windows 10/11 | `mdp-windows-x86_64.exe` (rename to `mdp.exe` if you like) |
| macOS 12+ (Apple silicon or Intel) | `mdp-macos-universal` |

On the Mac, the download is unsigned, so clear the quarantine flag once and make it executable:

```sh
xattr -d com.apple.quarantine ~/Downloads/mdp-macos-universal
chmod +x ~/Downloads/mdp-macos-universal
mv ~/Downloads/mdp-macos-universal /usr/local/bin/mdp
```

Or build from source (Rust stable): `cargo build --release -p mdp`.

## First run

1. **Start `mdp` on both machines.** Double-click `mdp.exe` on Windows; run `mdp` on the Mac.
   A tray / menu-bar icon appears.
2. **macOS permissions (Mac only, once).** mdp shows a setup screen until you grant
   **Accessibility** (to move the cursor and type) and **Input Monitoring** (to read the
   mouse and keyboard). **Open Settings** jumps to the right pane. The screen notices the
   grant by itself.
3. **Pair.** The two machines find each other on the same Wi-Fi. Both show the same 6-digit
   code. Check that they match and click **Codes match — pair** on both.
4. **Arrange.** In the Arrange window, drag the Mac's tile to the side it's really on
   (arrow keys work too), then click **Save**. The other machine picks up the same layout.

That's it. Move the cursor through the shared edge.

### Not on the same network?

If multicast discovery can't see the other machine (guest Wi-Fi, VPN, Tailscale), set
the peer by hand in the config file and restart mdp:

```toml
peer = "my-macbook.tailnet-name.ts.net:24800"   # or an IP address
```

The config lives at:
- Windows: `%APPDATA%\mdp\config\config.toml`
- macOS: `~/Library/Application Support/mdp/config.toml`

The file holds this machine's private pairing key, so keep it private.

## Tray menu

| Item | What it does |
|---|---|
| Share mouse & keyboard | Pause or resume crossing |
| Sync clipboard text | Pause or resume clipboard sync |
| Arrange displays… | Open the Arrange window |
| Pair a new device… / Unpair | Re-run pairing / forget the paired machine |
| Quit mdp | Stop |

The icon shows the state: **dashed grey** means not linked, **left square amber** means focus is on
this machine, and **right square amber** means focus is on the other machine.

**Start at login** is a checkbox in the Arrange window. It adds a `Run` entry on Windows or a
LaunchAgent on macOS.

## Troubleshooting

| Symptom | Fix |
|---|---|
| Cursor won't cross | Check the tray says *Linked*. Make sure the tiles in Arrange match your desk: the edge you push through must be the shared one. |
| Mac ignores input or the permissions screen won't clear | System Settings → Privacy & Security: re-add `mdp` under both Accessibility and Input Monitoring, then restart mdp. |
| Machines don't see each other | Allow `mdp` through the Windows firewall (private networks) and check TCP port 24800. Or set `peer` manually (above). |
| A key seems stuck | Press Ctrl + Alt + Shift + Esc. Every Crossing also releases all held keys. |
| Can't control the Windows login screen or UAC prompts | By design: mdp runs as your user, and Windows keeps those desktops isolated. |
| Check the input hooks work on this machine | `mdp selftest` injects a test event and confirms it isn't looped back. It exits 0 when healthy. |

## Commands

```
mdp            open the app (tray + windows)
mdp ui         same as above
mdp run        headless: share input without any window
mdp pair       pair from the terminal (prints the code, asks y/n)
mdp selftest   check capture/injection on this machine
```

## Development

Rust workspace with two crates: `mdp-core` (platform-free: protocol, encrypted Link,
Crossing engine, keymaps) and `mdp` (the binary: OS input hooks, config, discovery, UI).

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

CI runs all three on Windows and macOS. Design notes live in `CONTEXT/` (architecture,
glossary, ADRs, UI spec) and the visual mockups in `docs/design/`.
