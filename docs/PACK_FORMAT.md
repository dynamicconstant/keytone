# Keytone sound-pack format (schema 1)

A pack is a folder containing `pack.json` and audio files. Exported packs are
the same folder zipped, with the extension `.keytone`.

```
typewriter/
├── pack.json
├── bar1.wav  bar2.wav  bar3.wav  bar4.wav
├── space.wav
└── bell.wav
```

Supported audio: WAV, MP3, OGG/Vorbis, FLAC, AAC/M4A. Individual clips must be
≤ 10 s; a sprite file may be up to 5 min.

## `pack.json`

```jsonc
{
  "schema": 1,
  "id": "typewriter",              // informational; the folder name is the real id
  "name": "Typewriter",
  "author": "you",
  "description": "Metal type bars and a bell on Enter.",
  "volume": 1.0,                   // pack loudness (0–2), on top of the master volume
  "trim_silence": true,            // strip leading silence from clips at load
  "random": {
    "pitch_cents": 40,             // ± random pitch per press (100 cents = 1 semitone)
    "gain_db": 2.0                 // ± random loudness per press
  },

  "defaults": { "down": ["bar1.wav", "bar2.wav", "bar3.wav", "bar4.wav"] },
  "groups": {
    "space":     { "down": ["space.wav"] },
    "enter":     { "down": ["bell.wav"] },
    "modifiers": { "down": [] }     // empty list = silent
  },
  "keys": {
    "Backspace": { "down": ["bar1.wav"], "up": ["bar2.wav"] }
  },

  // Optional: single-file packs (Mechvibes style). Reference a segment as "#name".
  "sprite": {
    "file": "sound.ogg",
    "segments": { "KeyA": [1200, 180] }  // [start_ms, duration_ms]
  }
}
```

### Resolution

For each key event (`down` = press, `up` = release):

1. `keys[<code>].<dir>` if set
2. else `groups[<group of code>].<dir>` if set
3. else `defaults.<dir>` if set
4. else silent

A field that is absent inherits; an empty list (`[]`) is explicitly silent.
When a list has several clips, one is picked at random per press (never the
same one twice in a row).

### Key codes

W3C [`KeyboardEvent.code`](https://developer.mozilla.org/en-US/docs/Web/API/UI_Events/Keyboard_event_code_values)
names — positional, independent of keyboard layout: `KeyA`…`KeyZ`,
`Digit0`…`Digit9`, `Space`, `Enter`, `Backspace`, `Tab`, `Escape`,
`ShiftLeft`, `ShiftRight`, `ControlLeft`, `AltLeft`, `MetaLeft`, `CapsLock`,
`Fn`, `ArrowUp`, `Home`, `F1`…`F20`, `Numpad0`…`Numpad9`, `NumpadEnter`,
`Minus`, `Equal`, `BracketLeft`, `Semicolon`, `Quote`, `Backquote`, `Comma`,
`Period`, `Slash`, `Backslash`, …

### Groups

| group | keys |
|---|---|
| `letters` | `KeyA`–`KeyZ` |
| `digits` | `Digit0`–`Digit9` |
| `punctuation` | `` ` - = [ ] \ ; ' , . / `` |
| `modifiers` | Shift, Control, Alt/Option, Meta/Cmd/Win, CapsLock, Fn |
| `space` | Space |
| `enter` | Enter, NumpadEnter |
| `backspace` | Backspace, Delete |
| `tab`, `escape` | themselves |
| `arrows` | the four arrow keys |
| `navigation` | Home, End, PageUp, PageDown, Insert |
| `function` | F1–F20 |
| `numpad` | numpad keys except NumpadEnter |
| `other` | everything else |

## Mechvibes import

Import a Mechvibes pack folder (or zip) and Keytone converts it: `single`
packs become a sprite, `multi` packs copy their files. Keys the pack doesn't
define fall back to its `A` sound.
