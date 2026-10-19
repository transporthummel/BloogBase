import type { VideoIndex } from "../bindings";
import { ZmbvDecoder } from "./zmbv";

/** The most the asset protocol returns for one range request. */
const WINDOW = 1000 * 1024;
/** Windows kept in memory: about 24 MB. */
const KEEP = 24;
/** Sound is scheduled this many seconds ahead. */
const AUDIO_AHEAD = 0.5;
const EMPTY = new Uint8Array(0);

/** Reads a file through the asset protocol, in windows it keeps for a while. */
class RangeReader {
  private windows = new Map<number, Promise<Uint8Array>>();

  constructor(private url: string) {}

  private window(n: number): Promise<Uint8Array> {
    let w = this.windows.get(n);
    if (w) {
      // Least recently used goes first.
      this.windows.delete(n);
    } else {
      w = fetch(this.url, {
        headers: { Range: `bytes=${n * WINDOW}-${(n + 1) * WINDOW - 1}` },
      }).then(async (res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const bytes = new Uint8Array(await res.arrayBuffer());
        // Without range support, the whole file comes back.
        return res.status === 206 ? bytes : bytes.subarray(n * WINDOW, (n + 1) * WINDOW);
      });
      w.catch(() => this.windows.delete(n));
      if (this.windows.size >= KEEP) this.windows.delete(this.windows.keys().next().value!);
    }
    this.windows.set(n, w);
    return w;
  }

  async read(offset: number, size: number): Promise<Uint8Array> {
    const first = Math.floor(offset / WINDOW);
    const last = Math.floor((offset + size - 1) / WINDOW);
    if (first === last) {
      const at = offset - first * WINDOW;
      return (await this.window(first)).subarray(at, at + size);
    }
    const out = new Uint8Array(size);
    for (let n = first; n <= last; n++) {
      const from = Math.max(offset, n * WINDOW);
      const to = Math.min(offset + size, (n + 1) * WINDOW);
      const w = await this.window(n);
      out.set(w.subarray(from - n * WINDOW, to - n * WINDOW), from - offset);
    }
    return out;
  }

  prefetch(offset: number) {
    this.window(Math.floor(offset / WINDOW)).catch(() => {});
  }
}

/**
 * Plays a DOSBox video on a canvas, with its sound through Web Audio. The
 * sound sets the pace; without it (or before the webview lets it start) the
 * system clock does.
 */
export class VideoPlayback {
  readonly duration: number;
  error: unknown = null;
  private reader: RangeReader;
  private decoder: ZmbvDecoder;
  private image: ImageData;
  private canvas: CanvasRenderingContext2D;
  /** Frame number the decoder is at, and the one on the canvas. */
  private decoded = -1;
  private shown = -1;
  private decoding = false;
  private playing = false;
  /** Clock time of media time 0 while playing. */
  private started = 0;
  private pausedAt = 0;
  private audio: AudioContext | null = null;
  private audioClock = false;
  private sources = new Set<AudioBufferSourceNode>();
  /** Sample up to which sound is scheduled. */
  private scheduled = 0;
  private scheduling = false;
  /** Bumped on every pause and seek, to drop sound read before it. */
  private generation = 0;
  /** First sample of each audio chunk, plus the total at the end. */
  private sampleStarts: number[] = [0];
  private frame = 0;
  private lastNotice = 0;

  constructor(
    url: string,
    private index: VideoIndex,
    canvas: HTMLCanvasElement,
    /** Called when playing starts or stops, and a few times a second. */
    private onChange: () => void,
  ) {
    this.reader = new RangeReader(url);
    this.decoder = new ZmbvDecoder(index.width, index.height);
    this.duration = (index.frames.length * index.frameUs) / 1e6;
    canvas.width = index.width;
    canvas.height = index.height;
    this.canvas = canvas.getContext("2d")!;
    this.image = this.canvas.createImageData(index.width, index.height);
    for (const [, size] of index.audio?.chunks ?? []) {
      const total = this.sampleStarts[this.sampleStarts.length - 1];
      this.sampleStarts.push(total + size / (2 * index.audio!.channels));
    }
    this.frame = requestAnimationFrame(this.tick);
  }

  get paused(): boolean {
    return !this.playing;
  }

  get time(): number {
    return this.playing ? Math.min(this.now() - this.started, this.duration) : this.pausedAt;
  }

  private now(): number {
    return this.audioClock ? this.audio!.currentTime : performance.now() / 1000;
  }

  play() {
    if (this.playing || this.error) return;
    if (this.pausedAt >= this.duration) this.pausedAt = 0;
    if (this.index.audio && !this.audio) {
      this.audio = new AudioContext();
      // Webviews may hold sound back until the player interacts.
      this.audio.onstatechange = () => {
        if (this.audio?.state === "running" && this.playing && !this.audioClock) {
          const t = this.time;
          this.audioClock = true;
          this.restart(t);
        }
      };
    }
    this.audio?.resume().catch(() => {});
    this.audioClock = this.audio?.state === "running";
    this.playing = true;
    this.restart(this.pausedAt);
    this.onChange();
  }

  pause() {
    if (!this.playing) return;
    this.pausedAt = this.time;
    this.playing = false;
    this.stopSound();
    this.onChange();
  }

