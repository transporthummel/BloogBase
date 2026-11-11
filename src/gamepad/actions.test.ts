import { afterEach, describe, expect, it, vi } from "vitest";
import { activate, back, move, PAGE_SHARE, scrollPage, stepSelect, toggleNav } from "./actions";
import { nextFocus, padScope } from "./spatial";

/** Places `el` at x/y with size w/h; jsdom lays nothing out. */
function at<T extends Element>(el: T, x: number, y: number, w = 100, h = 30): T {
  el.getBoundingClientRect = () => new DOMRect(x, y, w, h);
  return el;
}

function page(html: string) {
  document.body.innerHTML = html;
  const get = (id: string) => document.getElementById(id) as HTMLElement;
  return get;
}

afterEach(() => {
  document.body.innerHTML = "";
  delete document.documentElement.dataset.input;
});

describe("nextFocus", () => {
  it("takes the nearest element in the direction, not the next in the tab order", () => {
    const get = page(`
      <button id="a">a</button><button id="far">far</button>
      <button id="right">right</button><button id="below">below</button>`);
    at(get("a"), 0, 0);
    at(get("far"), 0, 300);
    at(get("right"), 150, 0);
    at(get("below"), 20, 50);
    expect(nextFocus(get("a"), "down", document.body)?.id).toBe("below");
    expect(nextFocus(get("a"), "right", document.body)?.id).toBe("right");
    expect(nextFocus(get("a"), "up", document.body)).toBeNull();
  });

  it("prefers what's in line to what's nearer off to the side", () => {
    // The back link above the tabs, the sidebar's first item down and left.
    const get = page(
      `<a id="nav" href="#">nav</a><a id="from" href="#">back</a><button id="tab">tab</button>`,
    );
    at(get("nav"), 0, 80, 240, 40);
    at(get("from"), 360, 30, 100, 24);
    at(get("tab"), 360, 450, 120, 40);
    expect(nextFocus(get("from"), "down", document.body)?.id).toBe("tab");
    // Out of line, the nearest wins.
    at(get("tab"), 600, 450, 120, 40);
    expect(nextFocus(get("from"), "down", document.body)?.id).toBe("nav");
  });

  it("goes straight down into a nearer section, not to its top", () => {
    // tv-lrud-spatial would land on #top: a section is a container there.
    const get = page(`
      <section><button id="top">top</button><button id="bottom">bottom</button></section>
      <section><button id="from">from</button></section>`);
    at(get("top"), 0, 0);
    at(get("bottom"), 0, 100);
    at(get("from"), 0, 200);
    expect(nextFocus(get("from"), "up", document.body)?.id).toBe("bottom");
  });

  it("skips what can't take the focus", () => {
    const get = page(`
      <button id="from">from</button>
      <button id="off" disabled>off</button>
      <a id="nohref">no link</a>
      <button id="minus" tabindex="-1">minus</button>
      <button id="ignored" data-pad-ignore>ignored</button>
      <div inert><button id="inert">inert</button></div>
      <button id="hidden" style="visibility: hidden">hidden</button>
      <button id="gone">gone</button>
      <select id="select"></select>`);
    at(get("from"), 0, 0);
    for (const id of ["off", "nohref", "minus", "ignored", "inert", "hidden"]) at(get(id), 0, 40);
    at(get("select"), 0, 80);
    // "gone" keeps jsdom's empty rectangle, as with display: none.
    expect(nextFocus(get("from"), "down", document.body)?.id).toBe("select");
  });

  it("lets a candidate overlap the edge it is reached over by less than 30 %", () => {
    const get = page(`<button id="from">from</button><button id="side">side</button>`);
    at(get("from"), 0, 0, 100, 100);
    at(get("side"), 0, 75, 100, 100);
    expect(nextFocus(get("from"), "down", document.body)?.id).toBe("side");
    at(get("side"), 0, 65, 100, 100);
    expect(nextFocus(get("from"), "down", document.body)).toBeNull();
  });
});

