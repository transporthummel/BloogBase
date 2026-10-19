import { Inflate, Z_SYNC_FLUSH } from "pako";

/** Bytes per pixel of the ZMBV formats DOSBox writes. */
const BYTES: Record<number, number> = { 4: 1, 5: 2, 6: 2, 8: 4 };

/**
 * Decodes ZMBV, DOSBox's video codec. A key frame starts a new zlib stream;
 * each later frame continues it and moves blocks of the previous frame,
 * optionally XORed with new pixels.
 */
export class ZmbvDecoder {
  private frame: Uint8Array;
  private previous: Uint8Array;
  private palette = new Uint8Array(768);
  private format = 0;
  private bytes = 0;
  private compressed = false;
  private blockWidth = 0;
  private blockHeight = 0;
  private inflate: Inflate | null = null;
  /** Whether a key frame came since the last reset. */
  ready = false;

  constructor(
    readonly width: number,
    readonly height: number,
  ) {
    this.frame = new Uint8Array(width * height * 4);
    this.previous = new Uint8Array(width * height * 4);
  }

  /** Decodes the next frame; an empty one repeats the last. */
  decode(data: Uint8Array) {
    if (data.length === 0) return;
    const flags = data[0];
    let body: Uint8Array;
    if (flags & 1) {
      const [, , , compression, format, blockWidth, blockHeight] = data;
      if (!(format in BYTES) || compression > 1 || !blockWidth || !blockHeight) {
        throw new Error(`unsupported ZMBV format ${format}`);
      }
      this.format = format;
      this.bytes = BYTES[format];
      this.compressed = compression === 1;
      this.blockWidth = blockWidth;
      this.blockHeight = blockHeight;
      this.inflate = this.compressed ? new Inflate() : null;
      body = data.subarray(7);
    } else {
      if (!this.ready) return;
      body = data.subarray(1);
    }
    const work = this.compressed ? this.unzip(body) : body;
    if (flags & 1) {
      this.key(work);
      this.ready = true;
    } else {
      this.delta(work, (flags & 2) !== 0);
    }
  }

  private unzip(body: Uint8Array): Uint8Array {
    const inflate = this.inflate!;
    const chunks: Uint8Array[] = [];
    inflate.onData = (chunk) => chunks.push(chunk);
    if (!inflate.push(body, Z_SYNC_FLUSH) && inflate.err) throw new Error(inflate.msg);
    if (chunks.length === 1) return chunks[0];
    const out = new Uint8Array(chunks.reduce((n, c) => n + c.length, 0));
    let at = 0;
    for (const c of chunks) {
      out.set(c, at);
      at += c.length;
    }
    return out;
  }

  private key(work: Uint8Array) {
    let at = 0;
    if (this.format === 4) {
      this.palette.set(work.subarray(0, 768));
      at = 768;
    }
    const size = this.width * this.height * this.bytes;
    if (work.length < at + size) throw new Error("short ZMBV key frame");
    this.frame.set(work.subarray(at, at + size));
  }

  private delta(work: Uint8Array, newPalette: boolean) {
    [this.previous, this.frame] = [this.frame, this.previous];
    const { width, height, bytes, blockWidth, blockHeight, previous, frame } = this;
    let at = 0;
    if (newPalette) {
      for (let i = 0; i < 768; i++) this.palette[i] ^= work[at++];
    }
    const blocksX = Math.ceil(width / blockWidth);
    const blocksY = Math.ceil(height / blockHeight);
    const vectors = at;
    // The XOR data starts 4-byte aligned after the vectors.
    at = (at + blocksX * blocksY * 2 + 3) & ~3;

    let block = 0;
    for (let by = 0; by < blocksY; by++) {
      for (let bx = 0; bx < blocksX; bx++, block++) {
        const vx = (work[vectors + block * 2] << 24) >> 24;
        const vy = (work[vectors + block * 2 + 1] << 24) >> 24;
        const xor = (vx & 1) === 1;
        const dx = vx >> 1;
        const dy = vy >> 1;
        const x0 = bx * blockWidth;
        const y0 = by * blockHeight;
        const w = Math.min(blockWidth, width - x0);
        const h = Math.min(blockHeight, height - y0);
        for (let y = y0; y < y0 + h; y++) {
          const sy = y + dy;
          for (let x = x0; x < x0 + w; x++) {
            const sx = x + dx;
            const to = (y * width + x) * bytes;
            // Outside the picture is black.
            const inside = sx >= 0 && sx < width && sy >= 0 && sy < height;
            const from = (sy * width + sx) * bytes;
            for (let b = 0; b < bytes; b++) {
              const old = inside ? previous[from + b] : 0;
              frame[to + b] = xor ? old ^ work[at++] : old;
            }
          }
        }
      }
    }
  }

  /** Writes the current frame as RGBA. */
  toRgba(out: Uint8ClampedArray) {
    const { frame, palette } = this;
    const pixels = this.width * this.height;
    if (this.format === 4) {
      for (let i = 0; i < pixels; i++) {
        const p = frame[i] * 3;
        out[i * 4] = palette[p];
        out[i * 4 + 1] = palette[p + 1];
        out[i * 4 + 2] = palette[p + 2];
        out[i * 4 + 3] = 255;
      }
    } else if (this.format === 8) {
      for (let i = 0; i < pixels * 4; i += 4) {
        out[i] = frame[i + 2];
        out[i + 1] = frame[i + 1];
        out[i + 2] = frame[i];
        out[i + 3] = 255;
      }
    } else {
      const sixBitGreen = this.format === 6;
      for (let i = 0; i < pixels; i++) {
        const v = frame[i * 2] | (frame[i * 2 + 1] << 8);
        const g = sixBitGreen ? (v >> 5) & 63 : (v >> 5) & 31;
        out[i * 4] = five(sixBitGreen ? v >> 11 : v >> 10);
        out[i * 4 + 1] = sixBitGreen ? (g << 2) | (g >> 4) : five(g);
        out[i * 4 + 2] = five(v);
        out[i * 4 + 3] = 255;
      }
    }
  }
}

function five(v: number): number {
  const c = v & 31;
  return (c << 3) | (c >> 2);
}
