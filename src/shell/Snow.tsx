import { useEffect, useRef } from "react";
import { isChristmas } from "./christmas";

const FLAKES_PER_PIXEL = 1 / 12000;

type Flake = { x: number; y: number; size: number; speed: number; drift: number; phase: number };

/**
 * Pixel snow drifting over the window at Christmas. It never takes clicks and
 * stays off when the OS asks for reduced motion.
 */
export function Snow() {
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    if (!isChristmas(new Date())) return;
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;
    const el = canvas.current;
    const ctx = el?.getContext("2d");
    if (!el || !ctx) return;

    let flakes: Flake[] = [];
    const flake = (y: number): Flake => ({
      x: Math.random() * el.width,
      y,
      // Whole pixels, like the games' own sprites.
      size: 2 + Math.floor(Math.random() * 2),
      speed: 20 + Math.random() * 30,
      drift: 8 + Math.random() * 12,
      phase: Math.random() * Math.PI * 2,
    });
    const resize = () => {
      el.width = window.innerWidth;
      el.height = window.innerHeight;
      const count = Math.round(el.width * el.height * FLAKES_PER_PIXEL);
      flakes = Array.from({ length: count }, () => flake(Math.random() * el.height));
    };
    resize();
    window.addEventListener("resize", resize);

    let frame = 0;
    let last = performance.now();
    const draw = (now: number) => {
      // Capped, so a frame after a hidden window doesn't jump.
      const dt = Math.min(now - last, 100) / 1000;
      last = now;
      ctx.clearRect(0, 0, el.width, el.height);
      // White vanishes on the light theme; a soft lavender still reads as snow.
      ctx.fillStyle = document.documentElement.dataset.theme === "light" ? "#a19dc4" : "#ffffff";
      for (const [i, f] of flakes.entries()) {
        f.y += f.speed * dt;
        f.phase += dt;
        if (f.y > el.height) flakes[i] = flake(-f.size);
        ctx.globalAlpha = f.size === 2 ? 0.5 : 0.8;
        ctx.fillRect(
          Math.round(f.x + Math.sin(f.phase) * f.drift),
          Math.round(f.y),
          f.size,
          f.size,
        );
      }
      frame = requestAnimationFrame(draw);
    };
    frame = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("resize", resize);
    };
  }, []);

  if (!isChristmas(new Date())) return null;
  // Below menus and dialogs, above the pages.
  return <canvas ref={canvas} aria-hidden className="pointer-events-none fixed inset-0 z-30" />;
}
