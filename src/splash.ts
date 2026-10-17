/** Shortest time the boot splash stays, so it doesn't just flicker. */
const MIN_VISIBLE_MS = 700;

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
