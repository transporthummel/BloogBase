#!/usr/bin/env python3
"""Shows how SDL2 numbers a controller's inputs, for BloogBase's DOSBox
mapper file (src-tauri/src/keymap.rs), which binds raw joystick numbers.

DOSBox Staging 0.83 has SDL2 2.32.10 built in (statically on Linux), so
take the same version:

    python3 -m venv venv && venv/bin/pip install pysdl2-dll==2.32.10
    venv/bin/python scripts/padprobe.py

Plain Python otherwise (ctypes, no other packages). Options:

    --sdl PATH      an SDL2 library to load instead (else pysdl2-dll, else
                    the system's libSDL2)
    --dosbox PATH   also start that DOSBox briefly and print what its
                    mapper logs about the controller, to compare
    --live          then print each input as it's used, until Ctrl+C

Connect the controllers before starting it. Paste the whole output into the
plan: it has what keymap.rs needs.
"""

import argparse
import ctypes
import ctypes.util
import os
import platform
import subprocess
import sys
import time

SDL_INIT_JOYSTICK = 0x200
SDL_INIT_GAMECONTROLLER = 0x2000

# The inputs keymap.rs binds, named as in SDL's gamepad mappings, with
# BloogBase's name (after the place on an Xbox controller).
ROLES = [
    ("a", "A / Cross"),
    ("b", "B / Circle"),
    ("x", "X / Square"),
    ("y", "Y / Triangle"),
    ("leftshoulder", "LB / L1"),
    ("rightshoulder", "RB / R1"),
    ("back", "View / Share"),
    ("start", "Menu / Options"),
    ("leftstick", "left stick click"),
    ("rightstick", "right stick click"),
    ("dpup", "D-pad up"),
    ("dpright", "D-pad right"),
    ("dpdown", "D-pad down"),
    ("dpleft", "D-pad left"),
    ("leftx", "left stick x"),
    ("lefty", "left stick y"),
    ("rightx", "right stick x"),
    ("righty", "right stick y"),
    ("lefttrigger", "LT / L2"),
    ("righttrigger", "RT / R2"),
]

HAT_BITS = {1: "up", 2: "right", 4: "down", 8: "left"}


class GUID(ctypes.Structure):
    _fields_ = [("data", ctypes.c_uint8 * 16)]


class Version(ctypes.Structure):
    _fields_ = [("major", ctypes.c_uint8), ("minor", ctypes.c_uint8), ("patch", ctypes.c_uint8)]


def find_sdl(path):
    """The SDL2 library: the given one, pysdl2-dll's, or the system's."""
    if path:
        return path, "--sdl"
    try:
        import sdl2dll  # noqa: F401  (pysdl2-dll)

        folder = sdl2dll.get_dllpath()
        for name in sorted(os.listdir(folder)):
            if name.startswith(("libSDL2-2.0.so", "libSDL2.so", "SDL2.dll", "libSDL2.dylib", "libSDL2-2.0.dylib")):
                return os.path.join(folder, name), "pysdl2-dll"
    except ImportError:
        pass
    found = ctypes.util.find_library("SDL2") or ctypes.util.find_library("SDL2-2.0")
    if found:
        return found, "system"
    sys.exit("No SDL2 found: install pysdl2-dll==2.32.10 or pass --sdl PATH.")


def load(path):
    sdl = ctypes.CDLL(path)
    sig = [
        ("SDL_GetVersion", None, [ctypes.POINTER(Version)]),
        ("SDL_Init", ctypes.c_int, [ctypes.c_uint32]),
        ("SDL_Quit", None, []),
        ("SDL_GetError", ctypes.c_char_p, []),
        ("SDL_PumpEvents", None, []),
        ("SDL_NumJoysticks", ctypes.c_int, []),
        ("SDL_JoystickNameForIndex", ctypes.c_char_p, [ctypes.c_int]),
        ("SDL_JoystickOpen", ctypes.c_void_p, [ctypes.c_int]),
        ("SDL_JoystickGetVendor", ctypes.c_uint16, [ctypes.c_void_p]),
        ("SDL_JoystickGetProduct", ctypes.c_uint16, [ctypes.c_void_p]),
        ("SDL_JoystickNumButtons", ctypes.c_int, [ctypes.c_void_p]),
        ("SDL_JoystickNumAxes", ctypes.c_int, [ctypes.c_void_p]),
        ("SDL_JoystickNumHats", ctypes.c_int, [ctypes.c_void_p]),
        ("SDL_JoystickGetButton", ctypes.c_uint8, [ctypes.c_void_p, ctypes.c_int]),
        ("SDL_JoystickGetAxis", ctypes.c_int16, [ctypes.c_void_p, ctypes.c_int]),
        ("SDL_JoystickGetHat", ctypes.c_uint8, [ctypes.c_void_p, ctypes.c_int]),
        ("SDL_JoystickUpdate", None, []),
        ("SDL_JoystickGetDeviceGUID", GUID, [ctypes.c_int]),
        ("SDL_JoystickGetGUIDString", None, [GUID, ctypes.c_char_p, ctypes.c_int]),
        ("SDL_GameControllerMappingForGUID", ctypes.c_char_p, [GUID]),
    ]
    for name, res, args in sig:
        f = getattr(sdl, name)
        f.restype = res
        f.argtypes = args
    return sdl


def dosbox_bind(spec, role):
    """A mapping entry (b9, h0.4, a2, +a2, -a2, a2~) as keymap.rs binds it."""
    s = spec.lstrip("+-").rstrip("~")
    if s.startswith("b"):
        return f"stick_0 button {s[1:]}"
    if s.startswith("h"):
        hat, bit = s[1:].split(".")
        return f"stick_0 hat {hat} {bit}"
    if s.startswith("a"):
        n = s[1:]
        if role in ("lefttrigger", "righttrigger"):
            return f"stick_0 axis {n} 1 (pulled)"
        return f"stick_0 axis {n} 0 / 1 (negative / positive)"
    return spec


