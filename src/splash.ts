/**
 * Shortest time the boot splash stays: until both shooting stars of
 * index.html have crossed the sky (the second starts at 0.7s and flies 1s).
 */
const MIN_VISIBLE_MS = 1800;

let hidden = false;

/** Fades out the boot splash from index.html once the app has rendered. */
export function hideSplash() {
  if (hidden) return;
  hidden = true;
  const splash = document.getElementById("splash");
  if (!splash) return;
  const wait = Math.max(0, MIN_VISIBLE_MS - performance.now());
  setTimeout(() => {
    splash.classList.add("hidden");
    // Also when transitions are off (reduced motion): no transitionend then.
    setTimeout(() => splash.remove(), 500);
  }, wait);
}
