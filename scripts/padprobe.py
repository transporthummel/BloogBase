#!/usr/bin/env python3
"""Shows how SDL numbers a controller's inputs, for BloogBase's DOSBox
mapper file (src-tauri/src/keymap.rs), which binds raw joystick numbers,
and checks them against keymap.rs.

It tests exactly one SDL library, which must be given: the one DOSBox
uses. DOSBox Staging 0.83 has SDL2 2.32.10, a DLL next to dosbox.exe on
Windows; on Linux it's built in, so take pysdl2-dll of the same version:

    python scripts/padprobe.py --sdl <dosbox dir>/SDL2.dll
    python3 -m venv venv && venv/bin/pip install pysdl2-dll==2.32.10
    venv/bin/python scripts/padprobe.py --sdl "$(venv/bin/python -c \\
        'import sdl2dll, glob; print(glob.glob(sdl2dll.get_dllpath() + "/libSDL2*")[0])')"

Plain Python otherwise (ctypes, no other packages). SDL2 and SDL3 both
work. Options:

    --sdl PATH      the SDL2 or SDL3 library to test (required)
    --dosbox PATH   also start that DOSBox briefly and print what its
                    mapper logs about the controller, to compare
    --check         ask for each input in turn and check that it arrives as
                    keymap.rs binds it, for the first controller (the one
                    DOSBox plays with); exit code 1 on a difference
    --layout NAME   xbox or playstation for --check (else from the
                    controller's IDs, as controller.rs decides)
    --live          print each input as it's used, until Ctrl+C

After a DOSBox update: connect each controller alone and run
`--sdl <new DOSBox's SDL> --check`. Connect the controllers before
starting it. Paste the whole output into the plan: it has what keymap.rs
needs.
"""

import argparse
import ctypes
import os
import platform
import re
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
KEYMAP = os.path.join(ROOT, "src-tauri", "src", "keymap.rs")
CONTROLLER = os.path.join(ROOT, "src-tauri", "src", "controller.rs")

INIT_JOYSTICK = 0x200
INIT_GAMEPAD = 0x2000

# DOSBox's mapper presses a key bound to an axis past this value.
AXIS_PRESS = 25000

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

# keymap.rs's `Input`s in the order --check asks for them, with what to do
# on an Xbox and on a PlayStation controller.
PROMPTS = [
    ("A", "Press A", "Press Cross"),
    ("B", "Press B", "Press Circle"),
    ("X", "Press X", "Press Square"),
    ("Y", "Press Y", "Press Triangle"),
    ("Lb", "Press LB", "Press L1"),
    ("Rb", "Press RB", "Press R1"),
    ("View", "Press View", "Press Share"),
    ("Menu", "Press Menu", "Press Options"),
    ("LsClick", "Click the left stick", "Click the left stick (L3)"),
    ("RsClick", "Click the right stick", "Click the right stick (R3)"),
    ("Up", "Press D-pad up", "Press D-pad up"),
    ("Right", "Press D-pad right", "Press D-pad right"),
    ("Down", "Press D-pad down", "Press D-pad down"),
    ("Left", "Press D-pad left", "Press D-pad left"),
    ("LsUp", "Push the left stick up", "Push the left stick up"),
    ("LsRight", "Push the left stick right", "Push the left stick right"),
    ("LsDown", "Push the left stick down", "Push the left stick down"),
    ("LsLeft", "Push the left stick left", "Push the left stick left"),
    ("RsUp", "Push the right stick up", "Push the right stick up"),
    ("RsRight", "Push the right stick right", "Push the right stick right"),
    ("RsDown", "Push the right stick down", "Push the right stick down"),
    ("RsLeft", "Push the right stick left", "Push the right stick left"),
    ("Lt", "Pull LT all the way", "Pull L2 all the way"),
    ("Rt", "Pull RT all the way", "Pull R2 all the way"),
]

HAT_BITS = {1: "up", 2: "right", 4: "down", 8: "left"}


class GUID(ctypes.Structure):
    _fields_ = [("data", ctypes.c_uint8 * 16)]


