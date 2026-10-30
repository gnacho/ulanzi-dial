[English](README.md) | [Español](README.es.md)

# Ulanzi Dial

GTK4 + libadwaita (Rust) configurator for the **Ulanzi Dial D100H** on Linux.

![device map](assets/device.png)

## What it does

- Reads the dial through **evdev** (the kernel HID channel; the `0x1971` GATT
  service is not a real-time event channel, see AetherSDR #3232).
- **Exclusive grab** (`EVIOCGRAB`): the multimedia keys the firmware emits do
  not leak to other applications.
- Wheel (turn and press) and 7 buttons (3 on top + 2 per side) are assignable,
  each control with its own action.
- Actions go through **uinput** (virtual keyboard/mouse): volume, brightness,
  scroll, zoom, tracks, undo/redo, copy/paste, screenshot, lock, shortcuts and
  custom commands.
- On-screen HUD when turning (floating window; see the Wayland limitation
  below).
- Config at `~/.config/ulanzi-dial/config.toml` (readable TOML, no GSettings).

## Build and run

System dependencies:

```bash
# Fedora
sudo dnf install gtk4-devel libadwaita-devel rust cargo

# Debian/Ubuntu
sudo apt install libgtk-4-dev libadwaita-1-dev cargo
```

```bash
cargo run --release
```

## System install (optional)

```bash
install -Dm755 target/release/ulanzi-dial ~/.local/bin/ulanzi-dial
```

Menu entry at `~/.local/share/applications/dev.ulanzi.dial.desktop`:

```ini
[Desktop Entry]
Name=Ulanzi Dial
Comment=Ulanzi Dial D100H configurator (wheel and buttons)
Exec=ulanzi-dial
Icon=dev.ulanzi.dial
Type=Application
Categories=GNOME;GTK;Utility;
StartupNotify=true
```

The icon goes to
`~/.local/share/icons/hicolor/256x256/apps/dev.ulanzi.dial.png` (you can crop
it from `assets/device.png`).

## Permissions (required for evdev + uinput)

Without permissions the app cannot open `/dev/input/event*` nor write to
`/dev/uinput`.

If your user is already in the `input` group and `/dev/uinput` is
`root:input 660`, there is nothing to do: check with `id` and
`ls -l /dev/uinput`.

Otherwise, install the included udev rule:

```bash
sudo cp udev/99-ulanzi-dial.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Log out and back in (or reboot) for it to take effect.

## D100H key map

The default map was measured on a real D100H (capturing evdev exclusively,
2026-10-27):

| Control | Code it emits |
|---|---|
| Wheel right | `KEY_VOLUMEUP` |
| Wheel left | `KEY_VOLUMEDOWN` |
| Wheel press | `KEY_MUTE` |
| Top left | `KEY_PREVIOUSSONG` |
| Top center | `KEY_PLAYPAUSE` |
| Top right | `KEY_NEXTSONG` |
| Left side upper | `Ctrl+V` |
| Left side lower | `Ctrl+C` |
| Right side upper | `Ctrl+Y` |
| Right side lower | `Ctrl+Z` |

Three quirks of this firmware, measured with the device in hand:

- It releases Ctrl before the letter of the combo (the decoder resolves the
  release by the key, not by the Ctrl state).
- It does not report a held press on the wheel: it sends press and release
  back to back, so there is no long press.
- While a button is held the wheel emits nothing: wheel and buttons cannot be
  combined, so each one has a single action.

With the dial paired you can check the map on your unit:

```bash
evtest   # pick the "Ulanzi Dial Keyboard" device
```

If your unit differs, edit `~/.config/ulanzi-dial/config.toml`:

```toml
[keymap]
top-left = "Ctrl+X"
top-center = "Ctrl+C"
# keys: top-left, top-center, top-right,
#       side-left-upper, side-left-lower, side-right-upper, side-right-lower
```

## Volume

By default the wheel sends the system volume keys, so the desktop's native OSD
shows up and the step is whatever your desktop uses (6% on GNOME, adjustable
with `gsettings set org.gnome.settings-daemon.plugins.media-keys volume-step 3`).

If you prefer your own step and do not mind losing the desktop OSD, the dial
can talk to PipeWire (`wpctl`) or PulseAudio (`pactl`) directly and apply the
step you choose:

```toml
volume_mode = "mixer"   # "keys" (default) or "mixer"
volume_step = 3         # volume points per detent, mixer mode only
sensitivity = 5         # only affects scroll, horizontal scroll and zoom
```

In `mixer` mode the app's HUD shows the real volume returned by the mixer, not
an internal counter. With `keys` the HUD does not show for volume because the
desktop OSD already does it. Sensitivity never multiplies volume or track
changes: that was what made a single detent jump 30 points.

## Per-application profiles

Each application can have its own bindings for the seven buttons. The wheel
(turn and press) is always global: volume and mute mean the same thing
everywhere.

Profiles are implicit: with the application in the foreground, change a
binding in the Device tab and it is stored in that application's profile.
Buttons you do not touch keep using the global binding.

For the app to know which window is focused it needs a GNOME Shell extension,
because GNOME does not let a regular program query it (Mutter's introspection
API answers `AccessDenied` and Mutter does not implement
`wlr-foreign-toplevel-management`):

```bash
mkdir -p ~/.local/share/gnome-shell/extensions/ulanzi-dial-focus@gnacho
cp gnome-extension/metadata.json gnome-extension/extension.js \
   ~/.local/share/gnome-shell/extensions/ulanzi-dial-focus@gnacho/
# enable it in the Extensions app, or:
gnome-extensions enable ulanzi-dial-focus@gnacho
```

The extension publishes the focused application id on the
`dev.ulanzi.Dial.Focus` bus. After installing it you must log out and back in:
on Wayland GNOME Shell cannot be reloaded live. If the extension is missing or
disabled, the app works the same and stays on the global profile, with no
errors.

In the General tab you can pin a profile by hand (`Global` or one specific
application). The pinned profile wins over the focused window.

## Custom commands

Any button can run a shell command. In the app, pick "Ejecutar comando…"
(Run command) in a control's action menu and type it: the command runs through
your shell every time you press the control.

In `config.toml` the same binding is stored as a table:

```toml
[buttons.side-right-upper]
press = { command = "gnome-calculator" }
```

Recorded keyboard shortcuts are stored the same way:

```toml
[buttons.top-left]
press = { chord = "Ctrl+Shift+T" }
```

## Layout

```
src/
  main.rs            entry point (adw::Application)
  config.rs          config.toml: load/save, keymap overrides
  backend/
    keymap.rs        physical codes to logical identity (tested)
    decoder.rs       raw events to semantic gestures (tested)
    device.rs        evdev: detection, exclusive grab, hotplug, battery
    focus.rs         D-Bus listener for the focused window (GNOME extension)
    actions.rs       uinput executor + UI options (tested)
  ui/
    mod.rs           window, ViewSwitcher, banner, toasts, wiring
    device_page.rs   Device tab (status + map)
    control_map.rs   control map (photo + leader lines + cards)
    general_page.rs  General tab (wheel, behaviour, profile)
    chord_dialog.rs  keyboard shortcut recorder dialog
    hud.rs           on-screen indicator
assets/              D100H front photo
gnome-extension/     extension publishing the focused window
udev/                udev rule for input permissions
```

## Known limitations / roadmap

- **Wayland**: the HUD is a small regular window (no `gtk4-layer-shell`, so no
  real overlay). Moving the HUD to layer-shell is the first improvement
  candidate.
- **Per-application profiles**: they need the GNOME extension (see above),
  because GNOME does not expose the focused window to regular programs.
- **Fixed-size window** (1020x760): the map uses fixed geometry in v1.
- **Battery**: best-effort read from the sibling `ABS_MISC` node (0-255 range).
  If the firmware does not expose it cleanly, it simply does not show up.
- UI in imperative gtk4-rs; structure meant to migrate to **relm4**.

## Security

Please report vulnerabilities privately through the Security tab
([report a vulnerability](https://github.com/gnacho/ulanzi-dial/security/advisories/new))
instead of opening a public issue. See [SECURITY.md](SECURITY.md).

## Credits

- Protocol analysis and key map: AetherSDR #3232.
- Device photos: manufacturer material (Ulanzi), cropped for the map.
