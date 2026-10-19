import { Deflate, Z_SYNC_FLUSH } from "pako";
import { expect, test } from "vitest";
import { ZmbvDecoder } from "./zmbv";

/** Compresses frames as one zlib stream, flushed after each, as DOSBox does. */
function stream() {
  const deflate = new Deflate();
  return (data: number[]) => {
    const chunks: Uint8Array[] = [];
    deflate.onData = (c) => chunks.push(c);
    deflate.push(new Uint8Array(data), Z_SYNC_FLUSH);
    return chunks.flatMap((c) => [...c]);
  };
}

function rgba(decoder: ZmbvDecoder): number[] {
  const out = new Uint8ClampedArray(decoder.width * decoder.height * 4);
  decoder.toRgba(out);
  return [...out];
}

test("decodes 8-bit key frames and delta frames that move and change blocks", () => {
  // 3×2 pixels in 2×2 blocks: two blocks, the right one only one pixel wide.
  const decoder = new ZmbvDecoder(3, 2);
  const zip = stream();
  const palette = Array<number>(768).fill(0);
  palette.splice(3, 6, 10, 20, 30, 40, 50, 60);
  decoder.decode(new Uint8Array([1, 0, 1, 1, 4, 2, 2, ...zip([...palette, 1, 2, 0, 0, 1, 2])]));
  const c1 = [10, 20, 30, 255];
  const c2 = [40, 50, 60, 255];
  const black = [0, 0, 0, 255];
  expect(rgba(decoder)).toEqual([...c1, ...c2, ...black, ...black, ...c1, ...c2]);

  // Left block: copied from one pixel right and XORed; right block: moved
  // one pixel down, reading black from below the picture. Then the palette
  // changes color 1.
  const vectors = [(1 << 1) | 1, 0, 0, 1 << 1];
  const xor = [3, 0, 0, 0];
  const paletteXor = Array<number>(768).fill(0);
  paletteXor[3] = 10 ^ 99;
  decoder.decode(new Uint8Array([2, ...zip([...paletteXor, ...vectors, ...xor])]));
  const c1b = [99, 20, 30, 255];
  // Pixels: 1 0 2 / 1 2 0.
  expect(rgba(decoder)).toEqual([...c1b, ...black, ...c2, ...c1b, ...c2, ...black]);

  // Empty frames repeat the last one.
  decoder.decode(new Uint8Array(0));
  expect(rgba(decoder)).toEqual([...c1b, ...black, ...c2, ...c1b, ...c2, ...black]);
});

test("decodes 32-bit frames and waits for a key frame", () => {
  const decoder = new ZmbvDecoder(1, 1);
  decoder.decode(new Uint8Array([0, 0, 0, 0, 0]));
  expect(decoder.ready).toBe(false);
  decoder.decode(new Uint8Array([1, 0, 1, 0, 8, 16, 16, 3, 2, 1, 0]));
  expect(rgba(decoder)).toEqual([1, 2, 3, 255]);
  expect(() => decoder.decode(new Uint8Array([1, 0, 1, 0, 9, 16, 16]))).toThrow();
});
