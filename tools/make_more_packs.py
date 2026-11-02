#!/usr/bin/env python3
"""Synthesize Keytone's themed sound packs. Every sound is generated from
scratch (no recordings, no samples), so the packs are free to redistribute.

    python3 tools/make_more_packs.py            # all packs
    python3 tools/make_more_packs.py piano rain # just some
"""
import json
import sys
import wave
from pathlib import Path

import numpy as np
from scipy.signal import butter, sosfilt

SR = 44100
ROOT = Path(__file__).resolve().parent.parent / "packs"
rng = np.random.default_rng(2026)

# ----------------------------------------------------------------------------- helpers


def n(s):
    return max(1, int(SR * s))


def t(s):
    return np.arange(n(s)) / SR


def env(s, tau, attack=0.0005):
    x = t(s)
    return np.clip(x / max(attack, 1e-6), 0, 1) * np.exp(-x / tau)


def filt(x, kind, f, order=2):
    return sosfilt(butter(order, f, btype=kind, fs=SR, output="sos"), x)


def noise(s):
    return rng.standard_normal(n(s))


def sine(f, s, phase=0.0):
    return np.sin(2 * np.pi * f * t(s) + phase)


def chirp(f0, f1, s, shape="exp"):
    x = t(s) / s
    f = f0 * (f1 / f0) ** x if shape == "exp" else f0 + (f1 - f0) * x
    return np.sin(2 * np.pi * np.cumsum(f) / SR)


def square(f, s, duty=0.5):
    ph = (np.cumsum(np.full(n(s), f) if np.isscalar(f) else f) / SR) % 1.0
    return np.where(ph < duty, 1.0, -1.0)


def saw_f(freqs):
    ph = (np.cumsum(freqs) / SR) % 1.0
    return 2 * ph - 1


def mix(length, *parts):
    """parts: (signal, start_seconds, gain)"""
    out = np.zeros(n(length))
    for sig, at, g in parts:
        i = max(0, n(at)) if at > 0 else 0
        m = min(len(out) - i, len(sig))
        if m > 0:
            out[i:i + m] += sig[:m] * g
    return out


def note(name_or_midi):
    m = name_or_midi
    return 440.0 * 2 ** ((m - 69) / 12)


PENTA = [0, 2, 4, 7, 9]


