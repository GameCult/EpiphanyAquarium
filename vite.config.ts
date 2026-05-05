import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { readdir, readFile, stat } from "node:fs/promises";
import { extname, join, relative } from "node:path";

const defaultMidiCorpus =
  "D:\\Documents\\130000_Pop_Rock_Classical_Videogame_EDM_MIDI_Archive[6_19_15]\\Classical_www.midiworld.com_MIDIRip";
const midiFileLimit = 1000;

export default defineConfig({
  plugins: [react(), midiCorpusDevPlugin()],
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
});

function midiCorpusDevPlugin() {
  return {
    name: "epiphany-midi-corpus-dev",
    configureServer(server: any) {
      server.middlewares.use("/midi-corpus/list", async (request: any, response: any) => {
        try {
          const url = new URL(request.url ?? "", "http://127.0.0.1");
          const root = url.searchParams.get("root") || defaultMidiCorpus;
          const files = await listMidiFiles(root);
          writeJson(response, { root, files });
        } catch (error) {
          writeJson(response, { error: error instanceof Error ? error.message : String(error) }, 500);
        }
      });
      server.middlewares.use("/midi-corpus/file", async (request: any, response: any) => {
        try {
          const url = new URL(request.url ?? "", "http://127.0.0.1");
          const path = url.searchParams.get("path");
          if (!path) {
            writeJson(response, { error: "missing MIDI path" }, 400);
            return;
          }
          const data = await readFile(path);
          response.statusCode = 200;
          response.setHeader("content-type", "audio/midi");
          response.end(data);
        } catch (error) {
          writeJson(response, { error: error instanceof Error ? error.message : String(error) }, 500);
        }
      });
    },
  };
}

async function listMidiFiles(root: string) {
  const files: Array<{ name: string; path: string; relativePath: string; size: number }> = [];
  async function visit(directory: string) {
    if (files.length >= midiFileLimit) return;
    const entries = await readdir(directory, { withFileTypes: true });
    for (const entry of entries) {
      if (files.length >= midiFileLimit) break;
      const path = join(directory, entry.name);
      if (entry.isDirectory()) {
        await visit(path);
        continue;
      }
      const extension = extname(entry.name).toLowerCase();
      if (extension !== ".mid" && extension !== ".midi") continue;
      const metadata = await stat(path);
      files.push({
        name: entry.name,
        path,
        relativePath: relative(root, path),
        size: metadata.size,
      });
    }
  }
  await visit(root);
  return files;
}

function writeJson(response: any, payload: unknown, status = 200) {
  response.statusCode = status;
  response.setHeader("content-type", "application/json");
  response.end(JSON.stringify(payload));
}
