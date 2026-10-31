// The website's motion: the drifting sky, screenshots switching on, the
// Bloog hopping along the trail, the theme comparison and the download
// button for the visitor's system. Everything works without it.

const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)");
const root = document.documentElement;

// --- Sky ----------------------------------------------------------------------

let scrollQueued = false;
function onScroll() {
  if (scrollQueued) return;
  scrollQueued = true;
  requestAnimationFrame(() => {
    scrollQueued = false;
    root.style.setProperty("--scroll", String(window.scrollY));
    switchOnScreens();
    moveWalker();
  });
}
addEventListener("scroll", onScroll, { passive: true });

// --- Screens switching on -----------------------------------------------------

// Checked on scroll rather than with an IntersectionObserver: Chromium counts
// the screen's own clip-path, which hides all but a line of it until it's on.
let screens = [...document.querySelectorAll(".screen")];
function switchOnScreens() {
  screens = screens.filter((screen) => {
    if (screen.getBoundingClientRect().top > window.innerHeight * 0.85) return true;
    screen.classList.add("on");
    return false;
  });
}
switchOnScreens();

// --- Trail and Bloog ----------------------------------------------------------

const journey = document.querySelector(".journey");
const trail = journey.querySelector(".trail");
const ahead = trail.querySelector(".trail-ahead");
const done = trail.querySelector(".trail-done");
const clip = trail.querySelector(".trail-clip");
const walker = journey.querySelector(".walker");
const stops = [...journey.querySelectorAll(".stop")];

const narrow = matchMedia("(max-width: 860px)");
const follow = document.querySelector(".follow .bloog");

/** Points along the trail, every few pixels: { x, y } in journey coordinates. */
let samples = [];
let waypoints = [];
let last = null;
let hopTimer = 0;

/**
 * Where the trail passes a stop, level with its heading: in the gap between
 * text and screenshot, or in the margin left of a full-width stop, so it
 * never crosses text or pictures.
 */
function waypoint(stop, box) {
  const heading = stop.querySelector("h2").getBoundingClientRect();
  const rect = stop.getBoundingClientRect();
  const y = heading.top - box.top + heading.height / 2;
  const text = stop.querySelector(".station-text")?.getBoundingClientRect();
  const figure = stop.querySelector(".screen")?.getBoundingClientRect();
  let x;
  if (text && figure) {
    x = text.left < figure.left ? (text.right + figure.left) / 2 : (figure.right + text.left) / 2;
  } else {
    x = rect.left - Math.min(48, (rect.left - box.left) / 2);
  }
  return { stop, x: x - box.left, y, top: rect.top - box.top, bottom: rect.bottom - box.top };
}

function layoutTrail() {
  if (narrow.matches) {
    samples = [];
    walker.hidden = true;
    stops.forEach((stop) => stop.classList.add("lit"));
    return;
  }
  const box = journey.getBoundingClientRect();
  const start = follow.getBoundingClientRect();
  const origin = {
    x: start.left - box.left + start.width / 2,
    y: start.bottom - box.top,
    bottom: start.bottom - box.top,
  };
  waypoints = stops.map((stop) => waypoint(stop, box));
  // Down from each stop, across in the space between two stops, down to the
  // next one: always downwards, so the Bloog's height picks its place.
  let d = `M${origin.x},${origin.y}`;
  let a = origin;
  for (const b of waypoints) {
    const mid = (a.bottom + b.top) / 2;
    const swing = Math.min(56, (b.top - a.bottom) / 2 - 4);
    d += ` L${a.x},${mid - swing} C${a.x},${mid} ${b.x},${mid} ${b.x},${mid + swing} L${b.x},${b.y}`;
    a = b;
  }
  ahead.setAttribute("d", d);
  done.setAttribute("d", d);
  const length = ahead.getTotalLength();
  samples = [];
  for (let at = 0; at <= length; at += 4) {
    const p = ahead.getPointAtLength(at);
    samples.push({ x: p.x, y: p.y });
  }
  samples.push(ahead.getPointAtLength(length));
  last = null;
  moveWalker();
}

/** The trail point at a height, the Bloog's place for that scroll position. */
function pointAt(y) {
  let lo = 0;
  let hi = samples.length - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (samples[mid].y < y) lo = mid + 1;
    else hi = mid;
  }
  return samples[lo];
}

