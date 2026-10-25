import { readFile, access } from "node:fs/promises";
import { beforeAll, describe, expect, it } from "vitest";
import { parseStructure } from "../app/lib/parser";
import { validateSvg } from "../app/lib/exports";
import { defaultRenderOptions } from "../app/types/render";
import type { RenderResult } from "../app/types/render";

const moduleUrl = new URL("../public/wasm/ribbon_core.js", import.meta.url);
const available = await access(moduleUrl).then(
  () => true,
  () => false,
);
describe.skipIf(!available)(
  "real parser → web-target WASM integration (run pnpm build:wasm first)",
  () => {
    let renderJson: (request: string) => string;
    let Session: new (structure: string) => {
      render_options_json(options: string, preview: boolean): string;
      preview_scene_json(): string;
      free(): void;
    };
    beforeAll(async () => {
      const wasm = await import(/* @vite-ignore */ moduleUrl.href);
      await wasm.default({
        module_or_path: await readFile(
          new URL("../public/wasm/ribbon_core_bg.wasm", import.meta.url),
        ),
      });
      renderJson = wasm.render_json;
      Session = wasm.RenderSession;
    });
    it.each(["1ezg-backbone.pdb", "1mbn-backbone.pdb"])(
      "retains scientific %s geometry with exact stateful/stateless cache parity",
      async (name) => {
        const text = await readFile(new URL(`./fixtures/dssp/${name}`, import.meta.url), "utf8");
        const structure = await parseStructure(text, name);
        const options = defaultRenderOptions();
        options.samples = 8;
        options.selectedChains = structure.chains.map((c) => c.id);
        const session = new Session(JSON.stringify(structure));
        try {
          for (const rotation of [
            [0, 0, 0],
            [31, -45, 12],
          ] as [number, number, number][]) {
            options.rotation = rotation;
            const preview = JSON.parse(session.render_options_json(JSON.stringify(options), true));
            expect(preview.timings.clipMs).toBe(0);
            expect(preview.result.svg).toContain("Hidden strokes are NOT removed");
            const scene = JSON.parse(session.preview_scene_json());
            expect(scene.ribbons.length).toBe(structure.segments.length);
            expect(
              scene.ribbons.reduce(
                (n: number, r: { samples: unknown[] }) => n + r.samples.length,
                0,
              ),
            ).toBeLessThanOrEqual(800 + 2 * structure.segments.length);
            expect(JSON.stringify(scene)).not.toMatch(/null|NaN|Infinity/);
            for (const color of ["black", "red", "#12aBC3"]) {
              options.chainColors = { [structure.chains[0]!.id]: color };
              const actual = JSON.parse(
                session.render_options_json(JSON.stringify(options), false),
              );
              expect(actual.result).toEqual(
                JSON.parse(renderJson(JSON.stringify({ structure, options }))),
              );
              expect(actual.timings.pathCacheHit).toBe(color !== "black");
            }
          }
          if (name.startsWith("1ezg")) {
            expect(
              structure.segments.flatMap((s) => s.residues).filter((r) => r.kind === "E"),
            ).toHaveLength(38);
          }
        } finally {
          session.free();
        }
      },
      30_000,
    );
    it.each(["1crn-backbone.pdb", "1crn-backbone.cif"])(
      "renders %s with computed H/E, line-only physical SVG and option changes",
      async (name) => {
        const text = await readFile(new URL(`../public/examples/${name}`, import.meta.url), "utf8");
        const structure = await parseStructure(text, name);
        const options = defaultRenderOptions();
        options.selectedChains = structure.chains.map((c) => c.id);
        const first = JSON.parse(
          renderJson(JSON.stringify({ structure, options })),
        ) as RenderResult;
        validateSvg(first.svg);
        expect(first.strokeCount).toBeGreaterThan(0);
        expect(first.lengthMm).toBeGreaterThan(0);
        expect(first.residueCount).toBe(46);
        expect(first.svg).toContain('width="210mm" height="297mm"');
        options.rotation = [25, 40, 10];
        options.paper = "a3";
        options.landscape = true;
        options.widthMm = 420;
        options.heightMm = 297;
        const next = JSON.parse(renderJson(JSON.stringify({ structure, options }))) as RenderResult;
        expect(next.svg).not.toBe(first.svg);
        expect(next.residueCount).toBe(46);
        expect(next.svg).toContain('width="420mm" height="297mm"');
        expect(next.warnings.join(" ")).toMatch(/hydrogen-bond DSSP/);
      },
    );
  },
);
