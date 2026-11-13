// delve in the browser: fetch a public GitHub repository, hand its files to
// the Rust engine (WebAssembly) and draw the screen grid on a canvas.
import { sample } from "./sample.js";

const FEATURED = [
  "facebook/react",
  "pallets/flask",
  "BurntSushi/ripgrep",
  "gin-gonic/gin",
  "expressjs/express",
  "SAMARTHJAISWAL/delve",
];

// same file types and skipped folders as the Rust side
const EXTS = new Set("rs py pyi js mjs cjs jsx ts tsx mts cts go java kt kts scala c h cc cpp cxx hpp hh hxx cs rb php swift sh bash zsh lua ex exs hs ml mli zig gleam dart".split(" "));
const SKIP = new Set("node_modules vendor target dist build out __pycache__ venv third_party third-party external deps Pods bower_components site-packages".split(" "));
const MAX_FILES = 160;
const MAX_BYTES = 6_000_000;
const MAX_FILE = 300_000;

const PALETTE = [
  null, "#6e6e78", "#5f5a6e", "#464650", "#ffffff", "#eb5f55", "#78d278", "#dcb45a",
  "#6e96f5", "#c88cff", "#6ec8dc", "#e1e1e6", "#ff466e", "#1e1e28", "#f5c850",
];
const BG = "#0f0e13";

const $ = (id) => document.getElementById(id);
const canvas = $("screen");
const ctx = canvas.getContext("2d");
let wasm = null;
// ?bot=1 starts the bot as soon as the dungeon loads (a link to watch)
let autoBot = new URLSearchParams(location.search).get("bot") === "1";
let current = null; // { name, url }

// ---------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------

async function loadEngine() {
  const res = await fetch("delve.wasm");
  const { instance } = await WebAssembly.instantiate(await res.arrayBuffer(), {});
  wasm = instance.exports;
}

