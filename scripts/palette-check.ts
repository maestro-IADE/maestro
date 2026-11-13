// The custom palette maths (src/palette.ts). Run: `npm test`.
import { deepStrictEqual, equal, ok } from "node:assert/strict";
import { BUILTIN_SEEDS, contrast, decodePalette, derivePalette, encodePalette, freeName, isLightSeed, luminance, PALETTE_KEYS } from "../src/palette.ts";

const lum = (hex: string) => luminance(hex);

// contrast basics
equal(Math.round(contrast("#000000", "#ffffff")), 21, "black on white");
equal(contrast("#777777", "#777777"), 1, "same colour");

for (const [id, seed] of Object.entries(BUILTIN_SEEDS)) {
  for (const c of [0, seed.contrast, 100]) {
    const p = derivePalette({ ...seed, contrast: c });
    const v = p.vars;
    equal(p.light, isLightSeed(seed), `${id}: light flag from the base`);
    deepStrictEqual(Object.keys(v).sort(), [...PALETTE_KEYS].sort(), `${id}: every token`);
    for (const k of PALETTE_KEYS) ok(/^#[0-9a-f]{6}$/.test(v[k]), `${id} ${k} is a hex colour: ${v[k]}`);
    // the layers keep their order: dark grows lighter above the canvas, light grows darker below it
    const order = p.light
      ? ["--mae-node", "--mae-canvas", "--mae-panel", "--mae-panel-2", "--mae-hover", "--mae-selected"]
      : ["--mae-bg", "--mae-canvas", "--mae-panel", "--mae-node", "--mae-panel-2", "--mae-hover", "--mae-selected"];
    const ls = order.map((k) => lum(v[k as keyof typeof v]));
    for (let i = 1; i < ls.length; i++) {
      if (p.light) ok(ls[i] <= ls[i - 1], `${id}@${c}: ${order[i]} not lighter than ${order[i - 1]}`);
      else ok(ls[i] >= ls[i - 1], `${id}@${c}: ${order[i]} not darker than ${order[i - 1]}`);
    }
    // text reads: muted at 4.5:1 on the panels and inputs, text far above that, the accent's text at its best
    for (const surf of ["--mae-panel", "--mae-panel-2", "--mae-canvas", "--mae-node"] as const) {
      ok(contrast(v["--mae-muted"], v[surf]) >= 4.5, `${id}@${c}: muted on ${surf} ${contrast(v["--mae-muted"], v[surf]).toFixed(2)}`);
      ok(contrast(v["--mae-text"], v[surf]) >= 7, `${id}@${c}: text on ${surf}`);
    }
    ok(contrast(v["--mae-text-2"], v["--mae-panel"]) >= contrast(v["--mae-muted"], v["--mae-panel"]), `${id}@${c}: text-2 between text and muted`);
    ok(contrast(v["--mae-on-accent"], v["--mae-accent"]) >= 4.5, `${id}: text on the accent`);
  }
  // more contrast spreads the layers further apart
  const lo = derivePalette({ ...seed, contrast: 0 }).vars, hi = derivePalette({ ...seed, contrast: 100 }).vars;
  ok(Math.abs(lum(hi["--mae-selected"]) - lum(hi["--mae-canvas"])) > Math.abs(lum(lo["--mae-selected"]) - lum(lo["--mae-canvas"])), `${id}: contrast widens`);
}

// a bad colour falls back instead of breaking
equal(derivePalette({ base: "nope", accent: "#3d6fe0", contrast: 50 }).light, false, "bad base: the dark default");

// sharing: a round trip, Korean names included; anything else is refused
const t = { name: "보더콜리", base: "#111112", accent: "#e0955a", contrast: 70 };
deepStrictEqual(decodePalette(encodePalette(t)), t, "round trip");
deepStrictEqual(decodePalette("  " + encodePalette(t) + "\n"), t, "spaces around a paste");
equal(decodePalette("hello"), null, "not a palette");
equal(decodePalette("maestro-palette:!!!"), null, "broken text");
equal(decodePalette(encodePalette({ ...t, base: "red" })), null, "bad colour");
equal(decodePalette(encodePalette({ ...t, contrast: 250 }))!.contrast, 100, "contrast clamped");
equal(decodePalette(encodePalette({ ...t, name: "  " }))!.name, "가져온 팔레트", "an empty name gets one");

// names
equal(freeName([]), "내 팔레트");
equal(freeName(["내 팔레트", "내 팔레트 2"]), "내 팔레트 3");

console.log("ok palette");
