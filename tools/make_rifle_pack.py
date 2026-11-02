#!/usr/bin/env python3
"""Synthesize the "Rifle (AK-style)" pack. All sounds are procedurally
generated (no recordings), so the pack is free to redistribute.

    python3 tools/make_rifle_pack.py
"""
import json
import wave
from pathlib import Path

import numpy as np
from scipy.signal import butter, sosfilt

SR = 44100
OUT = Path(__file__).resolve().parent.parent / "packs" / "rifle-ak"
rng = np.random.default_rng(47)


def n(secs):
    return int(SR * secs)


def t(secs):
    return np.arange(n(secs)) / SR


def filt(x, kind, f, order=2):
    return sosfilt(butter(order, f, btype=kind, fs=SR, output="sos"), x)


def decay(secs, tau, attack=0.0002):
    x = t(secs)
    return np.clip(x / attack, 0, 1) * np.exp(-x / tau)


def place(dst, src, at):
    i = max(0, n(at))
    m = min(len(dst) - i, len(src))
    if m > 0:
        dst[i:i + m] += src[:m]


def shot(length=0.55, punch=1.0):
    out = np.zeros(n(length))
    # 1. supersonic crack: very short broadband burst
    crack = filt(rng.standard_normal(n(0.012)), "high", 1500) * decay(0.012, 0.0018, 0.00005)
    # 2. muzzle blast: saturated low-mid noise with a fast decay
    blast = filt(rng.standard_normal(n(0.25)), "low", 2500) * decay(0.25, 0.028, 0.0003)
    blast = np.tanh(blast * 3.0) * 0.8
    # 3. low "boom": falling pitch thump
    x = t(0.3)
    f = 45 + 110 * np.exp(-x / 0.03)
    boom = np.sin(2 * np.pi * np.cumsum(f) / SR) * decay(0.3, 0.07, 0.001) * 1.1 * punch
    # 4. bolt carrier cycling: two metallic clacks right after the shot
    def clack(level, lo, hi, tau):
        c = filt(rng.standard_normal(n(0.03)), "band", [lo, hi], 3) * decay(0.03, tau, 0.0001)
        ring = sum(np.sin(2 * np.pi * fr * t(0.03)) * a for fr, a in [(2900, 0.5), (4700, 0.3)]) * decay(0.03, 0.006)
        return (c + ring * 0.6) * level
    # 5. reflections / tail
    tail = filt(rng.standard_normal(n(length)), "band", [120, 1800], 2) * decay(length, 0.11, 0.004) * 0.35

    place(out, blast, 0.0)
    place(out, crack * 1.4, 0.0)
    place(out, boom, 0.0)
    place(out, clack(0.35, 1800, 6000, 0.004), 0.045 + rng.uniform(-0.004, 0.004))
    place(out, clack(0.25, 1500, 5000, 0.005), 0.075 + rng.uniform(-0.004, 0.004))
    place(out, tail, 0.01)
    place(out, filt(blast, "low", 900) * 0.18, 0.09)  # slap-back echo
    return out


def burst(rounds=3, interval=0.1):
    out = np.zeros(n(interval * (rounds - 1) + 0.6))
    for i in range(rounds):
        place(out, shot(0.55, 1.0 - 0.06 * i) * (1.0 - 0.05 * i), i * interval + rng.uniform(-0.003, 0.003))
    return np.tanh(out * 0.9)


def metal(at_len, freqs, tau, level=1.0):
    x = t(at_len)
    c = filt(rng.standard_normal(n(at_len)), "band", [1200, 7000], 3) * decay(at_len, 0.003, 0.0001)
    r = sum(np.sin(2 * np.pi * f * x) * a for f, a in freqs) * decay(at_len, tau)
    return (c + 0.5 * r) * level


def reload():
    out = np.zeros(n(0.75))
    place(out, metal(0.12, [(900, 0.6), (2100, 0.4)], 0.02, 0.6), 0.0)    # mag out
    place(out, metal(0.15, [(700, 0.6), (1600, 0.5)], 0.03, 0.9), 0.32)   # mag in
    place(out, metal(0.1, [(3100, 0.4), (5200, 0.3)], 0.01, 0.5), 0.36)   # latch
    return out


def charge():
    out = np.zeros(n(0.4))
    place(out, metal(0.12, [(1800, 0.5), (3300, 0.4)], 0.015, 0.8), 0.0)   # pull back
    slide = filt(rng.standard_normal(n(0.08)), "band", [2000, 6000]) * np.linspace(0.15, 0.05, n(0.08))
    place(out, slide, 0.04)
    place(out, metal(0.15, [(1500, 0.6), (2600, 0.5)], 0.02, 1.0), 0.13)   # slam forward
    return out


def casing():
    x = t(0.25)
    ping = sum(np.sin(2 * np.pi * f * x) * a for f, a in [(5400, 0.5), (7900, 0.3), (3100, 0.2)]) * decay(0.25, 0.04)
    out = np.zeros(n(0.4))
    place(out, ping * 0.6, 0.0)
    place(out, ping * 0.25, 0.11)  # bounce
    return out


def write(name, sig, peak=0.9):
    sig = sig / (np.max(np.abs(sig)) + 1e-9) * peak
    k = n(0.01)
    sig[-k:] *= np.linspace(1, 0, k)
    with wave.open(str(OUT / name), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes((np.clip(sig, -1, 1) * 32767).astype("<i2").tobytes())


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for i in range(1, 6):
        write(f"shot{i}.wav", shot())
    write("burst.wav", burst(3))
    write("full-auto.wav", burst(5))
    write("reload.wav", reload(), 0.7)
    write("charge.wav", charge(), 0.7)
    write("casing.wav", casing(), 0.35)
    pack = {
        "schema": 1,
        "id": "rifle-ak",
        "name": "Rifle (AK-style)",
        "author": "Keytone",
        "description": "Every key fires a round. Space = 3-round burst, Enter = reload, Backspace = charging handle.",
        "volume": 0.6,
        "trim_silence": True,
        "random": {"pitch_cents": 35, "gain_db": 1.5},
        "defaults": {"down": [f"shot{i}.wav" for i in range(1, 6)]},
        "groups": {
            "space": {"down": ["burst.wav"]},
            "enter": {"down": ["reload.wav"]},
            "backspace": {"down": ["charge.wav"]},
            "modifiers": {"down": [], "up": []},
        },
        "keys": {"Tab": {"down": ["full-auto.wav"]}},
    }
    (OUT / "pack.json").write_text(json.dumps(pack, indent=2) + "\n")
    print("wrote", OUT)


if __name__ == "__main__":
    main()
