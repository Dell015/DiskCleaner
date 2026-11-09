// Loads a view file (or makes the demo) off the main thread, so the page stays responsive.
// Nothing leaves this computer: the file is read from disk by the browser and parsed here.

import { makeDemo } from "./core/demo";
import { checkView, parseView, ViewError, type ViewData } from "./core/format";

export type LoaderRequest = { type: "file"; file: File } | { type: "demo"; size: number };
export type LoaderReply =
  | { type: "progress"; step: string }
  | { type: "done"; data: ViewData; ms: number }
  | { type: "error"; message: string };

const post = (m: LoaderReply, transfer: Transferable[] = []): void => (self as unknown as Worker).postMessage(m, transfer);

self.onmessage = async (e: MessageEvent<LoaderRequest>): Promise<void> => {
  const start = performance.now();
  try {
    let data: ViewData;
    if (e.data.type === "file") {
      const file = e.data.file;
      // V8 cannot hold one string much larger than this.
      if (file.size > 480_000_000) throw new ViewError("This file is too large to open (more than 480 MB). Make a view file from fewer scans.");
      post({ type: "progress", step: `Reading ${file.name}…` });
      const text = await file.text();
      post({ type: "progress", step: "Checking the file…" });
      data = parseView(text);
    } else {
      post({ type: "progress", step: "Making a demo disk…" });
      data = checkView(makeDemo(e.data.size));
    }
    post({ type: "progress", step: "Drawing…" });
    post({ type: "done", data, ms: performance.now() - start }, [
      data.parent.buffer, data.kind.buffer, data.size.buffer, data.mtime.buffer, data.flags.buffer, data.cat.buffer, data.assignments.buffer,
    ]);
  } catch (err) {
    post({ type: "error", message: err instanceof ViewError ? err.message : `The file could not be loaded: ${String(err)}` });
  }
};