describe("padScope", () => {
  it("is the innermost dialog, an open menu, else the page", () => {
    const get = page(`
      <div role="dialog" id="outer"><div role="dialog" id="inner"></div></div>`);
    expect(padScope()).toBe(get("inner"));
    get("inner").innerHTML = `<div role="menu" id="menu"></div>`;
    expect(padScope()).toBe(get("menu"));
    document.body.innerHTML = "";
    expect(padScope()).toBe(document.body);
  });
});

describe("move", () => {
  it("stays inside the dialog", () => {
    const get = page(`
      <button id="behind">behind</button>
      <div role="dialog"><button id="from">from</button></div>`);
    at(get("behind"), 0, 200);
    at(get("from"), 0, 0).focus();
    move("down");
    expect(document.activeElement?.id).toBe("from");
  });

  it("starts at the first element in view in <main>", () => {
    const get = page(`
      <nav><a id="nav" href="#">nav</a></nav>
      <main><button id="above">above</button><button id="first">first</button></main>`);
    at(get("nav"), 0, 0);
    at(get("above"), 100, -200);
    at(get("first"), 100, 10);
    move("down");
    expect(document.activeElement?.id).toBe("first");
    expect(document.documentElement.dataset.input).toBe("pad");
  });

  it("lets a component take its arrow key first", () => {
    const get = page(
      `<div id="list"><button id="tab">tab</button></div><button id="next">next</button>`,
    );
    at(get("tab"), 0, 0);
    at(get("next"), 150, 0);
    get("list").addEventListener("keydown", (e) => {
      if (e.key === "ArrowRight") e.preventDefault();
    });
    get("tab").focus();
    move("right");
    expect(document.activeElement?.id).toBe("tab");
    // A key it doesn't take moves the focus.
    at(get("next"), 0, 50);
    move("down");
    expect(document.activeElement?.id).toBe("next");
  });

  it("sends a text field no arrow keys", () => {
    const get = page(`<input id="field" /><button id="next">next</button>`);
    at(get("field"), 0, 0);
    at(get("next"), 0, 50);
    const keys = vi.fn();
    get("field").addEventListener("keydown", keys);
    get("field").focus();
    move("down");
    expect(keys).not.toHaveBeenCalled();
    expect(document.activeElement?.id).toBe("next");
  });
});

describe("activate", () => {
  it("clicks links, buttons, checkboxes and menu items", () => {
    const get = page(`
      <button id="button">b</button><input id="box" type="checkbox" />
      <button id="item" role="menuitem" tabindex="-1">item</button>`);
    const clicked = vi.fn();
    get("button").addEventListener("click", clicked);
    get("button").focus();
    activate();
    expect(clicked).toHaveBeenCalledOnce();

    get("box").focus();
    activate();
    expect((get("box") as HTMLInputElement).checked).toBe(true);

    const item = vi.fn();
    get("item").addEventListener("click", item);
    get("item").focus();
    activate();
    expect(item).toHaveBeenCalledOnce();
  });

  it("presses Enter with nothing in particular focused", () => {
    page(`<div role="dialog" id="panel" tabindex="-1"></div>`);
    const keys: string[] = [];
    window.addEventListener("keydown", (e) => keys.push(e.key), { once: true });
    (document.getElementById("panel") as HTMLElement).focus();
    activate();
    expect(keys).toEqual(["Enter"]);
  });

  it("steps a select to its next option, wrapping and skipping disabled ones", () => {
    const get = page(`
      <select id="s"><option>a</option><option disabled>b</option><option>c</option></select>`);
    const select = get("s") as HTMLSelectElement;
    const changes = vi.fn();
    select.addEventListener("change", changes);
    select.focus();
    activate();
    expect(select.value).toBe("c");
    stepSelect(select);
    expect(select.value).toBe("a");
    expect(changes).toHaveBeenCalledTimes(2);
  });
});

describe("back", () => {
  it("sends Escape while a dialog is open", () => {
    page(`<div role="dialog"><button id="in">in</button></div>`);
    const keys: string[] = [];
    document.addEventListener("keydown", (e) => keys.push(e.key), { once: true });
    const goBack = vi.fn();
    back(goBack);
    expect(keys).toEqual(["Escape"]);
    expect(goBack).not.toHaveBeenCalled();
  });

  it("goes back with nothing open", () => {
    page(`<button>b</button>`);
    const goBack = vi.fn();
    back(goBack);
    expect(goBack).toHaveBeenCalledOnce();
  });
});