def penta_midi(i, base=48):
    return base + 12 * (i // 5) + PENTA[i % 5]


ROWS = {
    "bottom": ["KeyZ", "KeyX", "KeyC", "KeyV", "KeyB", "KeyN", "KeyM", "Comma", "Period", "Slash"],
    "home": ["KeyA", "KeyS", "KeyD", "KeyF", "KeyG", "KeyH", "KeyJ", "KeyK", "KeyL", "Semicolon", "Quote"],
    "top": ["KeyQ", "KeyW", "KeyE", "KeyR", "KeyT", "KeyY", "KeyU", "KeyI", "KeyO", "KeyP", "BracketLeft", "BracketRight"],
    "digits": ["Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9", "Digit0", "Minus", "Equal"],
}
ROW_START = {"bottom": 0, "home": 2, "top": 4, "digits": 6}

# Per-pack loudness so packs sit at a similar perceived level.
LEVEL = {"arcade-coins": 0.45, "telegraph": 0.6, "piano": 0.6, "marimba": 0.6, "campfire": 1.7,
         "bubble-wrap": 1.3, "laser-blaster": 0.8, "robot-beeps": 0.8, "rubber-duck": 0.75, "game-show": 0.8}

# ----------------------------------------------------------------------------- writer


class Pack:
    def __init__(self, pid, name, description, random=(30, 1.5), volume=1.0):
        self.id, self.dir = pid, ROOT / pid
        self.dir.mkdir(parents=True, exist_ok=True)
        for old in self.dir.glob("*.wav"):
            old.unlink()
        self.meta = {
            "schema": 1, "id": pid, "name": name, "author": "Keytone", "description": description,
            "volume": volume, "trim_silence": True,
            "random": {"pitch_cents": random[0], "gain_db": random[1]},
            "defaults": {}, "groups": {}, "keys": {},
        }

    def wav(self, name, sig, peak=0.85):
        sig = np.asarray(sig, dtype=float)
        m = np.max(np.abs(sig))
        if m > 0:
            sig = sig / m * peak
        k = min(len(sig), n(0.006))
        sig[-k:] *= np.linspace(1, 0, k)
        with wave.open(str(self.dir / name), "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(SR)
            w.writeframes((np.clip(sig, -1, 1) * 32767).astype("<i2").tobytes())
        return name

    def many(self, stem, count, fn, peak=0.85):
        return [self.wav(f"{stem}{i}.wav", fn(i), peak) for i in range(1, count + 1)]

    def default(self, down=None, up=None):
        if down is not None:
            self.meta["defaults"]["down"] = down
        if up is not None:
            self.meta["defaults"]["up"] = up

    def group(self, g, down=None, up=None):
        s = self.meta["groups"].setdefault(g, {})
        if down is not None:
            s["down"] = down
        if up is not None:
            s["up"] = up

    def key(self, code, down=None, up=None):
        s = self.meta["keys"].setdefault(code, {})
        if down is not None:
            s["down"] = down
        if up is not None:
            s["up"] = up

    def silent_modifiers(self):
        self.group("modifiers", down=[], up=[])

    def save(self):
        self.meta["volume"] = LEVEL.get(self.id, self.meta["volume"])
        (self.dir / "pack.json").write_text(json.dumps(self.meta, indent=2) + "\n")
        print(f"  {self.id}: {len(list(self.dir.glob('*.wav')))} clips")


def musical_keys(p, make_note, prefix):
    """Map every letter/digit/punctuation key to a pentatonic note, rising left
    to right and bottom row to number row, so typing plays a tune."""
    made = {}
    for row, codes in ROWS.items():
        for j, code in enumerate(codes):
            idx = ROW_START[row] + j
            if idx not in made:
                made[idx] = p.wav(f"{prefix}{idx:02d}.wav", make_note(note(penta_midi(idx))))
            p.key(code, down=[made[idx]])
    return made

# ----------------------------------------------------------------------------- keyboards & machines


def buckling_spring():
    p = Pack("buckling-spring", "Buckling Spring", "Loud, crunchy 1980s buckling-spring board: a spring ping on every press and release.", (25, 1.5))

    def press(i, weight=1.0):
        buckle = filt(noise(0.008), "band", [2500, 8000], 3) * env(0.008, 0.0012, 0.0001)
        ping = (sine(2150 * rng.uniform(0.95, 1.05), 0.06) + 0.5 * sine(3900, 0.06)) * env(0.06, 0.012)
        bottom = filt(noise(0.06), "low", 2000) * env(0.06, 0.006) + sine(180 * weight, 0.06) * env(0.06, 0.012) * 0.8 * weight
        rattle = filt(noise(0.04), "band", [1500, 4000]) * env(0.04, 0.01) * 0.2
        return mix(0.12, (buckle, 0, 1.3), (ping, 0.001, 0.35), (bottom, 0.012, 1.0), (rattle, 0.02, 1))

    def release(i):
        c = filt(noise(0.006), "band", [2000, 7000], 3) * env(0.006, 0.001, 0.0001)
        ping = sine(2300 * rng.uniform(0.95, 1.05), 0.05) * env(0.05, 0.01)
        return mix(0.06, (c, 0, 0.8), (ping, 0.001, 0.25))

    p.default(down=p.many("press", 5, press), up=p.many("release", 3, release, 0.5))
    p.group("space", down=[p.wav("space.wav", mix(0.2, (press(0, 0.7), 0, 1), (filt(noise(0.12), "band", [300, 2000]) * env(0.12, 0.03), 0.015, 0.6)))])
    p.group("enter", down=[p.wav("enter.wav", press(0, 0.8))])
    p.save()


def bell(f, s=1.2, tau=0.5):
    return sum(sine(f * h, s) * a * env(s, tau / h ** 0.5, 0.001) for h, a in [(1, 1), (2.76, 0.4), (5.4, 0.2), (8.9, 0.1)])


def cash_register():
    p = Pack("cash-register", "Cash Register", "Chunky mechanical register keys. Enter rings the drawer open: ka-ching!", (20, 1.5))

    def key(i):
        clack = filt(noise(0.03), "band", [800, 5000]) * env(0.03, 0.004)
        metal = (sine(1700 * rng.uniform(0.9, 1.1), 0.08) + 0.5 * sine(3100, 0.08)) * env(0.08, 0.012)
        thunk = sine(140, 0.08) * env(0.08, 0.015)
        return mix(0.12, (clack, 0, 1), (metal, 0, 0.3), (thunk, 0.004, 0.7))

    drawer = mix(1.4, (filt(noise(0.2), "low", 900) * env(0.2, 0.05), 0.0, 0.9), (sine(80, 0.25) * env(0.25, 0.06), 0, 1.0),
                 (bell(2350, 1.2, 0.45), 0.06, 0.8), (bell(2350 * 1.26, 1.1, 0.4), 0.09, 0.5))
    p.default(down=p.many("key", 4, key))
    p.group("enter", down=[p.wav("ka-ching.wav", drawer)])
    p.group("space", down=[p.wav("space.wav", mix(0.15, (key(0), 0, 1), (sine(100, 0.12) * env(0.12, 0.03), 0, 0.8)))])
    p.save()


def adding_machine():
    p = Pack("adding-machine", "Adding Machine", "Heavy 1950s mechanical calculator. Enter cranks the motor and prints the total.", (20, 1.0))

    def key(i):
        return mix(0.12, (filt(noise(0.05), "low", 2500) * env(0.05, 0.008), 0, 1), (sine(110, 0.1) * env(0.1, 0.02), 0, 1),
                   (sine(900 * rng.uniform(0.9, 1.1), 0.05) * env(0.05, 0.008), 0.003, 0.25))

    def ratchet(s, rate):
        out = np.zeros(n(s))
        for k in range(int(s * rate)):
            c = filt(noise(0.006), "band", [1500, 6000]) * env(0.006, 0.0012)
            i = n(k / rate)
            out[i:i + len(c)] += c[: max(0, len(out) - i)]
        return out

    x = t(0.9)
    motor = saw_f(55 + 8 * np.sin(2 * np.pi * 7 * x)) * np.minimum(1, x / 0.05) * np.minimum(1, (0.9 - x) / 0.1)
    total = mix(1.0, (filt(motor, "low", 600), 0, 0.6), (ratchet(0.85, 22), 0.03, 0.9), (key(0), 0.86, 0.9))
    p.default(down=p.many("key", 4, key))
    p.group("enter", down=[p.wav("total.wav", total)])
    p.group("space", down=[p.wav("space.wav", ratchet(0.12, 40))])
    p.save()


def telegraph():
    p = Pack("telegraph", "Telegraph", "Morse key: a dit for every letter, a dah for Space, and a sign-off on Enter.", (0, 0.5))

    def tone(s):
        return sine(720, s) * np.minimum(1, t(s) / 0.004) * np.minimum(1, (s - t(s)) / 0.006)

    def key_click(s):
        return filt(noise(0.006), "band", [800, 4000]) * env(0.006, 0.0012)

    dit = mix(0.1, (tone(0.07), 0, 1), (key_click(0), 0, 0.3))
    dah = mix(0.24, (tone(0.21), 0, 1), (key_click(0), 0, 0.3))
    sk = np.concatenate([tone(0.07), np.zeros(n(0.07)), tone(0.07), np.zeros(n(0.07)), tone(0.07), np.zeros(n(0.07)),
                         tone(0.21), np.zeros(n(0.07)), tone(0.07), np.zeros(n(0.07)), tone(0.21)])
    p.default(down=[p.wav("dit.wav", dit, 0.6)])
    p.group("space", down=[p.wav("dah.wav", dah, 0.6)])
    p.group("enter", down=[p.wav("sign-off.wav", sk, 0.6)])
    p.silent_modifiers()
    p.save()


def camera():
    p = Pack("camera-shutter", "Camera Shutter", "DSLR shutter on every key, a burst on Space, and the film winder on Enter.", (20, 1.0))

    def click(f_lo=1500, f_hi=8000, tau=0.002):
        return filt(noise(0.015), "band", [f_lo, f_hi], 3) * env(0.015, tau, 0.0001)

    def shutter(i):
        gap = rng.uniform(0.035, 0.055)
        mirror = filt(noise(0.04), "band", [300, 2500]) * env(0.04, 0.008)
        return mix(0.12, (click(), 0, 1), (mirror, 0, 0.5), (click(1200, 6000, 0.003), gap, 0.9), (sine(160, 0.05) * env(0.05, 0.01), gap, 0.4))

    burst = mix(0.62, *[(shutter(0), k * 0.11, 1.0) for k in range(5)])
    winder = np.zeros(n(0.5))
    for k in range(12):
        c = click(2000, 7000, 0.0015) * (0.6 + 0.4 * (k % 2))
        i = n(k * 0.035)
        winder[i:i + len(c)] += c[: max(0, len(winder) - i)]
    winder += filt(saw_f(np.full(n(0.5), 140)), "low", 900) * 0.08
    p.default(down=p.many("shutter", 4, shutter))
    p.group("space", down=[p.wav("burst.wav", burst)])
    p.group("enter", down=[p.wav("winder.wav", winder)])
    p.save()

# ----------------------------------------------------------------------------- sci-fi & games


def hum(s, f=92, wobble=0.0):
    x = t(s)
    fr = f * (1 + wobble * np.sin(2 * np.pi * 3 * x))
    h = saw_f(fr) * 0.6 + saw_f(fr * 2.003) * 0.3 + np.sin(2 * np.pi * np.cumsum(fr * 0.5) / SR) * 0.4
    return filt(h, "low", 1400)


def energy_blade():
    p = Pack("energy-blade", "Energy Blade", "Humming plasma sword: every key is a swing, Enter ignites it, Backspace powers it down.", (60, 1.5))

    def swing(i):
        s = rng.uniform(0.28, 0.4)
        x = t(s) / s
        bend = 1 + 0.35 * np.sin(np.pi * x) * rng.choice([-1, 1])
        fr = 92 * bend
        sig = saw_f(fr) * 0.6 + saw_f(fr * 2.003) * 0.3
        sig = filt(sig, "low", 1200 + 800 * rng.random())
        whoosh = filt(noise(s), "band", [300, 2500]) * 0.25
        return (sig + whoosh) * np.sin(np.pi * x) ** 1.5

    x = t(1.0)
    ign_f = 40 + 52 * np.minimum(1, x / 0.35)
    ign = filt(saw_f(ign_f) * 0.6 + saw_f(ign_f * 2.003) * 0.3, "low", 2500) * np.minimum(1, x / 0.02) * np.exp(-np.maximum(0, x - 0.4) / 0.35)
    ign += filt(noise(1.0), "band", [2000, 8000]) * np.exp(-x / 0.08) * 0.4
    off_f = 92 * np.exp(-x / 0.25) + 20
    off = filt(saw_f(off_f) * 0.6 + saw_f(off_f * 2.003) * 0.3, "low", 1500) * np.exp(-x / 0.25)
    p.default(down=p.many("swing", 5, swing))
    p.group("enter", down=[p.wav("ignite.wav", ign)])
    p.group("backspace", down=[p.wav("power-down.wav", off[: n(0.8)])])
    p.silent_modifiers()
    p.save()


def laser():
    p = Pack("laser-blaster", "Laser Blaster", "Pew pew. Space fires a charged shot, Enter a big beam.", (80, 1.0))

    def pew(i):
        f0 = rng.uniform(1800, 3200)
        s = rng.uniform(0.12, 0.18)
        core = chirp(f0, 220, s) * env(s, s / 2.5, 0.001)
        sq = np.sign(chirp(f0 * 0.5, 120, s)) * env(s, s / 3, 0.001) * 0.25
        return core + filt(sq, "low", 3000)

    x = t(0.7)
    charge_f = 200 + 1600 * (x / 0.45) ** 2
    charge = np.sin(2 * np.pi * np.cumsum(charge_f) / SR) * np.minimum(1, x / 0.45) * 0.4 * (x < 0.45)
    shot = mix(0.75, (charge, 0, 1), (pew(0) * 1.0, 0.45, 1.2), (filt(noise(0.2), "low", 1500) * env(0.2, 0.05), 0.45, 0.6))
    xb = t(0.9)
    beam = (saw_f(260 - 120 * xb) * 0.4 + np.sin(2 * np.pi * np.cumsum(1300 - 600 * xb) / SR) * 0.6) * np.minimum(1, xb / 0.02) * np.exp(-xb / 0.45)
    p.default(down=p.many("pew", 6, pew))
    p.group("space", down=[p.wav("charged-shot.wav", shot)])
    p.group("enter", down=[p.wav("beam.wav", filt(beam, "low", 5000))])
    p.silent_modifiers()
    p.save()


def arcade():
    p = Pack("arcade-coins", "Arcade Coins", "Classic platformer bleeps: coins for letters, a jump on Space, a power-up on Enter, damage on Backspace.", (0, 0.5))

    def coin(i):
        a, b = note(83 + (i % 2) * 2), note(88 + (i % 2) * 2)
        return np.concatenate([square(a, 0.05, 0.5) * 0.5, square(b, 0.22, 0.5) * 0.5 * np.exp(-t(0.22) / 0.09)])

    x = t(0.2)
    jump = square(220 * 2 ** (x / 0.2 * 1.6), 0.2, 0.25) * np.exp(-x / 0.12) * 0.5
    notes = [60, 64, 67, 72, 76, 79, 84]
    power = np.concatenate([square(note(m + 7), 0.05, 0.5) * 0.4 * np.exp(-t(0.05) / 0.05) for m in notes])
    xd = t(0.35)
    hurt = (square(400 * 2 ** (-xd / 0.35 * 2), 0.35, 0.5) * 0.4 + filt(noise(0.35), "low", 3000) * 0.3) * np.exp(-xd / 0.15)
    p.default(down=p.many("coin", 2, coin, 0.6))
    p.group("space", down=[p.wav("jump.wav", jump, 0.6)])
    p.group("enter", down=[p.wav("power-up.wav", power, 0.6)])
    p.group("backspace", down=[p.wav("damage.wav", hurt, 0.6)])
    p.silent_modifiers()
    p.save()


def spaceship():
    p = Pack("spaceship-console", "Spaceship Console", "Soft bridge-console chirps. Enter engages warp.", (40, 1.5))

    def chirp2(i):
        f = rng.choice([1175, 1319, 1568, 1760, 2093])
        a = sine(f, 0.06) * env(0.06, 0.03, 0.003)
        b = sine(f * rng.choice([1.25, 1.5, 0.75]), 0.07) * env(0.07, 0.035, 0.003)
        return mix(0.14, (a, 0, 0.7), (b, 0.055, 0.6))

    x = t(1.6)
    whoosh = filt(noise(1.6), "band", [200, 3000]) * np.minimum(1, x / 0.9) * np.exp(-np.maximum(0, x - 1.0) / 0.2)
    rise = np.sin(2 * np.pi * np.cumsum(60 + 500 * (x / 1.6) ** 2) / SR) * np.minimum(1, x / 0.5) * np.exp(-np.maximum(0, x - 1.1) / 0.15) * 0.6
    boom = mix(1.6, (filt(noise(0.5), "low", 400) * env(0.5, 0.15), 1.05, 1.0))
    p.default(down=p.many("chirp", 6, chirp2, 0.5))
    p.group("enter", down=[p.wav("warp.wav", whoosh * 0.6 + rise + boom)])
    p.group("space", down=[p.wav("space.wav", mix(0.2, (sine(523, 0.18) * env(0.18, 0.08, 0.005), 0, 1), (sine(784, 0.18) * env(0.18, 0.08, 0.005), 0, 0.6)), 0.5)])
    p.silent_modifiers()
    p.save()


def robot():
    p = Pack("robot-beeps", "Robot Beeps", "A chatty little astromech-style robot burbles on every key.", (100, 1.5))

    def burble(i):
        parts, at = [], 0.0
        for _ in range(rng.integers(2, 6)):
            s = rng.uniform(0.03, 0.08)
            f0, f1 = rng.uniform(900, 3200), rng.uniform(900, 3600)
            tone = chirp(f0, f1, s, "lin") * env(s, s, 0.003) * 0.7 + np.sign(chirp(f0 / 2, f1 / 2, s, "lin")) * env(s, s, 0.003) * 0.12
            parts.append((tone, at, 1.0))
            at += s + rng.uniform(0.0, 0.025)
        return mix(at + 0.02, *parts)

    excited = mix(0.6, *[(chirp(rng.uniform(1500, 2500), rng.uniform(2500, 4000), 0.05, "lin") * env(0.05, 0.05, 0.002), k * 0.06, 1) for k in range(9)])
    x = t(0.6)
    sad = chirp(2400, 500, 0.6, "lin") * np.minimum(1, x / 0.02) * np.exp(-x / 0.35)
    p.default(down=p.many("beep", 10, burble, 0.55))
    p.group("enter", down=[p.wav("excited.wav", excited, 0.55)])
    p.group("backspace", down=[p.wav("sad.wav", sad, 0.55)])
    p.silent_modifiers()
    p.save()

# ----------------------------------------------------------------------------- nature & cozy


def rain():
    p = Pack("rain", "Rain", "Every key is a raindrop on a window. Space rolls distant thunder.", (120, 2.0))

    def drop(i):
        s = 0.05
        f0 = rng.uniform(1200, 2600)
        plink = chirp(f0, f0 * rng.uniform(1.4, 2.2), s) * env(s, rng.uniform(0.006, 0.014), 0.0005)
        tap = filt(noise(0.004), "high", 2000) * env(0.004, 0.0008, 0.0001)
        return mix(0.06, (tap, 0, 0.5), (plink, 0.0005, 1.0))

    x = t(2.5)
    brown = np.cumsum(noise(2.5))
    brown -= np.convolve(brown, np.ones(4410) / 4410, mode="same")
    rumble = filt(brown, "low", 160, 3) * (np.minimum(1, x / 0.4) * np.exp(-np.maximum(0, x - 0.3) / 0.7))
    rumble *= 1 + 0.5 * np.sin(2 * np.pi * 1.7 * x) * np.exp(-x / 1.0)
    p.default(down=p.many("drop", 8, drop, 0.6))
    p.group("space", down=[p.wav("thunder.wav", rumble, 0.9)])
    p.group("enter", down=[p.wav("splash.wav", mix(0.15, *[(drop(0), k * 0.02, 0.7) for k in range(4)]), 0.6)])
    p.silent_modifiers()
    p.save()


def marimba_note(f):
    s = 0.6
    body = sine(f, s) * env(s, 0.28, 0.001) + 0.3 * sine(f * 4.0, s) * env(s, 0.05, 0.001) + 0.08 * sine(f * 9.9, s) * env(s, 0.015, 0.001)
    mallet = filt(noise(0.01), "low", 3000) * env(0.01, 0.002, 0.0002) * 0.3
    return mix(s, (body, 0, 1), (mallet, 0, 1))


def marimba():
    p = Pack("marimba", "Marimba", "Wooden marimba bars tuned to a pentatonic scale, so whatever you type sounds like a melody.", (0, 1.0))
    musical_keys(p, marimba_note, "bar")
    root = note(36)
    p.group("space", down=[p.wav("space.wav", marimba_note(root) + 0.7 * marimba_note(root * 1.5))])
    p.group("enter", down=[p.wav("enter.wav", mix(0.9, *[(marimba_note(note(m)), k * 0.07, 1) for k, m in enumerate([60, 64, 67, 72])]))])
    p.group("backspace", down=[p.wav("backspace.wav", marimba_note(note(43)))])
    p.default(down=[p.wav("other.wav", marimba_note(note(55)))])
    p.silent_modifiers()
    p.save()


def bubble_wrap():
    p = Pack("bubble-wrap", "Bubble Wrap", "Pop. Pop. Pop. Space pops a whole row.", (60, 2.0))

    def pop(i):
        snap = filt(noise(0.012), "band", [800, 9000], 2) * env(0.012, 0.0015, 0.00005)
        body = sine(rng.uniform(900, 1600), 0.03) * env(0.03, 0.004, 0.0002) * 0.6
        squeak = filt(noise(0.02), "band", [2000, 5000]) * env(0.02, 0.004) * 0.15
        return mix(0.05, (squeak, 0, 1), (snap, 0.004, 1.2), (body, 0.004, 1))

    p.default(down=p.many("pop", 6, pop))
    p.group("space", down=[p.wav("row.wav", mix(0.45, *[(pop(0), k * rng.uniform(0.03, 0.06) + 0.01 * k, rng.uniform(0.6, 1)) for k in range(7)]))])
    p.save()


def campfire():
    p = Pack("campfire", "Campfire", "Crackling fire: a crackle on every key, a big pop when you hit Space.", (80, 2.0))

    def crackle(i):
        out = np.zeros(n(0.15))
        for _ in range(rng.integers(2, 6)):
            c = filt(noise(0.004), "band", [1500, 7000], 2) * env(0.004, 0.0007, 0.00005) * rng.uniform(0.3, 1)
            j = n(rng.uniform(0, 0.11))
            out[j:j + len(c)] += c[: max(0, len(out) - j)]
        hiss = filt(noise(0.15), "band", [500, 3000]) * env(0.15, 0.05) * 0.05
        return out + hiss

    big = mix(0.4, (filt(noise(0.02), "low", 3000) * env(0.02, 0.004, 0.0001), 0, 1.4), (sine(140, 0.15) * env(0.15, 0.03), 0, 0.6), (crackle(0), 0.03, 0.8))
    p.default(down=p.many("crackle", 8, crackle, 0.6))
    p.group("space", down=[p.wav("pop.wav", big)])
    p.silent_modifiers()
    p.save()


def pen_paper():
    p = Pack("pen-and-paper", "Pen & Paper", "Scratchy pen strokes. Enter turns the page.", (40, 2.0))

    def stroke(i):
        s = rng.uniform(0.07, 0.14)
        x = t(s)
        am = 0.6 + 0.4 * np.sin(2 * np.pi * rng.uniform(25, 45) * x)
        scratch = filt(noise(s), "band", [rng.uniform(1800, 2800), rng.uniform(5000, 8000)], 2) * am
        shape = np.sin(np.pi * x / s) ** 0.6
        return scratch * shape

    x = t(0.45)
    flip = filt(noise(0.45), "band", [400, 6000]) * (np.sin(np.pi * np.minimum(1, x / 0.35)) ** 2)
    flip = flip * (1 + 0.6 * np.sin(2 * np.pi * 18 * x))
    flip += mix(0.45, (filt(noise(0.05), "low", 1500) * env(0.05, 0.01), 0.36, 1.2))
    dot = filt(noise(0.02), "band", [1000, 5000]) * env(0.02, 0.003)
    p.default(down=p.many("stroke", 6, stroke, 0.5))
    p.group("enter", down=[p.wav("page-flip.wav", flip, 0.6)])
    p.group("space", down=[p.wav("lift.wav", dot, 0.25)])
    p.group("punctuation", down=[p.wav("dot.wav", dot, 0.4)])
    p.silent_modifiers()
    p.save()

# ----------------------------------------------------------------------------- funny


def formant(sig, centers, q=6.0):
    out = np.zeros_like(sig)
    for fc, g in centers:
        bw = fc / q
        out += filt(sig, "band", [max(50, fc - bw), min(SR / 2 - 100, fc + bw)], 2) * g
    return out


def rubber_duck():
    p = Pack("rubber-duck", "Rubber Duck", "Squeaky bath-toy duck on every key. Enter gets a proper quack.", (120, 1.5))

    def squeak(i):
        s = rng.uniform(0.12, 0.2)
        x = t(s) / s
        f = rng.uniform(1300, 1800) * (1 + 0.35 * np.sin(np.pi * x)) * (1 + 0.02 * np.sin(2 * np.pi * 30 * t(s)))
        ph = np.cumsum(f) / SR
        tone = np.sin(2 * np.pi * ph) + 0.4 * np.sin(4 * np.pi * ph) + 0.2 * np.sin(6 * np.pi * ph)
        air = filt(noise(s), "band", [2000, 6000]) * 0.15
        return (tone + air) * np.sin(np.pi * x) ** 0.7

    def quack():
        s = 0.22
        x = t(s) / s
        f = 340 * (1 - 0.25 * x)
        src = saw_f(f) + 0.3 * filt(noise(s), "high", 1000)
        voiced = formant(src, [(1000, 1.0), (1800, 0.7), (2900, 0.3)], 4)
        return voiced * np.sin(np.pi * np.minimum(1, x * 1.2)) ** 0.5 * np.exp(-x * 1.5)

    p.default(down=p.many("squeak", 5, squeak, 0.6))
    p.group("enter", down=[p.wav("quack.wav", quack(), 0.7)])
    p.group("space", down=[p.wav("double-quack.wav", mix(0.5, (quack(), 0, 1), (quack(), 0.24, 0.8)), 0.7)])
    p.silent_modifiers()
    p.save()


def cat():
    p = Pack("cat", "Cat", "A cartoon cat: little chirps and mrrps for keys, a full meow on Enter, a hiss on Backspace.", (90, 1.5))

    def voice(f_curve, formant_curve, s):
        ph = np.cumsum(f_curve) / SR
        harm = sum(np.sin(2 * np.pi * h * ph) / h ** 1.6 for h in range(1, 10))
        # time-varying formant: crossfade between two filtered versions
        a = formant(harm, [(formant_curve[0], 1.0), (formant_curve[0] * 2.2, 0.4)], 3)
        b = formant(harm, [(formant_curve[1], 1.0), (formant_curve[1] * 2.2, 0.4)], 3)
        w = np.linspace(0, 1, len(harm))
        return filt(a * (1 - w) + b * w, "low", 3500)

    def mrrp(i):
        s = rng.uniform(0.16, 0.24)
        x = t(s) / s
        f = rng.uniform(520, 700) * (1 + 0.2 * np.sin(np.pi * x))
        trill = 1 + 0.6 * np.sin(2 * np.pi * 28 * t(s))
        return voice(f, (1400, 900), s) * trill * np.sin(np.pi * x) ** 0.8

    s = 0.75
    x = t(s) / s
    f = 480 + 380 * np.sin(np.pi * np.minimum(1, x * 1.3)) - 120 * x
    meow = voice(f, (2600, 800), s) * np.sin(np.pi * x) ** 0.6
    xh = t(0.6)
    hiss = filt(noise(0.6), "band", [3000, 9000]) * np.minimum(1, xh / 0.05) * np.exp(-np.maximum(0, xh - 0.35) / 0.08)
    p.default(down=p.many("mrrp", 5, mrrp, 0.55))
    p.group("enter", down=[p.wav("meow.wav", meow, 0.65)])
    p.group("backspace", down=[p.wav("hiss.wav", hiss, 0.5)])
    p.silent_modifiers()
    p.save()


def drums():
    p = Pack("drum-kit", "Drum Kit", "Hi-hat on letters, toms on numbers, kick on Space, snare on Enter, crash on Backspace. Type a beat.", (15, 2.0))

    def hat(i, open_=False):
        s = 0.35 if open_ else 0.06
        metal = sum(np.sign(sine(f, s)) for f in [205.3, 304.4, 369.6, 522.7, 540, 800]) / 6
        return filt(metal + 0.5 * noise(s), "high", 7000, 3) * env(s, 0.12 if open_ else 0.012, 0.0002)

    def kick(i=0):
        s = 0.45
        x = t(s)
        f = 48 + 110 * np.exp(-x / 0.035)
        return mix(s, (np.sin(2 * np.pi * np.cumsum(f) / SR) * np.exp(-x / 0.18), 0, 1), (filt(noise(0.005), "high", 2000) * env(0.005, 0.001), 0, 0.3))

    def snare(i=0):
        s = 0.3
        tone = (sine(185, s) + 0.5 * sine(330, s)) * env(s, 0.05, 0.0005)
        wires = filt(noise(s), "band", [1500, 9000]) * env(s, 0.07, 0.0005)
        return tone * 0.7 + wires

    def tom(f):
        s = 0.5
        x = t(s)
        fr = f * (1 + 0.25 * np.exp(-x / 0.05))
        return mix(s, (np.sin(2 * np.pi * np.cumsum(fr) / SR) * np.exp(-x / 0.2), 0, 1), (filt(noise(0.01), "band", [500, 4000]) * env(0.01, 0.003), 0, 0.3))

    def crash():
        s = 1.6
        metal = sum(np.sign(sine(f * rng.uniform(0.98, 1.02), s)) for f in [311, 427, 593, 701, 877, 1137]) / 6
        return filt(metal + noise(s) * 0.8, "high", 4000, 2) * env(s, 0.55, 0.001)

    p.default(down=p.many("hat", 4, hat, 0.5))
    p.group("letters", down=p.many("hat", 4, hat, 0.5))
    toms = [p.wav(f"tom{k}.wav", tom(f), 0.8) for k, f in enumerate([200, 165, 135, 110], 1)]
    for k, code in enumerate(ROWS["digits"]):
        p.key(code, down=[toms[min(3, k * 4 // len(ROWS["digits"]))]])
    p.group("space", down=[p.wav("kick.wav", kick(), 0.95)])
    p.group("enter", down=[p.wav("snare.wav", snare(), 0.85)])
    p.group("backspace", down=[p.wav("crash.wav", crash(), 0.7)])
    p.group("tab", down=[p.wav("open-hat.wav", hat(0, True), 0.5)])
    p.group("modifiers", down=[p.wav("rim.wav", filt(noise(0.03), "band", [1500, 5000]) * env(0.03, 0.004) + sine(900, 0.03) * env(0.03, 0.006), 0.5)], up=[])
    p.save()


def piano_note(f):
    s = 1.1
    B = 0.0004
    tone = np.zeros(n(s))
    for k in range(1, 9):
        fk = f * k * np.sqrt(1 + B * k * k)
        if fk > 16000:
            break
        tone += sine(fk, s, rng.uniform(0, 6.28)) / k ** 1.1 * env(s, 0.9 / (1 + 0.35 * k), 0.002)
    hammer = filt(noise(0.01), "band", [400, 4000]) * env(0.01, 0.002, 0.0002) * 0.2
    return mix(s, (tone, 0, 1), (hammer, 0, 1))


def piano():
    p = Pack("piano", "Piano", "Every key is a piano note on a pentatonic scale (low on the left, high on the right), so typing sounds musical.", (0, 1.0))
    musical_keys(p, piano_note, "note")
    p.group("space", down=[p.wav("chord.wav", sum(piano_note(note(m)) for m in [36, 43, 48]))])
    p.group("enter", down=[p.wav("arpeggio.wav", mix(1.5, *[(piano_note(note(m)), k * 0.08, 1) for k, m in enumerate([60, 64, 67, 72, 76])]))])
    p.group("backspace", down=[p.wav("low.wav", piano_note(note(31)))])
    p.default(down=[p.wav("other.wav", piano_note(note(55)))])
    p.silent_modifiers()
    p.save()


def game_show():
    p = Pack("game-show", "Game Show", "Correct-answer dings for every key, a buzzer on Backspace, and applause when you hit Enter.", (30, 1.0))

    def ding(i):
        f = rng.choice([1568, 1760, 2093])
        return bell(f, 0.6, 0.25)

    x = t(0.7)
    buzzer = (np.sign(np.sin(2 * np.pi * 110 * x)) + np.sign(np.sin(2 * np.pi * 116 * x))) * 0.4
    buzzer = filt(buzzer, "low", 2500) * np.minimum(1, x / 0.01) * np.minimum(1, (0.7 - x) / 0.02)
    s = 2.2
    applause = np.zeros(n(s))
    for _ in range(420):
        at = rng.uniform(0, 1.0) ** 1.4 * 2.1
        c = filt(noise(0.012), "band", [rng.uniform(600, 1200), rng.uniform(2500, 6000)]) * env(0.012, 0.002, 0.0002) * rng.uniform(0.3, 1)
        j = n(at)
        applause[j:j + len(c)] += c[: max(0, len(applause) - j)]
    xa = t(s)
    applause *= np.minimum(1, xa / 0.15) * np.exp(-np.maximum(0, xa - 1.0) / 0.4)
    chime = mix(0.9, (bell(1319, 0.8, 0.35), 0, 1), (bell(1047, 0.8, 0.35), 0.18, 1))
    p.default(down=p.many("ding", 3, ding, 0.5))
    p.group("backspace", down=[p.wav("buzzer.wav", buzzer, 0.6)])
    p.group("enter", down=[p.wav("applause.wav", applause, 0.8)])
    p.group("space", down=[p.wav("chime.wav", chime, 0.5)])
    p.silent_modifiers()
    p.save()


ALL = {
    "buckling-spring": buckling_spring, "cash-register": cash_register, "adding-machine": adding_machine,
    "telegraph": telegraph, "camera-shutter": camera, "energy-blade": energy_blade, "laser-blaster": laser,
    "arcade-coins": arcade, "spaceship-console": spaceship, "robot-beeps": robot, "rain": rain,
    "marimba": marimba, "bubble-wrap": bubble_wrap, "campfire": campfire, "pen-and-paper": pen_paper,
    "rubber-duck": rubber_duck, "cat": cat, "drum-kit": drums, "piano": piano, "game-show": game_show,
}

if __name__ == "__main__":
    wanted = sys.argv[1:] or list(ALL)
    print(f"writing to {ROOT}")
    for k in wanted:
        ALL[k]()
