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
    beforeAll(async () => {
      const wasm = await import(/* @vite-ignore */ moduleUrl.href);
      await wasm.default({
        module_or_path: await readFile(
          new URL("../public/wasm/ribbon_core_bg.wasm", import.meta.url),
        ),
      });
      renderJson = wasm.render_json;
    });
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
