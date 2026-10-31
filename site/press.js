// The press kit's script: the drifting sky and the Copy buttons next to the
// texts. Without it, the texts can still be selected and copied by hand.

const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)");
const root = document.documentElement;

let scrollQueued = false;
addEventListener(
  "scroll",
  () => {
    if (scrollQueued || reduceMotion.matches) return;
    scrollQueued = true;
    requestAnimationFrame(() => {
      scrollQueued = false;
      root.style.setProperty("--scroll", String(window.scrollY));
    });
  },
  { passive: true },
);

// Copies a text block as plain text, paragraphs separated by a blank line.
for (const button of document.querySelectorAll(".copy")) {
  const block = button.closest(".copy-block").querySelector(".copy-text");
  const label = button.textContent.trim();
  let timer = 0;
  button.addEventListener("click", async () => {
    const paragraphs = block.matches("p") ? [block] : [...block.querySelectorAll("p")];
    const text = paragraphs.map((p) => p.textContent.trim().replace(/\s+/g, " ")).join("\n\n");
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // No clipboard (an old browser or no permission): select the text instead.
      getSelection().selectAllChildren(block);
      return;
    }
    button.textContent = button.dataset.copied;
    button.classList.add("done");
    clearTimeout(timer);
    timer = setTimeout(() => {
      button.textContent = label;
      button.classList.remove("done");
    }, 2000);
  });
}
