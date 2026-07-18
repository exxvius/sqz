// Cursor-driven visual effects, all CSS-var driven so the styling lives in CSS.
//
//  - Card border glow: every card gets --mx/--my (pointer position relative to
//    the card, which may fall outside it) plus --glow (0..1 proximity intensity
//    by distance to the card's nearest edge). Cards light the edge facing the
//    cursor even when it's outside them — no :hover needed.
//  - Background parallax: the root gets --par-x/--par-y in [-1, 1] from the
//    viewport centre, which the ambient gradient layer translates by.
//
// One passive pointermove listener, rAF-throttled. Reads all rects before
// writing any vars to avoid layout thrashing. Honors prefers-reduced-motion by
// skipping the background parallax.

const CARD_SELECTOR = ".card, .live-card, .ecard, .dropzone";
const GLOW_RANGE = 260; // px falloff: cards within this of the cursor glow

export function initCursorFx(): () => void {
  const root = document.documentElement;
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  let frame = 0;
  let clientX = 0;
  let clientY = 0;

  const apply = () => {
    frame = 0;

    const cards = Array.from(document.querySelectorAll<HTMLElement>(CARD_SELECTOR));
    // Read phase: all layout reads first.
    const rects = cards.map((c) => c.getBoundingClientRect());
    // Write phase: no reads interleaved.
    cards.forEach((card, i) => {
      const r = rects[i];
      if (r.width === 0 || r.height === 0) return;
      // Distance from the cursor to the card's nearest point (0 when inside).
      const nx = Math.max(r.left, Math.min(clientX, r.right));
      const ny = Math.max(r.top, Math.min(clientY, r.bottom));
      const dist = Math.hypot(clientX - nx, clientY - ny);
      const glow = Math.max(0, 1 - dist / GLOW_RANGE);
      card.style.setProperty("--mx", `${clientX - r.left}px`);
      card.style.setProperty("--my", `${clientY - r.top}px`);
      card.style.setProperty("--glow", glow.toFixed(3));
    });

    if (!reduce) {
      const px = (clientX / window.innerWidth - 0.5) * 2;
      const py = (clientY / window.innerHeight - 0.5) * 2;
      root.style.setProperty("--par-x", px.toFixed(4));
      root.style.setProperty("--par-y", py.toFixed(4));
    }
  };

  const onMove = (e: PointerEvent) => {
    clientX = e.clientX;
    clientY = e.clientY;
    if (!frame) frame = requestAnimationFrame(apply);
  };

  // When the pointer leaves the window, fade every card's glow out.
  const onLeave = () => {
    document.querySelectorAll<HTMLElement>(CARD_SELECTOR).forEach((c) => {
      c.style.setProperty("--glow", "0");
    });
  };

  window.addEventListener("pointermove", onMove, { passive: true });
  document.addEventListener("mouseleave", onLeave);

  return () => {
    window.removeEventListener("pointermove", onMove);
    document.removeEventListener("mouseleave", onLeave);
    if (frame) cancelAnimationFrame(frame);
  };
}