function fnv(text) {
  let h = 0xcbf29ce484222325n;
  for (const b of new TextEncoder().encode(text)) {
    h ^= BigInt(b);
    h = (h * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return h;
}

function start(name, files, commits, seed) {
  const clean = (s) => s.replace(/[\0\x1f]/g, " ");
  const parts = [clean(name)];
  for (const [path, text] of files) parts.push(`F\x1f${clean(path)}\x1f${clean(text)}`);
  for (const [author, subject] of commits) parts.push(`C\x1f${clean(author)}\x1f${clean(subject)}`);
  const bytes = new TextEncoder().encode(parts.join("\0"));
  const ptr = wasm.input_buffer(bytes.length);
  new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
  const s = seed ?? fnv(name);
  const n = wasm.start(Number(s >> 32n), Number(s & 0xffffffffn));
  return n;
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

let cellW = 10;
let cellH = 20;
let fontPx = 16;

function layout() {
  const W = wasm.width();
  const H = wasm.height();
  const dpr = window.devicePixelRatio || 1;
  fontPx = 16;
  ctx.font = `${fontPx}px ui-monospace, "SF Mono", Menlo, Consolas, monospace`;
  cellW = Math.ceil(ctx.measureText("M").width);
  cellH = Math.ceil(fontPx * 1.32);
  canvas.width = W * cellW * dpr;
  canvas.height = H * cellH * dpr;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
}

function paint() {
  const W = wasm.width();
  const H = wasm.height();
  const chars = new Uint32Array(wasm.memory.buffer, wasm.chars(), W * H);
  const colors = new Uint8Array(wasm.memory.buffer, wasm.colors(), W * H * 2);
  ctx.fillStyle = BG;
  ctx.fillRect(0, 0, W * cellW, H * cellH);
  ctx.font = `${fontPx}px ui-monospace, "SF Mono", Menlo, Consolas, monospace`;
  ctx.textBaseline = "middle";
  ctx.textAlign = "center";
  for (let i = 0; i < W * H; i++) {
    const bg = PALETTE[colors[i * 2 + 1]];
    if (bg) {
      ctx.fillStyle = bg;
      ctx.fillRect((i % W) * cellW, Math.floor(i / W) * cellH, cellW + 0.5, cellH + 0.5);
    }
  }
  for (let i = 0; i < W * H; i++) {
    const code = chars[i];
    if (code === 32) continue;
    const x = (i % W) * cellW;
    const y = Math.floor(i / W) * cellH;
    const ch = String.fromCodePoint(code);
    ctx.fillStyle = PALETTE[colors[i * 2]] || "#e1e1e6";
    if (ch === "█" || ch === "░") {
      // blocks drawn as rectangles so they join up
      ctx.globalAlpha = ch === "█" ? 1 : 0.35;
      ctx.fillRect(x, y + 2, cellW, cellH - 4);
      ctx.globalAlpha = 1;
    } else if (ch === "─") {
      ctx.fillRect(x, y + cellH / 2, cellW, 1);
    } else {
      ctx.fillText(ch, x + cellW / 2, y + cellH / 2 + 1);
    }
  }
  follow(chars, W);
  const state = wasm.state();
  $("share").hidden = state === 0;
  if (state !== 0) showShare();
}

/// On a narrow screen the game scrolls sideways: keep the player in view.
function follow(chars, W) {
  const box = canvas.parentElement;
  if (box.scrollWidth <= box.clientWidth) return;
  const at = chars.indexOf(64); // '@'
  if (at < 0) return;
  const scale = canvas.clientWidth / (W * cellW);
  const x = (at % W) * cellW * scale;
  box.scrollLeft = Math.max(0, x - box.clientWidth / 2);
}

function showShare() {
  const ptr = wasm.summary();
  const text = new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, ptr, wasm.text_len()));
  const link = `${location.origin}${location.pathname}?repo=${encodeURIComponent(current.name)}`;
  const full = `${text} ${link}`;
  $("summary").textContent = text;
  $("copy").onclick = async () => {
    try {
      await navigator.clipboard.writeText(full);
      $("copy").textContent = "Copied";
    } catch {
      $("copy").textContent = "Copy failed";
    }
  };
  $("post").href = `https://x.com/intent/post?text=${encodeURIComponent(`${text} #delve\n${link}`)}`;
}

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

const KEYS = {
  ArrowUp: 1, k: 1, w: 1, ArrowDown: 2, j: 2, s: 2, ArrowLeft: 3, h: 3, a: 3, ArrowRight: 4, l: 4, d: 4,
  y: 5, u: 6, b: 7, n: 8, ".": 9, " ": 9, ">": 10, "<": 11, "?": 30, m: 31, r: 32,
};

let botTimer = null;

function stopBot() {
  clearInterval(botTimer);
  botTimer = null;
  $("watch").textContent = "Watch a bot play";
  $("watch").classList.remove("on");
}

function toggleBot() {
  if (botTimer) {
    stopBot();
    return;
  }
  $("watch").textContent = "Stop the bot";
  $("watch").classList.add("on");
  botTimer = setInterval(() => {
    wasm.bot_step();
    paint();
    if (wasm.state() !== 0) stopBot();
  }, 90);
}

$("watch").addEventListener("click", toggleBot);

function press(code) {
  if (!wasm || $("stage").hidden) return;
  if (botTimer) stopBot();
  $("copy").textContent = "Copy";
  wasm.key(code);
  paint();
}

canvas.addEventListener("keydown", (e) => {
  if (e.metaKey || e.ctrlKey || e.altKey) return;
  let code = KEYS[e.key];
  if (code === undefined && /^[1-9]$/.test(e.key)) code = 20 + Number(e.key);
  if (code === undefined) code = 0;
  e.preventDefault();
  press(code);
});

for (const b of $("pad").querySelectorAll("button")) {
  b.addEventListener("click", () => {
    press(Number(b.dataset.k));
    canvas.focus({ preventScroll: true });
  });
}

$("again").addEventListener("click", () => {
  press(32);
  canvas.focus({ preventScroll: true });
});

// ---------------------------------------------------------------------------
// GitHub
// ---------------------------------------------------------------------------

function status(text, error = false) {
  $("status").textContent = text;
  $("status").classList.toggle("error", error);
}

async function getJSON(url) {
  const res = await fetch(url, { headers: { Accept: "application/vnd.github+json" } });
  if (res.status === 404) throw new Error("not found");
  if (res.status === 403 || res.status === 429) {
    throw new Error("GitHub allows 60 requests an hour from one network without signing in, and that is used up. Try again later, or play delve's own code.");
  }
  if (!res.ok) throw new Error(`GitHub answered ${res.status}`);
  return res.json();
}

function wanted(path, size) {
  const parts = path.split("/");
  if (parts.some((p) => p.startsWith(".") || SKIP.has(p))) return false;
  if (/\.min\.js$|\.d\.ts$|\.generated\.|_pb2\.py$|\.pb\.go$/.test(path)) return false;
  const ext = path.includes(".") ? path.split(".").pop().toLowerCase() : "";
  return EXTS.has(ext) && size > 0 && size <= MAX_FILE;
}

/// Spread the budget across folders: the biggest file of each folder first.
function choose(blobs) {
  const groups = new Map();
  for (const b of blobs) {
    const dir = b.path.includes("/") ? b.path.slice(0, b.path.lastIndexOf("/")) : "";
    if (!groups.has(dir)) groups.set(dir, []);
    groups.get(dir).push(b);
  }
  const lists = [...groups.values()].map((l) => l.sort((a, b) => b.size - a.size));
  lists.sort((a, b) => b.reduce((s, x) => s + x.size, 0) - a.reduce((s, x) => s + x.size, 0));
  const out = [];
  let bytes = 0;
  for (let round = 0; out.length < MAX_FILES; round++) {
    let added = false;
    for (const l of lists) {
      const b = l[round];
      if (!b) continue;
      if (out.length >= MAX_FILES || bytes + b.size > MAX_BYTES) continue;
      out.push(b);
      bytes += b.size;
      added = true;
    }
    if (!added) break;
  }
  return out;
}

async function fetchRepo(full) {
  const api = `https://api.github.com/repos/${full}`;
  status(`Opening ${full}…`);
  let meta;
  try {
    meta = await getJSON(api);
  } catch (e) {
    throw new Error(e.message === "not found" ? `There is no public repository called ${full}.` : e.message);
  }
  const branch = meta.default_branch;
  const tree = await getJSON(`${api}/git/trees/${encodeURIComponent(branch)}?recursive=1`);
  const commits = await getJSON(`${api}/commits?per_page=100&sha=${encodeURIComponent(branch)}`).catch(() => []);
  const blobs = tree.tree.filter((t) => t.type === "blob" && wanted(t.path, t.size));
  if (!blobs.length) throw new Error(`${meta.full_name} has no source code delve can read.`);
  const picked = choose(blobs);
  const files = [];
  let done = 0;
  const queue = [...picked];
  async function worker() {
    while (queue.length) {
      const b = queue.shift();
      try {
        const res = await fetch(`https://raw.githubusercontent.com/${meta.full_name}/${encodeURIComponent(branch)}/${b.path.split("/").map(encodeURIComponent).join("/")}`);
        if (res.ok) files.push([b.path, await res.text()]);
      } catch {
        // a file that fails to load just isn't in the dungeon
      }
      done++;
      status(`Reading ${meta.full_name}: ${done}/${picked.length} files${blobs.length > picked.length ? ` (of ${blobs.length})` : ""}…`);
    }
  }
  await Promise.all(Array.from({ length: 12 }, worker));
  const people = commits.map((c) => [c.author?.login || c.commit?.author?.name || "", (c.commit?.message || "").split("\n")[0]]);
  return { name: meta.full_name, files, commits: people, total: blobs.length };
}

// ---------------------------------------------------------------------------

async function play(full) {
  full = full.trim().replace(/^https?:\/\/github\.com\//, "").replace(/\.git$/, "").replace(/\/+$/, "");
  if (!/^[\w.-]+\/[\w.-]+$/.test(full)) {
    status("Enter a repository as owner/name, for example facebook/react.", true);
    return;
  }
  $("pick").querySelector("button").disabled = true;
  try {
    if (!wasm) await loadEngine();
    let repo;
    if (full.toLowerCase() === sample.name.toLowerCase()) {
      repo = sample;
    } else {
      repo = await fetchRepo(full);
    }
    const seedParam = new URLSearchParams(location.search).get("seed");
    const seed = seedParam && /^\d+$/.test(seedParam) ? BigInt(seedParam) : undefined;
    const n = start(repo.name, repo.files, repo.commits, seed);
    if (!n) throw new Error(`delve could not find any functions to fight in ${repo.name}.`);
    stopBot();
    current = { name: repo.name };
    $("repo").value = repo.name;
    history.replaceState(null, "", `?repo=${encodeURIComponent(repo.name)}`);
    status(`${repo.name}: ${n} files${repo.total && repo.total > n ? ` read of ${repo.total}` : ""}. Click the game and use the arrow keys. Press ? for help.`);
    $("stage").hidden = false;
    layout();
    paint();
    canvas.focus({ preventScroll: true });
    $("stage").scrollIntoView({ behavior: "smooth", block: "start" });
    if (autoBot) {
      autoBot = false;
      toggleBot();
    }
  } catch (e) {
    status(e.message, true);
  } finally {
    $("pick").querySelector("button").disabled = false;
  }
}

$("pick").addEventListener("submit", (e) => {
  e.preventDefault();
  play($("repo").value);
});

for (const name of FEATURED) {
  const b = document.createElement("button");
  b.type = "button";
  b.textContent = name;
  b.onclick = () => {
    $("repo").value = name;
    play(name);
  };
  $("chips").appendChild(b);
}

window.addEventListener("resize", () => {
  if (wasm && !$("stage").hidden) {
    layout();
    paint();
  }
});

const asked = new URLSearchParams(location.search).get("repo");
if (asked) play(asked);
