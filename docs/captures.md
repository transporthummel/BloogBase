# Screenshots and videos

Players take screenshots (Ctrl+F5) and videos (Ctrl+F7) with DOSBox's own
hotkeys. BloogBase points DOSBox at the profile's capture folder and shows
what lands there on the profile page, videos included, without a codec the
system would need: DOSBox's AVI files are read and decoded by the app
itself. The code is in `src-tauri/src/captures.rs` and, for playback,
`src/profile/videoPlayback.ts` and `zmbv.ts`.

## Where they go

The launch config sets `capture_dir` to `profiles/<id>/capture/`
([dosbox.md](dosbox.md)). Screenshots are PNG files, videos AVI files; the
profile page lists each kind newest first, by modification time. Deleting
checks that the name stays inside the capture folder.

## Reading an AVI

DOSBox records ZMBV video and 16-bit PCM sound into an AVI (RIFF) file.
`read_avi` walks its chunks without decoding anything:

- `avih` gives the frame time, the frame count (and with it the length) and
  the size.
- `strh` says which stream is video (`vids`) and which is sound (`auds`);
  the sound's `strf` gives format, channels, rate and bits.
- In `movi`, `NNdc`/`NNdb` chunks are video frames of stream `NN` and
  `NNwb` sound. For each frame its offset, size and whether it's a key
  frame (bit 0 of its first byte) are noted.

DOSBox writes the list sizes only when it stops recording. A list with size
0 is a recording still going on, read to the end of the file; its length is
unknown until then. Frame sizes over 64 MB are taken as broken.

For the list, reading stops at the first frame. Playing reads the whole
index: where every frame and every chunk of sound is.

## Thumbnails

Each video gets its first frame as a thumbnail, made once and kept in
`generated/video-thumbs/`; it's made again only if the video is newer. The
first frame of a ZMBV stream is always a key frame, which can be decoded on
its own: a 7-byte header (flags, version, compression, format, block size),
then the pixels, zlib-compressed or raw, in 8-bit with a 256-color palette,
15-bit, 16-bit or 32-bit color. They're turned into RGB and written as PNG.

## Playing

The player draws onto a canvas, stretched to 4:3 as DOSBox shows it, and
plays the sound through Web Audio.

- **Reading.** The file comes through Tauri's asset protocol in range
  requests of 1000 KB each. The 24 most recently used windows (about 24 MB)
  stay in memory, so even long videos don't need to fit. Frames ahead are
  prefetched.
- **Decoding** (`ZmbvDecoder`). A key frame starts a new zlib stream; every
  later frame continues it. A delta frame moves blocks of the previous
  frame by their motion vectors and, where flagged, XORs new pixels in. An
  empty frame repeats the last one. The 8-bit palette can change with any
  frame.
- **Timing.** The sound sets the pace: it's scheduled half a second ahead,
  and frames are drawn as its clock reaches them. Without sound, or before
  the webview lets it start, the system clock does. A player that falls
  behind skips to where it should be.
- **Seeking.** Going back, or far enough ahead, starts again at the last key
  frame before the target and decodes up to it. Pausing or seeking drops
  sound read before.
