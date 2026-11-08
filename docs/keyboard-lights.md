# Keyboard lights: the interface

On my laptop Lattice lights the keys that matter now, through `qs-rgbd`, a service of mine that
shares the keyboard's lights between programs. That service is tied to my laptop and isn't
released, but what Lattice asks of it is small, so if your keyboard lights its keys one by one
(OpenRGB, a QMK or ZMK board, a Razer or Corsair with a Linux driver) you can write something
that answers it. This is everything Lattice needs. What the colours mean is in the manual, under
[the keyboard's lights](manual/10-files-and-config.md).

Linux and macOS only: on Windows Lattice doesn't look for it. macOS doesn't set `$XDG_RUNTIME_DIR`,
so set it yourself, for Lattice and the service alike.

## The socket

Lattice connects to a Unix stream socket at `$XDG_RUNTIME_DIR/qs-rgb.sock` when it starts. If
nothing's there it carries on without lights, and tries again whenever the lights change, so
the service can start after Lattice does.

Requests are newline-delimited JSON, one object per line. **Answer every request with one line**
(anything will do; `{"ok":true}` is what `qs-rgbd` sends), because Lattice waits for it before
sending the next. If the connection closes, or a write or read fails, Lattice stops and
reconnects later.

When the connection closes, for any reason, including Lattice crashing or being killed, take its
lights off the keyboard. Lattice never sends anything to tidy up; it relies on this.

## What Lattice sends

All under one layer, called `lattice`, at priority `20`. If Lattice is the only thing lighting
your keyboard, you can ignore both.

**`set`** replaces every light Lattice has on the keyboard. Keys it leaves out aren't
Lattice's: show whatever you'd show without it, or leave them dark.

```json
{"cmd":"set","layer":"lattice","priority":20,"keys":{"h":"#0028c0","j":"#0028c0","esc":"#808080"}}
```

**`patch`** changes just the keys given and leaves the rest as they are. Lattice sends it when
the same keys stay lit and only their colours change, as when a note sounds or the beat moves on.

```json
{"cmd":"patch","layer":"lattice","keys":{"1":"#ffffff","2":"#202020"}}
```

**An animated key** comes as a `patch` with an object instead of a colour. The effects are
`breathe`, `pulse` and `blink`, and `period` is in seconds. A plain colour later stops it.

```json
{"cmd":"patch","layer":"lattice","keys":{"f3":{"color":"#ff0000","effect":"blink","period":0.25}}}
```

Colours are always `#rrggbb`. They're bright and saturated on purpose, for LEDs rather than a
screen; scale them down if yours are too bright.

## Key names

Lattice names the key a character is typed on, on a British layout: `"` lights `2`, `@` lights
`'`, and `~` lights `#`. The names are:

- `a` to `z`, `0` to `9`
- `` ` `` `-` `=` `[` `]` `;` `'` `#` `\` `,` `.` `/`
- `space` `esc` `enter` `backspace` `delete` `tab`
- `up` `down` `left` `right` `home` `end`
- `f1` to `f12`, `rightalt`

There's no numpad, so nothing lights there.

## A starting point

A whole service in Python, with no dependencies, that keeps the colours Lattice asks for and
hands them to `show`, which is the one part to write for your keyboard. It draws them in the
terminal, so you can see it working before you touch the hardware. It ignores animations and
draws them in their colour.

```python
#!/usr/bin/env python3
"""Answers Lattice's keyboard lights. Write `show` for your keyboard."""
import json, os, socket

PATH = os.path.join(os.environ["XDG_RUNTIME_DIR"], "qs-rgb.sock")

def show(keys):
    """Light each key in `keys` ({name: "#rrggbb"}) and turn the rest off."""
    print("\r\033[K" + " ".join(
        f"\033[48;2;{int(c[1:3], 16)};{int(c[3:5], 16)};{int(c[5:7], 16)}m {k} \033[0m"
        for k, c in sorted(keys.items())), end="", flush=True)

def colour(value):
    return value["color"] if isinstance(value, dict) else value

if os.path.exists(PATH):
    os.remove(PATH)
server = socket.socket(socket.AF_UNIX)
server.bind(PATH)
server.listen()
while True:
    conn, _ = server.accept()
    keys = {}
    for line in conn.makefile():
        request = json.loads(line)
        if request["cmd"] == "set":
            keys = {}
        keys.update({k: colour(v) for k, v in request.get("keys", {}).items()})
        show(keys)
        conn.sendall(b'{"ok":true}\n')
    conn.close()
    show({})
```

It serves one Lattice at a time, which is all it needs to.
