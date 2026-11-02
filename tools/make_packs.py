#!/usr/bin/env python3
"""Synthesize Keytone's bundled starter packs (all sounds are original and
procedurally generated, so they are free to redistribute).

    python3 tools/make_packs.py            # writes ./packs/<id>/...
"""
import json
import os
import wave
from pathlib import Path

import numpy as np
from scipy.signal import butter, sosfilt

SR = 44100
ROOT = Path(__file__).resolve().parent.parent / "packs"
rng = np.random.default_rng(1234)


def t(dur):
    return np.arange(int(SR * dur)) / SR


def env(dur, attack=0.0005, decay=0.03):
    x = t(dur)
    a = np.clip(x / max(attack, 1e-6), 0, 1)
    return a * np.exp(-x / decay)


def band(sig, lo=None, hi=None, order=2):
    if lo and hi:
        sos = butter(order, [lo, hi], btype="band", fs=SR, output="sos")
    elif lo:
        sos = butter(order, lo, btype="high", fs=SR, output="sos")
    else:
        sos = butter(order, hi, btype="low", fs=SR, output="sos")
    return sosfilt(sos, sig)


def noise(dur):
    return rng.standard_normal(int(SR * dur))


def tone(freq, dur, decay, phase=0.0):
    return np.sin(2 * np.pi * freq * t(dur) + phase) * env(dur, 0.0005, decay)


def pad(sig, dur):
    n = int(SR * dur)
    out = np.zeros(max(n, len(sig)))
    out[: len(sig)] = sig
    return out


def mix(*parts, dur=None):
    n = max(len(p) for p in parts) if dur is None else int(SR * dur)
    out = np.zeros(n)
    for p in parts:
        m = min(n, len(p))
        out[:m] += p[:m]
    return out


def delay(sig, secs):
    return np.concatenate([np.zeros(int(SR * secs)), sig])


def fade_out(sig, secs=0.004):
    n = min(len(sig), int(SR * secs))
    sig = sig.copy()
    sig[-n:] *= np.linspace(1, 0, n)
    return sig


def norm(sig, peak=0.85):
    m = np.max(np.abs(sig))
    return sig if m == 0 else sig / m * peak


def write(path, sig, peak=0.85):
    sig = fade_out(norm(sig, peak))
    data = (np.clip(sig, -1, 1) * 32767).astype("<i2").tobytes()
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(data)


# --------------------------------------------------------------------------- packs

def thock(body=210.0, bright=2200.0, dur=0.07, weight=1.0):
    n = band(noise(dur), hi=bright, order=2) * env(dur, 0.0003, 0.006)
    b = tone(body * rng.uniform(0.96, 1.04), dur, 0.018) * 0.9 * weight
    b2 = tone(body * 2.7, dur, 0.008) * 0.25
    return mix(n * 0.8, b, b2)


def thock_up(dur=0.03):
    return band(noise(dur), lo=900, hi=4000) * env(dur, 0.0002, 0.003) * 0.4


def clicky(dur=0.06):
    click = band(noise(0.01), lo=3000, hi=9000, order=3) * env(0.01, 0.0001, 0.0012)
    click2 = delay(band(noise(0.008), lo=2500, hi=7000) * env(0.008, 0.0001, 0.001) * 0.6, 0.004)
    body = thock(260, 3000, dur, 0.6) * 0.6
    return mix(click * 1.3, click2, delay(body, 0.006))


def clicky_up(dur=0.04):
    c = band(noise(0.01), lo=3000, hi=8000, order=3) * env(0.01, 0.0001, 0.0015)
    return mix(c * 0.7, delay(thock_up() * 0.6, 0.003))


def typebar(dur=0.14):
    strike = band(noise(dur), lo=600, hi=6000) * env(dur, 0.0002, 0.01)
    metal = sum(tone(f * rng.uniform(0.98, 1.02), dur, d) * a
                for f, d, a in [(1250, 0.03, 0.4), (2330, 0.02, 0.3), (3710, 0.015, 0.25), (5200, 0.01, 0.15)])
    thud = tone(140, dur, 0.02) * 0.5
    return mix(strike, metal, thud)


def carriage_bell(dur=1.2):
    bell = sum(tone(2093 * h, dur, 0.5 / h ** 0.6) * a for h, a in [(1, 1.0), (2.76, 0.35), (5.4, 0.18)])
    return mix(typebar(0.14) * 0.9, delay(bell * 0.7, 0.05), dur=dur)


def bubble(f0, f1, dur=0.06):
    x = t(dur)
    freq = f0 + (f1 - f0) * (1 - np.exp(-x / (dur / 4)))
    ph = 2 * np.pi * np.cumsum(freq) / SR
    return np.sin(ph) * env(dur, 0.001, dur / 3)


def blip(freq, dur=0.05, duty=0.5, slide=0.0):
    x = t(dur)
    f = freq * (1 + slide * x / dur)
    ph = np.cumsum(f) / SR
    sq = np.where((ph % 1.0) < duty, 1.0, -1.0)
    return sq * env(dur, 0.0005, dur / 2.5) * 0.6


def build(pack_id, meta, files, defaults, groups=None, keys=None, random=None):
    d = ROOT / pack_id
    d.mkdir(parents=True, exist_ok=True)
    for name, sig in files.items():
        write(d / name, sig)
    pack = {
        "schema": 1,
        "id": pack_id,
        **meta,
        "volume": 1.0,
        "trim_silence": True,
        "random": random or {"pitch_cents": 30, "gain_db": 1.5},
        "defaults": defaults,
        "groups": groups or {},
        "keys": keys or {},
    }
    (d / "pack.json").write_text(json.dumps(pack, indent=2) + "\n")
    print(f"  {pack_id}: {len(files)} clips")


