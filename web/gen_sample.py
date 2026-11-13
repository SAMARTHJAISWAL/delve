"""Bundle delve's own source code as the offline sample for the web page."""

import json
import pathlib
import subprocess
import sys

here = pathlib.Path(__file__).resolve().parent.parent
# usage: gen_sample.py OUT [REPO_DIR NAME]  (default: delve's own source)
root = pathlib.Path(sys.argv[2]).resolve() if len(sys.argv) > 3 else here
name = sys.argv[3] if len(sys.argv) > 3 else "SAMARTHJAISWAL/delve"
EXTS = {".rs", ".js", ".py", ".gleam", ".ts", ".go", ".ml", ".zig"}

files = []
tracked = subprocess.run(["git", "-C", str(root), "ls-files"], capture_output=True, text=True).stdout.split()
candidates = [root / t for t in tracked] if tracked else sorted(root.glob("crates/**/*")) + sorted(root.glob("web/*"))
for path in candidates:
    if path.is_file() and path.suffix in EXTS and "target" not in path.parts:
        files.append([str(path.relative_to(root)), path.read_text(encoding="utf-8")])

commits = []
try:
    log = subprocess.run(
        ["git", "-C", str(root), "log", "-n", "100", "--no-merges", "--format=%an%x1f%s"],
        capture_output=True, text=True, check=True,
    ).stdout
    commits = [line.split("\x1f", 1) for line in log.splitlines() if "\x1f" in line]
except (subprocess.CalledProcessError, FileNotFoundError):
    pass
if not commits:
    commits = [["Samarth Jaiswal", "delve: play any codebase as a roguelike"]]

sample = {"name": name, "files": files, "commits": commits, "total": len(files)}
target = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else here / "web/sample.js"
target.write_text("export const sample = " + json.dumps(sample) + ";\n", encoding="utf-8")
print(f"wrote {target} ({len(files)} files)")
