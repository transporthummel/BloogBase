# DOSBox Staging

BloogBase doesn't emulate anything itself: every game runs in
[DOSBox Staging](https://www.dosbox-staging.org/), started as a separate
program. This covers how BloogBase gets DOSBox, how it configures it, what
it gives DOSBox to run, and what it reads back. The code is mostly in
`src-tauri/src/runtime.rs`, `dosconf.rs` and `launcher.rs`.

## The runtime

### Managed install

BloogBase pins one DOSBox Staging version, currently 0.83.0, in
`src-tauri/runtimes/dosbox-staging.json`: for Windows, Linux and macOS the
URL of the official GitHub release, its SHA-256, size, archive type and
where the executable is inside. The setup wizard and the settings page offer
to install it:

1. The archive is downloaded into the cache and checked against size and
   SHA-256.
2. It's extracted into a scratch folder: ZIP on Windows, `.tar.xz` on Linux.
   On macOS the DMG is attached with `hdiutil` and the `.app` is copied with
   `ditto`, which keeps its signature, then the quarantine attribute is
   removed. A single top-level folder is stripped.
3. If the executable is there, the folder replaces
   `runtimes/dosbox-staging/<version>/` and the runtime is registered in the
   database.
4. Except on macOS, an empty `dosbox-staging.conf` next to the executable
   puts Staging in portable mode: it keeps its own config folder (shell
   history, mapper files) there instead of sharing the user's. Writing into
   the macOS app bundle would break its signature.

Only one install runs at a time. Platforms without a pinned build (e.g.
Linux on ARM) can only use their own DOSBox.

### Custom executable

On the settings page the player can pick their own DOSBox Staging instead.
BloogBase runs it with `--version` (10 seconds at most) and accepts it only if
the banner says DOSBox Staging; plain DOSBox and other forks are refused,
since launches rely on Staging's overlay mounts and command-line switches.
A custom executable wins over the managed one while it's set; clearing it
goes back to the managed one. Among managed installs, the pinned version
wins, else the newest. Runtimes whose executable is gone are skipped.

Changing the managed version means checking everything that copies Staging
0.83's behavior, above all the default key bindings in `keymap.rs`.

## Configuration

Each launch loads five config files with `--conf`, in this order; a later
one wins for every setting it sets:

| Layer    | File                                                  | Written                                 |
| -------- | ----------------------------------------------------- | --------------------------------------- |
| Defaults | `configs/defaults/<engine>.conf` or the profile's own | Every launch, from `src-tauri/configs/` |
| Options  | `configs/options.conf`                                | Every launch, from the settings page    |
| Global   | `configs/global-override.conf`                        | By the player; a template if missing    |
| Profile  | `profiles/<id>/config/override.conf`                  | By the player; a template if missing    |
| Launch   | `profiles/<id>/generated/launch.conf`                 | Every launch                            |

- **Defaults** per engine: `aspect = on`, the normal core, 4000 cycles for
  the speed-sensitive Keen 1-3, 20000 for Keen 4-6 and Keen Dreams. A mod
  whose patch needs another machine gets a defaults file of its own with
  `machine = cga` (composite CGA graphics, Staging then switches on its
  composite emulation) or `machine = svga_paradise` (128K of video memory
  at A0000h). The patch file decides ([recipes.md](recipes.md)).
- **Options** from the settings page: fullscreen, CRT shader (`crt-auto`,
  else `sharp`) and vsync, plus the keyboard mapper file if there is one.
  If the global override sets the same setting, the settings page says so
  and disables the switch.
- **Global and profile overrides** are the player's own DOSBox settings,
  edited in the app (settings page and the profile's config tab, with
  CodeMirror). Saving reports lines DOSBox would skip: syntax errors,
  settings before the first section, and `[autoexec]` sections, which never
  run. The profile's tab also shows the defaults and what all layers add up
  to, with the layer each setting comes from.
- **Launch** settings belong to the launch itself and come last so they
  can't be broken by accident: `automount = off`, `startup_verbosity =
quiet`, the window title ("<game or mod> – BloogBase", without cycles and
  mouse hints) and `capture_dir`. DOSBox resolves a relative `capture_dir`
  against the config file's folder, so it's `../capture`.

Files are only written when their content changes.

### The keyboard

Keen and most DOS games read raw scancodes, so DOSBox's `keyboard_layout`
doesn't reach them: on a German keyboard the key labeled Y arrives as Z. On
Windows, BloogBase asks for the active layout's key for each letter. If any
letter moves, it writes `configs/keyboard.map`, a mapper file that binds
each emulated letter to the host key that types it. A mapper file replaces
all of DOSBox's default bindings, so it repeats Staging 0.83's
(`CreateDefaultBinds` in `gui/mapper.cpp`) and moves only the letters. On
US layouts, and outside Windows, there is no mapper file and DOSBox keeps
its own defaults.

## What DOSBox runs

### Drive C

The game's files are never written to. Drive C is two layers:

- **Lower:** the game install in the library, read only. For a mod, or
  with one-button firing on, a composed folder in the cache: the install,
  the mod below if any, and the mod's files on top, plus whatever the start
  needs (an unpacked executable for old patchers, CKPatch for mods without
  one, CWSDPMI for DJGPP programs, the firing patch). It's rebuilt when its
  inputs or the recipe change (`compose.rs`).
- **Upper:** `profiles/<id>/state/`, mounted over it with Staging's overlay
  mount. Everything the game writes (saves, settings, high scores) lands
  there. Staging marks deleted files with `DBOVERLAY_DEL_` and
  `DBOVERLAY_RMD_` files; `overlay_view.rs` applies them so saves, backups
  and covers see the same files DOS does.

### The command line

```
dosbox --noprimaryconf --nolocalconf
       --conf <defaults> --conf <options> --conf <global> --conf <profile> --conf <launch>
       --noautoexec
       -c "@mount C \"<lower>\" > NUL"
       -c "@mount C \"<upper>\" -t overlay > NUL"
       -c "@mount X \"<exit folder>\" > NUL"
       -c "@C:"
       -c "@<start>"                (or "@call <file>.BAT")
       -c "X:\SCREEN.COM X:\SCREEN.BIN"
       -c "exit"
```

- `--noprimaryconf` and `--nolocalconf` keep the player's own
  `dosbox-staging.conf` and any `dosbox.conf` in the working folder out.
- Staging joins the `[autoexec]` sections of all config files. Instead of
  relying on `autoexec_section = overwrite`, launches pass `--noautoexec`
  and send their commands with `-c`, so no `[autoexec]` ever runs.
- `@` and `> NUL` keep BloogBase's own commands off the screen. A batch file
  is started with `call`, or `exit` would never run.
- The start is the mod's recipe ([recipes.md](recipes.md)), or the version's
  start program or the episode's executable, in catalog order.
- DOSBox Staging 0.83 crashes on mount paths with non-ASCII characters (an
  umlaut in the Windows user name), so it runs in the data folder and gets
  only paths relative to it.

DOSBox's console window is hidden on Windows. Its output goes to
`logs/dosbox-<profile>.log`. The diagnostics on the About page include the
end of the last played profile's log, next to the end of `logs/bloogbase.log`.

## While it runs

- **One at a time per profile.** Starting a profile that's already running
  is refused. Different profiles may run side by side.
- **Startup.** The app shows the launch until DOSBox's log reports the
  first video mode it presents (`DISPLAY: … mode … scaled to …`). Then, on
  Windows, the DOSBox window gets BloogBase's terminal icon, so the taskbar
  tells it apart from the app.
- **Play time.** A session starts at launch and ends when DOSBox exits,
  with its exit code.
- **Backups.** Saves are backed up before the launch and compared after the
  session.
- **Screenshots and videos** use DOSBox's own hotkeys (Ctrl+F5, Ctrl+F7) and
  land in the profile's capture folder, where the profile page shows them.
  Videos are DOSBox's ZMBV AVI files; the app reads their first frame and
  length itself.
- **Quitting BloogBase** while games run asks first: stop them, so their
  play time counts in full, or let them run on, ending their sessions now.

## After it ends

DOSBox closes right after the game, so nobody could read what it printed,
and patchers end with errorlevel 0 even after an error. So after the game,
a 72-byte program (`SCREEN.COM`, written into the profile's
`generated/exit/` folder, mounted as X:) saves the 80×25 text screen to
`SCREEN.BIN`. It does nothing in a graphics mode.

BloogBase then takes the lines between the prompt that started the game
and the one that saved the screen. They only count as a message if all of
them are plain light-grey text: games end with colorful screens (like
Keen's ordering info) on purpose, while DOS, patchers and error messages
print plain text. Farewells of tools such as the Adlib music TSR are left
out. The app shows the message once the game has closed, e.g. CKPatch's
"Unrecognised version of KEEN4C.EXE".

## Licenses

DOSBox Staging is GPL-2.0-or-later. BloogBase downloads the official,
unmodified release and starts it as a separate program; the About page
links its source code.
