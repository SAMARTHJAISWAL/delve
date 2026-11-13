//! A repository as delve sees it: source files with their functions and
//! notes, commits for the ghosts, and the folder tree that becomes floors.

use crate::analyze::{analyze, Function, Note};
use crate::lang::Lang;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: String,
    pub lang: Lang,
    pub lines: usize,
    pub functions: Vec<Function>,
    pub notes: Vec<Note>,
}

impl SourceFile {
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub author: String,
    pub subject: String,
    /// Paths the commit touched, when known.
    pub files: Vec<String>,
}

/// A folder that becomes a floor of the dungeon.
#[derive(Clone, Debug)]
pub struct Dir {
    /// "" for the repository root, otherwise "src/net" (chains of folders
    /// holding a single folder are merged: "src/main/java/com/acme").
    pub path: String,
    pub files: Vec<usize>,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
    pub depth: usize,
    /// Source lines in this folder and everything below it.
    pub total_lines: usize,
}

impl Dir {
    pub fn label(&self) -> &str {
        if self.path.is_empty() {
            "/"
        } else {
            &self.path
        }
    }
}

#[derive(Clone, Debug)]
pub struct Repo {
    pub name: String,
    pub files: Vec<SourceFile>,
    pub commits: Vec<Commit>,
    pub dirs: Vec<Dir>,
    /// (file index, function index) of the longest function: the final boss.
    pub boss: Option<(usize, usize)>,
}

/// Folders that never hold code worth exploring.
pub fn ignored(path: &str) -> bool {
    path.split('/').any(|part| {
        part.starts_with('.')
            || matches!(
                part,
                "node_modules"
                    | "vendor"
                    | "target"
                    | "dist"
                    | "build"
                    | "out"
                    | "__pycache__"
                    | "venv"
                    | ".venv"
                    | "third_party"
                    | "third-party"
                    | "external"
                    | "deps"
                    | "Pods"
                    | "bower_components"
                    | "site-packages"
            )
    }) || path.ends_with(".min.js")
        || path.ends_with(".d.ts")
        || path.contains(".generated.")
        || path.ends_with("_pb2.py")
        || path.ends_with(".pb.go")
}

impl Repo {
    /// Build a repository from (path, contents) pairs. Unknown file types and
    /// ignored folders are skipped.
    pub fn from_sources(name: &str, sources: Vec<(String, String)>, commits: Vec<Commit>) -> Repo {
        let mut files: Vec<SourceFile> = sources
            .into_iter()
            .filter(|(path, _)| !ignored(path))
            .filter_map(|(path, text)| {
                let lang = Lang::from_path(&path)?;
                let a = analyze(lang, &text);
                Some(SourceFile {
                    path,
                    lang,
                    lines: a.lines,
                    functions: a.functions,
                    notes: a.notes,
                })
            })
            .filter(|f| f.lines > 0)
            .collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let dirs = build_dirs(&files);
        let boss = files
            .iter()
            .enumerate()
            .flat_map(|(fi, f)| {
                f.functions
                    .iter()
                    .enumerate()
                    .map(move |(gi, g)| (fi, gi, g.len))
            })
            .max_by(|a, b| a.2.cmp(&b.2).then(b.0.cmp(&a.0)).then(b.1.cmp(&a.1)))
            .map(|(fi, gi, _)| (fi, gi));
        Repo {
            name: name.to_string(),
            files,
            commits,
            dirs,
            boss,
        }
    }

    pub fn boss_function(&self) -> Option<(&SourceFile, &Function)> {
        self.boss
            .map(|(fi, gi)| (&self.files[fi], &self.files[fi].functions[gi]))
    }

    /// The floor (folder) a file lives on.
    pub fn dir_of_file(&self, file: usize) -> usize {
        self.dirs
            .iter()
            .position(|d| d.files.contains(&file))
            .unwrap_or(0)
    }

    pub fn total_lines(&self) -> usize {
        self.files.iter().map(|f| f.lines).sum()
    }

    pub fn function_count(&self) -> usize {
        self.files.iter().map(|f| f.functions.len()).sum()
    }

    /// The folders from the root down to `dir`.
    pub fn path_to(&self, dir: usize) -> Vec<usize> {
        let mut out = vec![dir];
        let mut d = dir;
        while let Some(p) = self.dirs[d].parent {
            out.push(p);
            d = p;
        }
        out.reverse();
        out
    }

    /// Authors who committed under a folder, most commits first, with the
    /// subjects of their commits there. Commits without file lists count
    /// everywhere.
    pub fn ghosts(&self, dir: usize) -> Vec<(String, Vec<String>)> {
        let prefix = &self.dirs[dir].path;
        let mut by_author: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for c in &self.commits {
            let here = c.files.is_empty()
                || prefix.is_empty()
                || c.files
                    .iter()
                    .any(|f| f.starts_with(prefix.as_str()) && f[prefix.len()..].starts_with('/'));
            if here && !c.author.is_empty() && !c.subject.is_empty() {
                by_author.entry(&c.author).or_default().push(&c.subject);
            }
        }
        let mut list: Vec<(String, Vec<String>)> = by_author
            .into_iter()
            .map(|(a, s)| (a.to_string(), s.into_iter().map(str::to_string).collect()))
            .collect();
        list.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
        list
    }
}

fn parent_path(path: &str) -> &str {
    path.rsplit_once('/').map(|(p, _)| p).unwrap_or("")
}

