import type { RenderOptions, RenderResult, StructureData } from "../types/render";

export type WorkerRequest =
  | { id: number; revision: number; type: "parse"; text: string; name: string }
  | {
      id: number;
      revision: number;
      type: "render";
      options: RenderOptions;
      quality: "preview" | "final";
      includePreviewScene?: boolean;
      wasmUrl: string;
      /** Only needed to rehydrate a fresh worker. Normal requests are options-only. */
      structure?: StructureData;
    };
export type WorkerResponse =
  | { id: number; type: "parsed"; structure: StructureData }
  | { id: number; type: "rendered"; result: RenderResult }
  | { id: number; type: "error"; message: string; fatal?: boolean };