/** A scroll box at the top of the window, `height` tall, holding `content`. */
function scrollBox(box: HTMLElement, height: number, content: number, top = 0) {
  Object.defineProperties(box, {
    clientHeight: { value: height, configurable: true },
    scrollHeight: { value: content, configurable: true },
    scrollTop: { value: top, configurable: true, writable: true },
  });
  box.style.overflowY = "auto";
  at(box, 0, 0, 800, height);
  box.scrollTo = vi.fn() as typeof box.scrollTo;
  return box;
}

describe("scrollPage", () => {
  it("scrolls the page by most of its height, not past its end", () => {
    const get = page('<main id="m"><button id="a">A</button></main>');
    const main = scrollBox(get("m"), 500, 1200);
    at(get("a"), 0, 10);
    scrollPage(1);
    expect(main.scrollTo).toHaveBeenCalledWith({ top: 500 * PAGE_SHARE, behavior: "smooth" });
    main.scrollTop = 600;
    scrollPage(1);
    expect(main.scrollTo).toHaveBeenLastCalledWith({ top: 700, behavior: "smooth" });
    vi.mocked(main.scrollTo).mockClear();
    main.scrollTop = 700;
    scrollPage(1);
    expect(main.scrollTo).not.toHaveBeenCalled();
  });

  it("moves the focus to the first element in view when it scrolls out", () => {
    const get = page(
      '<main id="m"><button id="a">A</button><button id="b">B</button><button id="c">C</button><button id="d">D</button></main>',
    );
    scrollBox(get("m"), 500, 2000);
    at(get("a"), 0, 10);
    at(get("b"), 0, 440);
    at(get("c"), 200, 520);
    at(get("d"), 0, 600);
    get("a").focus();
    scrollPage(1);
    // 450 down: B (then at -10) is cut off at the top, C (at 70) comes first.
    expect(get("c")).toHaveFocus();
  });

  it("going up, takes the bottom one; a focus still in view stays", () => {
    const get = page(
      '<main id="m"><button id="a">A</button><button id="b">B</button><button id="c">C</button></main>',
    );
    scrollBox(get("m"), 500, 2000, 900);
    at(get("a"), 0, -400);
    at(get("b"), 0, -100);
    at(get("c"), 0, 400);
    get("c").focus();
    scrollPage(-1);
    expect(get("b")).toHaveFocus();

    at(get("b"), 0, 10);
    scrollPage(1);
    expect(get("b")).toHaveFocus();
  });
});

describe("toggleNav", () => {
  const html = `
    <nav><a id="lib" href="/">Library</a><a id="set" href="/settings" aria-current="page">Settings</a></nav>
    <main><button id="x">X</button><button id="y">Y</button></main>`;

  it("goes to the current page's item and back where it came from", () => {
    const get = page(html);
    for (const [id, y] of [
      ["lib", 0],
      ["set", 50],
      ["x", 0],
      ["y", 50],
    ] as const)
      at(get(id), 0, y);
    get("y").focus();
    toggleNav();
    expect(get("set")).toHaveFocus();
    toggleNav();
    expect(get("y")).toHaveFocus();
  });

  it("goes back to the page's first element when where it was is gone", () => {
    const get = page(html);
    for (const [id, y] of [
      ["lib", 0],
      ["set", 50],
      ["x", 0],
      ["y", 50],
    ] as const)
      at(get(id), 0, y);
    get("y").focus();
    toggleNav();
    get("y").remove();
    toggleNav();
    expect(get("x")).toHaveFocus();
  });

  it("does nothing with a dialog open or without a sidebar", () => {
    const get = page(html + '<div role="dialog"><button id="ok">OK</button></div>');
    at(get("ok"), 0, 0);
    get("ok").focus();
    toggleNav();
    expect(get("ok")).toHaveFocus();

    const none = page('<main><button id="next">Next</button></main>');
    at(none("next"), 0, 0);
    none("next").focus();
    toggleNav();
    expect(none("next")).toHaveFocus();
  });
});