  seek(t: number) {
    t = Math.max(0, Math.min(t, this.duration));
    if (this.playing) this.restart(t);
    else this.pausedAt = t;
    this.onChange();
  }

  destroy() {
    cancelAnimationFrame(this.frame);
    this.playing = false;
    this.stopSound();
    this.audio?.close().catch(() => {});
  }

  /** Continues playing from media time `t`. */
  private restart(t: number) {
    this.stopSound();
    this.started = this.now() - t;
    this.scheduled = Math.round(t * (this.index.audio?.rate ?? 0));
  }

  private stopSound() {
    this.generation++;
    for (const source of this.sources) source.stop();
    this.sources.clear();
  }

  private tick = () => {
    this.frame = requestAnimationFrame(this.tick);
    const t = this.time;
    if (this.playing && t >= this.duration) {
      this.pausedAt = this.duration;
      this.playing = false;
      this.stopSound();
      this.onChange();
    }
    const frames = this.index.frames.length;
    void this.show(Math.min(frames - 1, Math.floor((t * 1e6) / this.index.frameUs)));
    if (this.playing && this.audioClock) void this.scheduleSound(t);
    if (this.playing && performance.now() - this.lastNotice > 200) {
      this.lastNotice = performance.now();
      this.onChange();
    }
  };

  private fail(err: unknown) {
    this.error = err;
    this.playing = false;
    this.stopSound();
    this.onChange();
  }

  /** Decodes up to frame `target` and draws it. */
  private async show(target: number) {
    if (this.decoding || target === this.shown || target < 0 || this.error) return;
    this.decoding = true;
    try {
      const { frames, keyFrames } = this.index;
      // Going back, or far enough ahead, starts over at a key frame.
      const k = lastAtMost(keyFrames, target);
      const key = k < 0 ? 0 : keyFrames[k];
      let from = this.decoded + 1;
      if (this.decoded > target || key > this.decoded) from = key;
      for (let i = from; i <= target; i++) {
        const [offset, size] = frames[i];
        this.decoder.decode(size ? await this.reader.read(offset, size) : EMPTY);
        this.decoded = i;
      }
      if (this.decoder.ready) {
        this.decoder.toRgba(this.image.data);
        this.canvas.putImageData(this.image, 0, 0);
      }
      this.shown = target;
      // Two seconds ahead.
      const ahead =
        frames[Math.min(frames.length - 1, target + Math.ceil(2e6 / this.index.frameUs))];
      this.reader.prefetch(ahead[0]);
    } catch (err) {
      this.fail(err);
    } finally {
      this.decoding = false;
    }
  }

  private async scheduleSound(t: number) {
    const audio = this.index.audio;
    const context = this.audio;
    if (!audio || !context || this.scheduling) return;
    const total = this.sampleStarts[this.sampleStarts.length - 1];
    // Fallen behind: skip what's past.
    const from = Math.max(this.scheduled, Math.round(t * audio.rate));
    if (from >= total || from > (t + AUDIO_AHEAD) * audio.rate) return;
    const to = Math.min(total, from + Math.round(AUDIO_AHEAD * audio.rate));
    const generation = this.generation;
    this.scheduling = true;
    try {
      const pcm = await this.readSamples(from, to);
      if (generation !== this.generation) return;
      const channels = audio.channels;
      const buffer = context.createBuffer(channels, to - from, audio.rate);
      for (let c = 0; c < channels; c++) {
        const samples = new Float32Array(to - from);
        for (let i = 0; i < samples.length; i++) samples[i] = pcm[i * channels + c] / 32768;
        buffer.copyToChannel(samples, c);
      }
      const source = context.createBufferSource();
      source.buffer = buffer;
      source.connect(context.destination);
      const when = this.started + from / audio.rate;
      if (when >= context.currentTime) source.start(when);
      else source.start(0, context.currentTime - when);
      source.onended = () => this.sources.delete(source);
      this.sources.add(source);
      this.scheduled = to;
    } catch (err) {
      this.fail(err);
    } finally {
      this.scheduling = false;
    }
  }

  /** Interleaved 16-bit samples `from` up to `to`. */
  private async readSamples(from: number, to: number): Promise<Int16Array> {
    const { chunks, channels } = this.index.audio!;
    const out = new Int16Array((to - from) * channels);
    let chunk = Math.max(0, lastAtMost(this.sampleStarts, from));
    for (; chunk < chunks.length && this.sampleStarts[chunk] < to; chunk++) {
      const start = this.sampleStarts[chunk];
      const [offset, size] = chunks[chunk];
      if (size === 0) continue;
      const bytes = await this.reader.read(offset, size);
      const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
      const first = Math.max(from, start);
      const last = Math.min(to, this.sampleStarts[chunk + 1]);
      for (let s = first; s < last; s++) {
        for (let c = 0; c < channels; c++) {
          out[(s - from) * channels + c] = view.getInt16(((s - start) * channels + c) * 2, true);
        }
      }
    }
    return out;
  }
}

/** Index of the last value in the sorted `list` that's at most `n`, or -1. */
function lastAtMost(list: number[], n: number): number {
  let lo = 0;
  let hi = list.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (list[mid] <= n) lo = mid + 1;
    else hi = mid;
  }
  return lo - 1;
}