fn build_dirs(files: &[SourceFile]) -> Vec<Dir> {
    // every folder that holds code, directly or below
    let mut paths: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    paths.insert(String::new(), vec![]);
    for (i, f) in files.iter().enumerate() {
        let dir = parent_path(&f.path).to_string();
        let mut p = dir.as_str();
        loop {
            paths.entry(p.to_string()).or_default();
            if p.is_empty() {
                break;
            }
            p = parent_path(p);
        }
        paths.get_mut(&dir).unwrap().push(i);
    }
    // children of each folder
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for p in paths.keys() {
        if !p.is_empty() {
            children
                .entry(parent_path(p).to_string())
                .or_default()
                .push(p.clone());
        }
    }
    // merge chains: a folder with no files and exactly one child folder
    fn collapse(
        p: &str,
        paths: &BTreeMap<String, Vec<usize>>,
        children: &BTreeMap<String, Vec<String>>,
    ) -> String {
        let mut p = p.to_string();
        loop {
            let kids = children.get(&p).map(Vec::as_slice).unwrap_or(&[]);
            if paths[&p].is_empty() && kids.len() == 1 && !p.is_empty() {
                p = kids[0].clone();
            } else {
                return p;
            }
        }
    }
    let mut dirs: Vec<Dir> = Vec::new();
    fn add(
        p: &str,
        parent: Option<usize>,
        depth: usize,
        files: &[SourceFile],
        paths: &BTreeMap<String, Vec<usize>>,
        children: &BTreeMap<String, Vec<String>>,
        dirs: &mut Vec<Dir>,
    ) -> usize {
        let id = dirs.len();
        dirs.push(Dir {
            path: p.to_string(),
            files: paths[p].clone(),
            children: vec![],
            parent,
            depth,
            total_lines: paths[p].iter().map(|&i| files[i].lines).sum(),
        });
        let kids: Vec<String> = children.get(p).cloned().unwrap_or_default();
        for k in kids {
            let k = collapse(&k, paths, children);
            let child = add(&k, Some(id), depth + 1, files, paths, children, dirs);
            dirs[id].children.push(child);
            dirs[id].total_lines += dirs[child].total_lines;
        }
        id
    }
    // a root that only leads into one folder starts there instead
    let root = if paths[""].is_empty() && children.get("").map_or(0, Vec::len) == 1 {
        collapse(&children[""][0], &paths, &children)
    } else {
        String::new()
    };
    add(&root, None, 0, files, &paths, &children, &mut dirs);
    // bigger folders first, so the stairs that matter come first
    let order: Vec<Vec<usize>> = dirs
        .iter()
        .map(|d| {
            let mut c = d.children.clone();
            c.sort_by(|&a, &b| {
                dirs[b]
                    .total_lines
                    .cmp(&dirs[a].total_lines)
                    .then(dirs[a].path.cmp(&dirs[b].path))
            });
            c
        })
        .collect();
    for (d, c) in dirs.iter_mut().zip(order) {
        d.children = c;
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> Repo {
        let src = |n: usize| {
            (0..n)
                .map(|i| format!("fn f{i}() {{\n    x();\n}}\n"))
                .collect::<String>()
        };
        Repo::from_sources(
            "demo",
            vec![
                ("README.md".into(), "# hi".into()),
                ("src/main.rs".into(), src(2)),
                (
                    "src/net/http.rs".into(),
                    format!("{}fn big() {{\n{}}}\n", src(1), "  y();\n".repeat(50)),
                ),
                ("src/net/tcp.rs".into(), src(3)),
                ("tools/a/b/c/gen.py".into(), "def gen():\n    pass\n".into()),
                (
                    "node_modules/x/index.js".into(),
                    "function x() {\n}\n".into(),
                ),
            ],
            vec![
                Commit {
                    author: "ada".into(),
                    subject: "speed up http".into(),
                    files: vec!["src/net/http.rs".into()],
                },
                Commit {
                    author: "bob".into(),
                    subject: "init".into(),
                    files: vec!["src/main.rs".into()],
                },
                Commit {
                    author: "ada".into(),
                    subject: "tcp keepalive".into(),
                    files: vec!["src/net/tcp.rs".into()],
                },
            ],
        )
    }

    #[test]
    fn tree() {
        let r = repo();
        assert_eq!(r.files.len(), 4);
        let labels: Vec<&str> = r.dirs.iter().map(|d| d.label()).collect();
        assert_eq!(labels, vec!["/", "src", "src/net", "tools/a/b/c"]);
        assert_eq!(r.dirs[0].children, vec![1, 3]);
        assert_eq!(r.dirs[2].depth, 2);
        assert_eq!(r.path_to(2), vec![0, 1, 2]);
    }

    #[test]
    fn boss_is_longest_function() {
        let r = repo();
        let (file, f) = r.boss_function().unwrap();
        assert_eq!(file.path, "src/net/http.rs");
        assert_eq!(f.name, "big");
        assert_eq!(f.len, 52);
    }

    #[test]
    fn ghosts_by_folder() {
        let r = repo();
        let g = r.ghosts(2);
        assert_eq!(g[0].0, "ada");
        assert_eq!(g[0].1.len(), 2);
        assert_eq!(g.len(), 1);
        assert_eq!(r.ghosts(0).len(), 2);
    }
}
