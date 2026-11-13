// Load the built WebAssembly module in Node, start a game on the bundled
// sample and let the bot play it to the end. Used by CI.
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import path from "node:path";

const site = process.argv[2] || "_site";
const { sample } = await import(pathToFileURL(path.resolve(site, "sample.js")).href);
const { instance } = await WebAssembly.instantiate(await readFile(path.join(site, "delve.wasm")), {});
const w = instance.exports;

const parts = [sample.name, ...sample.files.map(([p, t]) => `F\x1f${p}\x1f${t}`), ...sample.commits.map(([a, s]) => `C\x1f${a}\x1f${s}`)];
const bytes = new TextEncoder().encode(parts.join("\0"));
const ptr = w.input_buffer(bytes.length);
new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes);
const files = w.start(0, 1);
if (!files) throw new Error("no files understood");

const screen = () => String.fromCodePoint(...new Uint32Array(w.memory.buffer, w.chars(), w.width() * w.height()));
if (!screen().includes("QUEST")) throw new Error("first screen has no sidebar");

let steps = 0;
while (w.state() === 0 && steps < 20000) {
  w.bot_step();
  steps++;
}
if (w.state() === 0) throw new Error("the game did not end");
const summary = new TextDecoder().decode(new Uint8Array(w.memory.buffer, w.summary(), w.text_len()));
console.log(`${files} files, ${steps} bot moves: ${summary}`);