class Version2(ctypes.Structure):
    _fields_ = [("major", ctypes.c_uint8), ("minor", ctypes.c_uint8), ("patch", ctypes.c_uint8)]


def bind(lib, sig):
    for name, res, args in sig:
        f = getattr(lib, name)
        f.restype = res
        f.argtypes = args


P = ctypes.c_void_p
COMMON = [
    ("SDL_GetError", ctypes.c_char_p, []),
    ("SDL_PumpEvents", None, []),
    ("SDL_Quit", None, []),
]


class Sdl2:
    """SDL2's joystick functions. Joysticks go by device index."""

    major = 2

    def __init__(self, lib):
        self.lib = lib
        bind(lib, COMMON + [
            ("SDL_GetVersion", None, [ctypes.POINTER(Version2)]),
            ("SDL_Init", ctypes.c_int, [ctypes.c_uint32]),
            ("SDL_NumJoysticks", ctypes.c_int, []),
            ("SDL_JoystickNameForIndex", ctypes.c_char_p, [ctypes.c_int]),
            ("SDL_JoystickOpen", P, [ctypes.c_int]),
            ("SDL_JoystickGetVendor", ctypes.c_uint16, [P]),
            ("SDL_JoystickGetProduct", ctypes.c_uint16, [P]),
            ("SDL_JoystickNumButtons", ctypes.c_int, [P]),
            ("SDL_JoystickNumAxes", ctypes.c_int, [P]),
            ("SDL_JoystickNumHats", ctypes.c_int, [P]),
            ("SDL_JoystickGetButton", ctypes.c_uint8, [P, ctypes.c_int]),
            ("SDL_JoystickGetAxis", ctypes.c_int16, [P, ctypes.c_int]),
            ("SDL_JoystickGetHat", ctypes.c_uint8, [P, ctypes.c_int]),
            ("SDL_JoystickUpdate", None, []),
            ("SDL_JoystickGetDeviceGUID", GUID, [ctypes.c_int]),
            ("SDL_JoystickGetGUIDString", None, [GUID, ctypes.c_char_p, ctypes.c_int]),
            ("SDL_GameControllerMappingForGUID", ctypes.c_char_p, [GUID]),
        ])

    def version(self):
        v = Version2()
        self.lib.SDL_GetVersion(ctypes.byref(v))
        return (v.major, v.minor, v.patch)

    def init(self):
        return self.lib.SDL_Init(INIT_JOYSTICK | INIT_GAMEPAD) == 0

    def ids(self):
        return list(range(self.lib.SDL_NumJoysticks()))

    def name(self, i):
        return self.lib.SDL_JoystickNameForIndex(i)

    def open(self, i):
        return self.lib.SDL_JoystickOpen(i)

    def guid(self, i):
        buf = ctypes.create_string_buffer(33)
        self.lib.SDL_JoystickGetGUIDString(self.lib.SDL_JoystickGetDeviceGUID(i), buf, 33)
        return buf.value.decode()

    def mapping(self, i):
        m = self.lib.SDL_GameControllerMappingForGUID(self.lib.SDL_JoystickGetDeviceGUID(i))
        return m.decode() if m else None

    def vendor(self, js):
        return self.lib.SDL_JoystickGetVendor(js)

    def product(self, js):
        return self.lib.SDL_JoystickGetProduct(js)

    def counts(self, js):
        return (self.lib.SDL_JoystickNumButtons(js), self.lib.SDL_JoystickNumAxes(js), self.lib.SDL_JoystickNumHats(js))

    def update(self):
        self.lib.SDL_JoystickUpdate()

    def button(self, js, n):
        return self.lib.SDL_JoystickGetButton(js, n)

    def axis(self, js, n):
        return self.lib.SDL_JoystickGetAxis(js, n)

    def hat(self, js, n):
        return self.lib.SDL_JoystickGetHat(js, n)


