import type { RenderRequest, RenderResult, StructureData } from "../types/render";

export type WorkerRequest =
  | { id: number; type: "parse"; text: string; name: string }
  | { id: number; type: "render"; request: RenderRequest; wasmUrl: string };
export type WorkerResponse =
  | { id: number; type: "parsed"; structure: StructureData }
  | { id: number; type: "rendered"; result: RenderResult }
  | { id: number; type: "error"; message: string };