def describe(sdl, index):
    js = sdl.SDL_JoystickOpen(index)
    if not js:
        print(f"  could not open: {sdl.SDL_GetError().decode()}")
        return None
    name = (sdl.SDL_JoystickNameForIndex(index) or b"?").decode(errors="replace")
    guid = sdl.SDL_JoystickGetDeviceGUID(index)
    buf = ctypes.create_string_buffer(33)
    sdl.SDL_JoystickGetGUIDString(guid, buf, 33)
    print(f"stick_{index}: {name}")
    print(f"  vendor {sdl.SDL_JoystickGetVendor(js):04x}, product {sdl.SDL_JoystickGetProduct(js):04x}, guid {buf.value.decode()}")
    print(
        f"  {sdl.SDL_JoystickNumButtons(js)} buttons, {sdl.SDL_JoystickNumAxes(js)} axes, "
        f"{sdl.SDL_JoystickNumHats(js)} hat(s)"
    )
    mapping = sdl.SDL_GameControllerMappingForGUID(guid)
    if not mapping:
        print("  no gamepad mapping: use --live to find the numbers")
        return js
    mapping = mapping.decode()
    print(f"  mapping: {mapping}")
    fields = dict(f.split(":", 1) for f in mapping.split(",")[2:] if ":" in f)
    print("  for keymap.rs:")
    for role, label in ROLES:
        spec = fields.get(role)
        print(f"    {label:<18} {role:<14} {spec or '-':<6} {dosbox_bind(spec, role) if spec else 'missing'}")
    return js


def from_dosbox(path):
    """What DOSBox's mapper logs about the controllers at its start."""
    print(f"\nDOSBox ({path}):")
    try:
        out = subprocess.run(
            [path, "-c", "exit"], capture_output=True, text=True, errors="replace", timeout=60
        )
    except (OSError, subprocess.TimeoutExpired) as e:
        print(f"  could not run: {e}")
        return
    lines = [l for l in (out.stdout + out.stderr).splitlines() if "MAPPER:" in l or "SDL: Version" in l]
    for line in lines or ["  (no MAPPER lines in its log)"]:
        print(f"  {line.strip()}")


def live(sdl, sticks):
    """Prints each button, hat and axis (past half way) as it changes."""
    print("\nPress buttons and move sticks and triggers; Ctrl+C ends.")
    # The first states can still be zero before the pad has reported.
    for _ in range(10):
        sdl.SDL_JoystickUpdate()
        time.sleep(0.02)
    before = {}
    try:
        while True:
            sdl.SDL_JoystickUpdate()
            for i, js in sticks:
                for b in range(sdl.SDL_JoystickNumButtons(js)):
                    v = sdl.SDL_JoystickGetButton(js, b)
                    if before.get((i, "b", b), 0) != v:
                        before[(i, "b", b)] = v
                        print(f"stick_{i} button {b} {'down' if v else 'up'}")
                for h in range(sdl.SDL_JoystickNumHats(js)):
                    v = sdl.SDL_JoystickGetHat(js, h)
                    if before.get((i, "h", h), 0) != v:
                        before[(i, "h", h)] = v
                        dirs = "+".join(n for bit, n in HAT_BITS.items() if v & bit) or "centre"
                        print(f"stick_{i} hat {h} {v} ({dirs})")
                for a in range(sdl.SDL_JoystickNumAxes(js)):
                    raw = sdl.SDL_JoystickGetAxis(js, a)
                    zone = 1 if raw > 16384 else -1 if raw < -16384 else 0
                    key = (i, "a", a)
                    if key not in before:
                        before[key] = zone
                        print(f"stick_{i} axis {a} rests at {raw}")
                    elif before[key] != zone:
                        before[key] = zone
                        side = {1: "positive (1)", -1: "negative (0)", 0: "back"}[zone]
                        print(f"stick_{i} axis {a} {side} ({raw})")
            time.sleep(0.02)
    except KeyboardInterrupt:
        print()


def main():
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument("--sdl")
    p.add_argument("--dosbox")
    p.add_argument("--live", action="store_true")
    args = p.parse_args()

    path, source = find_sdl(args.sdl)
    sdl = load(path)
    v = Version()
    sdl.SDL_GetVersion(ctypes.byref(v))
    print(f"{platform.platform()}, Python {platform.python_version()}")
    print(f"SDL {v.major}.{v.minor}.{v.patch} ({source}: {path})")
    if (v.major, v.minor, v.patch) != (2, 32, 10):
        print("  note: DOSBox Staging 0.83 has SDL 2.32.10; numbers may differ in others")
    hints = {k: os.environ[k] for k in os.environ if k.startswith("SDL_")}
    if hints:
        print(f"  SDL environment: {hints}")

    if sdl.SDL_Init(SDL_INIT_JOYSTICK | SDL_INIT_GAMECONTROLLER) != 0:
        sys.exit(f"SDL_Init failed: {sdl.SDL_GetError().decode()}")
    # Some backends list devices only after a moment.
    for _ in range(25):
        sdl.SDL_PumpEvents()
        time.sleep(0.02)
    n = sdl.SDL_NumJoysticks()
    print(f"\n{n} controller(s)")
    sticks = []
    for i in range(n):
        js = describe(sdl, i)
        if js:
            sticks.append((i, js))
    if args.dosbox:
        from_dosbox(args.dosbox)
    if args.live and sticks:
        live(sdl, sticks)
    sdl.SDL_Quit()


if __name__ == "__main__":
    main()