class Sdl3(Sdl2):
    """SDL3's joystick functions. Joysticks go by instance ID."""

    major = 3

    def __init__(self, lib):
        self.lib = lib
        u32 = ctypes.c_uint32
        bind(lib, COMMON + [
            ("SDL_GetVersion", ctypes.c_int, []),
            ("SDL_Init", ctypes.c_bool, [u32]),
            ("SDL_free", None, [P]),
            ("SDL_GetJoysticks", P, [ctypes.POINTER(ctypes.c_int)]),
            ("SDL_GetJoystickNameForID", ctypes.c_char_p, [u32]),
            ("SDL_OpenJoystick", P, [u32]),
            ("SDL_GetJoystickVendor", ctypes.c_uint16, [P]),
            ("SDL_GetJoystickProduct", ctypes.c_uint16, [P]),
            ("SDL_GetNumJoystickButtons", ctypes.c_int, [P]),
            ("SDL_GetNumJoystickAxes", ctypes.c_int, [P]),
            ("SDL_GetNumJoystickHats", ctypes.c_int, [P]),
            ("SDL_GetJoystickButton", ctypes.c_bool, [P, ctypes.c_int]),
            ("SDL_GetJoystickAxis", ctypes.c_int16, [P, ctypes.c_int]),
            ("SDL_GetJoystickHat", ctypes.c_uint8, [P, ctypes.c_int]),
            ("SDL_UpdateJoysticks", None, []),
            ("SDL_GetJoystickGUIDForID", GUID, [u32]),
            ("SDL_GUIDToString", None, [GUID, ctypes.c_char_p, ctypes.c_int]),
            ("SDL_GetGamepadMappingForID", P, [u32]),
        ])

    def version(self):
        v = self.lib.SDL_GetVersion()
        return (v // 1000000, v // 1000 % 1000, v % 1000)

    def init(self):
        return bool(self.lib.SDL_Init(INIT_JOYSTICK | INIT_GAMEPAD))

    def ids(self):
        n = ctypes.c_int(0)
        arr = self.lib.SDL_GetJoysticks(ctypes.byref(n))
        if not arr:
            return []
        out = list(ctypes.cast(arr, ctypes.POINTER(ctypes.c_uint32))[: n.value])
        self.lib.SDL_free(arr)
        return out

    def name(self, i):
        return self.lib.SDL_GetJoystickNameForID(i)

    def open(self, i):
        return self.lib.SDL_OpenJoystick(i)

    def guid(self, i):
        buf = ctypes.create_string_buffer(33)
        self.lib.SDL_GUIDToString(self.lib.SDL_GetJoystickGUIDForID(i), buf, 33)
        return buf.value.decode()

    def mapping(self, i):
        m = self.lib.SDL_GetGamepadMappingForID(i)
        if not m:
            return None
        s = ctypes.cast(m, ctypes.c_char_p).value.decode()
        self.lib.SDL_free(m)
        return s

    def vendor(self, js):
        return self.lib.SDL_GetJoystickVendor(js)

    def product(self, js):
        return self.lib.SDL_GetJoystickProduct(js)

    def counts(self, js):
        return (
            self.lib.SDL_GetNumJoystickButtons(js),
            self.lib.SDL_GetNumJoystickAxes(js),
            self.lib.SDL_GetNumJoystickHats(js),
        )

    def update(self):
        self.lib.SDL_UpdateJoysticks()

    def button(self, js, n):
        return int(self.lib.SDL_GetJoystickButton(js, n))

    def axis(self, js, n):
        return self.lib.SDL_GetJoystickAxis(js, n)

    def hat(self, js, n):
        return self.lib.SDL_GetJoystickHat(js, n)


def load(path):
    """The given SDL library and nothing else: a file, not a name the
    system would look up."""
    if not os.path.isfile(path):
        sys.exit(f"--sdl: no file {path}")
    path = os.path.abspath(path)
    # Windows looks for an SDL DLL's own dependencies next to it.
    if hasattr(os, "add_dll_directory"):
        os.add_dll_directory(os.path.dirname(path))
    lib = ctypes.CDLL(path)
    return Sdl3(lib) if hasattr(lib, "SDL_GetJoysticks") else Sdl2(lib)


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


def describe(sdl, index, jid):
    js = sdl.open(jid)
    if not js:
        print(f"  could not open: {sdl.lib.SDL_GetError().decode()}")
        return None
    name = (sdl.name(jid) or b"?").decode(errors="replace")
    print(f"stick_{index}: {name}")
    print(f"  vendor {sdl.vendor(js):04x}, product {sdl.product(js):04x}, guid {sdl.guid(jid)}")
    buttons, axes, hats = sdl.counts(js)
    print(f"  {buttons} buttons, {axes} axes, {hats} hat(s)")
    mapping = sdl.mapping(jid)
    if not mapping:
        print("  no gamepad mapping: use --live or --check to find the numbers")
        return js
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


def expected_bindings():
    """keymap.rs's binding of each input on each layout, read from its
    `fn sdl`, so this script can't drift from it."""
    src = open(KEYMAP, encoding="utf-8").read()
    body = re.search(r"fn sdl\(pad: Controller, input: Input\)[^{]*\{(.*?)\n\}", src, re.S)
    if not body:
        sys.exit(f"`fn sdl` not found in {KEYMAP}: update padprobe.py")
    xbox, ps = {}, {}
    for name, guard, value in re.findall(r"(\w+)( if ps)?\s*=>\s*\"([^\"]+)\"", body.group(1)):
        if guard:
            ps[name] = value
        else:
            xbox[name] = value
    ps = {**xbox, **ps}
    missing = [n for n, *_ in PROMPTS if n not in xbox]
    if missing:
        sys.exit(f"keymap.rs's `fn sdl` lacks {missing}: update padprobe.py")
    return {"xbox": xbox, "playstation": ps}


def playstation_ids():
    """Sony's vendor ID and the product IDs controller.rs takes as a
    PlayStation controller."""
    src = open(CONTROLLER, encoding="utf-8").read()
    sony = int(re.search(r"const SONY: u32 = (0x[0-9A-Fa-f]+);", src).group(1), 16)
    products = re.search(r"const PLAYSTATION: &\[u32\] = &\[([^\]]+)\];", src).group(1)
    return sony, {int(p.strip(), 16) for p in products.split(",") if p.strip()}


def active(sdl, js):
    """Every input of the stick DOSBox's mapper would see as pressed, in
    its binding names."""
    sdl.lib.SDL_PumpEvents()
    sdl.update()
    buttons, axes, hats = sdl.counts(js)
    out = set()
    for b in range(buttons):
        if sdl.button(js, b):
            out.add(f"stick_0 button {b}")
    for h in range(hats):
        v = sdl.hat(js, h)
        for bit in HAT_BITS:
            if v & bit:
                out.add(f"stick_0 hat {h} {bit}")
    for a in range(axes):
        v = sdl.axis(js, a)
        if v > AXIS_PRESS:
            out.add(f"stick_0 axis {a} 1")
        elif v < -AXIS_PRESS:
            out.add(f"stick_0 axis {a} 0")
    return out


def settle(sdl, js):
    # The first states can still be zero before the pad has reported.
    for _ in range(25):
        active(sdl, js)
        time.sleep(0.02)


def check(sdl, js, version, layout, timeout):
    """Asks for each input and compares what arrives with keymap.rs."""
    want_all = expected_bindings()
    if not layout:
        sony, products = playstation_ids()
        is_ps = sdl.vendor(js) == sony and sdl.product(js) in products
        layout = "playstation" if is_ps else "xbox"
        print(f"\nLayout: {layout} (from the controller's IDs; --layout to choose)")
    else:
        print(f"\nLayout: {layout} (--layout)")
    want = want_all[layout]
    by_bind = {}
    for name, value in want.items():
        by_bind.setdefault(value, []).append(name)

    settle(sdl, js)
    rest = active(sdl, js)
    problems = []
    held = sorted(b for b in rest if b in by_bind)
    print(f"At rest: {', '.join(sorted(rest)) or 'nothing pressed'}")
    for b in held:
        msg = f"{b} is pressed at rest, so {'/'.join(by_bind[b])} would hold its key all the time"
        print(f"  DIFFERENT: {msg}")
        problems.append(msg)

    print(f"\nDo each step, then let go. Nothing within {timeout} s skips it; Ctrl+C stops.\n")
    ok = 0
    for name, xbox_text, ps_text in PROMPTS:
        text = ps_text if layout == "playstation" else xbox_text
        expect = want[name]
        print(f"  {text} ... ", end="", flush=True)
        got = set()
        start = time.time()
        while not got and time.time() - start < timeout:
            got = active(sdl, js) - rest
            time.sleep(0.01)
        if not got:
            print("nothing arrived")
            problems.append(f"{name}: nothing arrived, keymap.rs expects {expect}")
            continue
        # Collect everything until it's let go again.
        while True:
            now = active(sdl, js) - rest
            if not now:
                break
            got |= now
            time.sleep(0.01)
        seen = ", ".join(sorted(got))
        if expect in got:
            extra = got - {expect}
            print(f"ok ({expect})" + (f", also {', '.join(sorted(extra))}" if extra else ""))
            ok += 1
        else:
            print(f"DIFFERENT: got {seen}, keymap.rs expects {expect}")
            problems.append(f"{name}: got {seen}, keymap.rs expects {expect}")
        time.sleep(0.2)

    print(f"\n{version}, {layout} layout: {ok} of {len(PROMPTS)} inputs as keymap.rs binds them.")
    if problems:
        print("Differences:")
        for p in problems:
            print(f"  {p}")
    return not problems


def live(sdl, sticks):
    """Prints each button, hat and axis (past half way) as it changes."""
    print("\nPress buttons and move sticks and triggers; Ctrl+C ends.")
    for _, js in sticks:
        settle(sdl, js)
    before = {}
    try:
        while True:
            sdl.update()
            for i, js in sticks:
                buttons, axes, hats = sdl.counts(js)
                for b in range(buttons):
                    v = sdl.button(js, b)
                    if before.get((i, "b", b), 0) != v:
                        before[(i, "b", b)] = v
                        print(f"stick_{i} button {b} {'down' if v else 'up'}")
                for h in range(hats):
                    v = sdl.hat(js, h)
                    if before.get((i, "h", h), 0) != v:
                        before[(i, "h", h)] = v
                        dirs = "+".join(n for bit, n in HAT_BITS.items() if v & bit) or "centre"
                        print(f"stick_{i} hat {h} {v} ({dirs})")
                for a in range(axes):
                    raw = sdl.axis(js, a)
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
    p.add_argument("--sdl", required=True, help="the SDL2 or SDL3 library to test")
    p.add_argument("--dosbox")
    p.add_argument("--check", action="store_true")
    p.add_argument("--layout", choices=["xbox", "playstation"])
    p.add_argument("--timeout", type=float, default=15, help="seconds to wait per --check step")
    p.add_argument("--live", action="store_true")
    args = p.parse_args()

    sdl = load(args.sdl)
    version = "SDL " + ".".join(map(str, sdl.version()))
    print(f"{platform.platform()}, Python {platform.python_version()}")
    print(f"{version} ({os.path.abspath(args.sdl)})")
    hints = {k: os.environ[k] for k in os.environ if k.startswith("SDL_")}
    if hints:
        print(f"  SDL environment: {hints}")

    if not sdl.init():
        sys.exit(f"SDL_Init failed: {sdl.lib.SDL_GetError().decode()}")
    # Some backends list devices only after a moment.
    for _ in range(25):
        sdl.lib.SDL_PumpEvents()
        time.sleep(0.02)
    ids = sdl.ids()
    print(f"\n{len(ids)} controller(s)")
    sticks = []
    for i, jid in enumerate(ids):
        js = describe(sdl, i, jid)
        if js:
            sticks.append((i, js))
    if args.dosbox:
        from_dosbox(args.dosbox)

    passed = True
    if args.check:
        if not sticks or sticks[0][0] != 0:
            sys.exit("\n--check: no first controller to check.")
        if len(ids) > 1:
            print("\n--check: checks stick_0, the one DOSBox plays with; connect one controller at a time.")
        js = sticks[0][1]
        try:
            passed = check(sdl, js, version, args.layout, args.timeout)
        except KeyboardInterrupt:
            print("\nStopped.")
            passed = False
    if args.live and sticks:
        live(sdl, sticks)
    sdl.lib.SDL_Quit()
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