def main():
    print(f"writing packs to {ROOT}")

    # 1. Soft Thock — deep, muffled, lubed-linear-switch vibe
    files = {f"key{i}.wav": thock(rng.uniform(190, 240)) for i in range(1, 5)}
    files.update({
        "space.wav": thock(120, 1400, 0.12, 1.4),
        "enter.wav": thock(150, 1800, 0.1, 1.3),
        "backspace.wav": thock(170, 1900, 0.09, 1.1),
        "mod.wav": thock(230, 1500, 0.06, 0.8),
        "up.wav": thock_up(),
    })
    build("soft-thock", {"name": "Soft Thock", "author": "Keytone",
                         "description": "Deep, muffled, lubed-linear thock."},
          files,
          defaults={"down": [f"key{i}.wav" for i in range(1, 5)], "up": ["up.wav"]},
          groups={"space": {"down": ["space.wav"]}, "enter": {"down": ["enter.wav"]},
                  "backspace": {"down": ["backspace.wav"]}, "modifiers": {"down": ["mod.wav"]}})

    # 2. Clicky — crisp click-jacket switches, with distinct release clicks
    files = {f"click{i}.wav": clicky() for i in range(1, 5)}
    files.update({f"release{i}.wav": clicky_up() for i in range(1, 3)})
    files["space.wav"] = mix(clicky() * 0.8, thock(150, 1800, 0.1, 1.2))
    files["enter.wav"] = mix(clicky(), thock(180, 2000, 0.1, 1.0) * 0.8)
    build("clicky", {"name": "Clicky", "author": "Keytone",
                     "description": "Crisp click-jacket switches with a sharp release."},
          files,
          defaults={"down": [f"click{i}.wav" for i in range(1, 5)], "up": ["release1.wav", "release2.wav"]},
          groups={"space": {"down": ["space.wav"]}, "enter": {"down": ["enter.wav"]}})

    # 3. Typewriter — type bars, a thumpy space bar and a carriage-return bell
    files = {f"bar{i}.wav": typebar() for i in range(1, 5)}
    files.update({
        "space.wav": mix(tone(110, 0.15, 0.03), band(noise(0.15), hi=900) * env(0.15, 0.0005, 0.01) * 0.6),
        "bell.wav": carriage_bell(),
        "shift.wav": mix(typebar(0.12) * 0.5, tone(90, 0.12, 0.03)),
    })
    build("typewriter", {"name": "Typewriter", "author": "Keytone",
                         "description": "Metal type bars, a thumpy space bar and a carriage-return bell on Enter."},
          files,
          defaults={"down": [f"bar{i}.wav" for i in range(1, 5)]},
          groups={"space": {"down": ["space.wav"]}, "enter": {"down": ["bell.wav"]},
                  "modifiers": {"down": ["shift.wav"]}},
          random={"pitch_cents": 40, "gain_db": 2.0})

    # 4. Bubble Pop — playful pops, pitch varies by key row
    files = {f"pop{i}.wav": bubble(f, f * 2.2) for i, f in enumerate([420, 480, 540, 600, 680], 1)}
    files.update({
        "space.wav": bubble(220, 520, 0.12),
        "enter.wav": mix(bubble(400, 900, 0.07), delay(bubble(600, 1300, 0.08), 0.07)),
        "backspace.wav": bubble(700, 300, 0.07),
        "mod.wav": bubble(300, 450, 0.05),
    })
    build("bubble-pop", {"name": "Bubble Pop", "author": "Keytone",
                         "description": "Playful water-drop pops. Space is a big bloop, Enter a double pop."},
          files,
          defaults={"down": [f"pop{i}.wav" for i in range(1, 6)]},
          groups={"space": {"down": ["space.wav"]}, "enter": {"down": ["enter.wav"]},
                  "backspace": {"down": ["backspace.wav"]}, "modifiers": {"down": ["mod.wav"]}},
          random={"pitch_cents": 80, "gain_db": 1.0})

    # 5. Retro 8-bit — square-wave blips
    notes = [523.25, 587.33, 659.25, 783.99, 880.0]
    files = {f"blip{i}.wav": blip(f, 0.045, 0.25) for i, f in enumerate(notes, 1)}
    files.update({
        "space.wav": blip(261.6, 0.07, 0.5),
        "enter.wav": mix(blip(523.25, 0.06), delay(blip(659.25, 0.06), 0.06), delay(blip(1046.5, 0.1), 0.12)),
        "backspace.wav": blip(660, 0.08, 0.5, slide=-0.5),
        "mod.wav": blip(392, 0.04, 0.125),
    })
    build("retro-8bit", {"name": "Retro 8-bit", "author": "Keytone",
                         "description": "Square-wave blips from an imaginary 1985 console."},
          files,
          defaults={"down": [f"blip{i}.wav" for i in range(1, 6)]},
          groups={"space": {"down": ["space.wav"]}, "enter": {"down": ["enter.wav"]},
                  "backspace": {"down": ["backspace.wav"]}, "modifiers": {"down": ["mod.wav"]}},
          random={"pitch_cents": 0, "gain_db": 1.0})


if __name__ == "__main__":
    main()