function moveWalker() {
  if (!samples.length) return;
  if (reduceMotion.matches) {
    walker.hidden = true;
    clip.setAttribute("height", "100%");
    stops.forEach((stop) => stop.classList.add("lit"));
    return;
  }
  walker.hidden = false;
  const top = journey.getBoundingClientRect().top;
  const y = Math.max(
    samples[0].y,
    Math.min(samples[samples.length - 1].y, window.innerHeight * 0.55 - top),
  );
  const p = pointAt(y);
  walker.style.transform = `translate(${p.x}px, ${p.y}px)`;
  clip.setAttribute("height", String(p.y));
  for (const w of waypoints) w.stop.classList.toggle("lit", p.y >= w.y - 2);

  if (last && (Math.abs(p.x - last.x) > 0.5 || Math.abs(p.y - last.y) > 0.5)) {
    if (Math.abs(p.x - last.x) > 0.5)
      walker.style.setProperty("--facing", p.x < last.x ? "-1" : "1");
    walker.classList.add("hopping");
    clearTimeout(hopTimer);
    hopTimer = setTimeout(() => walker.classList.remove("hopping"), 180);
  }
  last = p;
}

new ResizeObserver(layoutTrail).observe(journey);
narrow.addEventListener("change", layoutTrail);
reduceMotion.addEventListener("change", () => {
  stops.forEach((stop) => stop.classList.remove("lit"));
  layoutTrail();
});

// --- Day and night ------------------------------------------------------------

const compare = document.querySelector(".compare");
const range = compare.querySelector("input");
range.addEventListener("input", () => compare.style.setProperty("--split", `${range.value}%`));

// --- Download for this system -------------------------------------------------

const OS_NAMES = { windows: "Windows", mac: "macOS", linux: "Linux" };

async function detectSystem() {
  const data = navigator.userAgentData;
  const platform = (data?.platform || navigator.platform || "").toLowerCase();
  const agent = navigator.userAgent.toLowerCase();
  if (/android|iphone|ipad/.test(agent)) return null;
  let os = null;
  if (platform.includes("win")) os = "windows";
  else if (platform.includes("mac")) os = "mac";
  else if (platform.includes("linux")) os = "linux";
  let arm = false;
  if (os === "mac" && data?.getHighEntropyValues) {
    try {
      arm = (await data.getHighEntropyValues(["architecture"])).architecture === "arm";
    } catch {
      // Unknown: the button leads to the release page.
    }
  }
  return os && { os, arm };
}

// The latest release's files, fetched once for the button and the file links.
const release = fetch("https://api.github.com/repos/transporthummel/BloogBase/releases/latest", {
  headers: { Accept: "application/vnd.github+json" },
})
  .then((response) => (response.ok ? response.json() : null))
  .then((data) => data?.assets ?? [])
  .catch(() => []);

/** The asset whose name ends in `suffix`, if the latest release has one. */
async function asset(suffix) {
  return (await release).find((file) => file.name.endsWith(suffix)) ?? null;
}

// The file names under each system link straight to their files, with the
// version filled in; without the release they lead to its page.
for (const link of document.querySelectorAll(".asset")) {
  asset(link.dataset.asset).then((file) => {
    if (!file) return;
    link.href = file.browser_download_url;
    const code = link.querySelector("code");
    if (code.textContent.includes("…")) code.textContent = file.name;
  });
}

// The installer the button offers on each system; for Macs only when the
// browser says it's Apple Silicon.
const INSTALLERS = { windows: "_x64-setup.exe", linux: "_amd64.AppImage" };

detectSystem().then(async (system) => {
  if (!system) return;
  document.querySelector(`.system[data-os="${system.os}"]`)?.classList.add("mine");
  const buttons = document.querySelectorAll(".download-button");
  for (const button of buttons) {
    button.querySelector("span").textContent = button.dataset.label.replace(
      "{os}",
      OS_NAMES[system.os],
    );
  }
  const suffix = system.os === "mac" ? system.arm && "_aarch64.dmg" : INSTALLERS[system.os];
  const file = suffix && (await asset(suffix));
  // Only the download section's button: the hero's leads to its hints.
  const direct = document.querySelectorAll(".download .download-button");
  if (file) direct.forEach((button) => (button.href = file.browser_download_url));
});
