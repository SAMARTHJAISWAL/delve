//! Building a floor of the dungeon from one folder of the repository.
//!
//! Rooms are files (bigger files, bigger rooms), monsters are the functions
//! in them, loot is their TODO/FIXME comments, stairs lead into subfolders
//! and the ghosts are the people who committed here.

use crate::analyze::NoteKind;
use crate::repo::Repo;
use crate::rng::{hash, Rng};

pub const MAP_W: i32 = 64;
pub const MAP_H: i32 = 22;

const GRID_COLS: i32 = 4;
const GRID_ROWS: i32 = 3;
const MAX_ROOMS: usize = (GRID_COLS * GRID_ROWS) as usize;
const MAX_STAIRS: usize = 4;
const MONSTERS_PER_ROOM: usize = 3;
const ITEMS_PER_ROOM: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

impl Pos {
    pub fn new(x: i32, y: i32) -> Pos {
        Pos { x, y }
    }
    pub fn dist(self, o: Pos) -> i32 {
        (self.x - o.x).abs().max((self.y - o.y).abs())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    Void,
    Wall,
    Floor,
    Door,
    Corridor,
    /// Stairs down into the n-th subfolder of this floor's folder.
    Down(usize),
    Up,
}

impl Tile {
    pub fn walkable(self) -> bool {
        !matches!(self, Tile::Void | Tile::Wall)
    }
}

#[derive(Clone, Debug)]
pub struct Room {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub file: Option<usize>,
    pub label: String,
}

impl Room {
    /// Inside the walls.
    pub fn contains(&self, p: Pos) -> bool {
        p.x > self.x && p.x < self.x + self.w - 1 && p.y > self.y && p.y < self.y + self.h - 1
    }
    /// Including the walls.
    pub fn covers(&self, p: Pos) -> bool {
        p.x >= self.x && p.x < self.x + self.w && p.y >= self.y && p.y < self.y + self.h
    }
    pub fn center(&self) -> Pos {
        Pos::new(self.x + self.w / 2, self.y + self.h / 2)
    }
}

#[derive(Clone, Debug)]
pub struct Monster {
    pub name: String,
    pub creature: &'static str,
    pub file: usize,
    pub line: usize,
    pub len: usize,
    pub pos: Pos,
    pub hp: i32,
    pub max_hp: i32,
    pub atk: i32,
    pub def: i32,
    pub xp: u32,
    pub boss: bool,
    pub awake: bool,
}

#[derive(Clone, Debug)]
pub struct Item {
    /// The comment this item came from; `None` for a cup of coffee.
    pub kind: Option<NoteKind>,
    pub text: String,
    pub file: usize,
    pub line: usize,
}

#[derive(Clone, Debug)]
pub struct Ghost {
    pub author: String,
    pub subjects: Vec<String>,
    pub pos: Pos,
    pub said: usize,
}

#[derive(Clone, Debug)]
pub struct Floor {
    pub dir: usize,
    pub tiles: Vec<Tile>,
    pub seen: Vec<bool>,
    pub rooms: Vec<Room>,
    pub monsters: Vec<Monster>,
    pub items: Vec<(Pos, Item)>,
    pub ghosts: Vec<Ghost>,
    /// Where you arrive: next to the up stairs, or the entrance on floor one.
    pub entry: Pos,
    /// Files in the folder that did not get a room of their own.
    pub hidden_files: usize,
    /// Subfolders whose stairs did not fit.
    pub hidden_dirs: usize,
}

impl Floor {
    pub fn tile(&self, p: Pos) -> Tile {
        if p.x < 0 || p.y < 0 || p.x >= MAP_W || p.y >= MAP_H {
            Tile::Void
        } else {
            self.tiles[(p.y * MAP_W + p.x) as usize]
        }
    }

    fn set(&mut self, p: Pos, t: Tile) {
        if p.x >= 0 && p.y >= 0 && p.x < MAP_W && p.y < MAP_H {
            self.tiles[(p.y * MAP_W + p.x) as usize] = t;
        }
    }

    pub fn is_seen(&self, p: Pos) -> bool {
        p.x >= 0
            && p.y >= 0
            && p.x < MAP_W
            && p.y < MAP_H
            && self.seen[(p.y * MAP_W + p.x) as usize]
    }

    pub fn see(&mut self, p: Pos) {
        if p.x >= 0 && p.y >= 0 && p.x < MAP_W && p.y < MAP_H {
            self.seen[(p.y * MAP_W + p.x) as usize] = true;
        }
    }

    pub fn room_at(&self, p: Pos) -> Option<usize> {
        self.rooms.iter().position(|r| r.contains(p))
    }

    pub fn monster_at(&self, p: Pos) -> Option<usize> {
        self.monsters.iter().position(|m| m.pos == p && m.hp > 0)
    }

    pub fn ghost_at(&self, p: Pos) -> Option<usize> {
        self.ghosts.iter().position(|g| g.pos == p)
    }

    pub fn occupied(&self, p: Pos) -> bool {
        self.monster_at(p).is_some() || self.ghost_at(p).is_some()
    }

    /// Where the stairs into a subfolder are.
    pub fn stairs_down(&self, child: usize) -> Option<Pos> {
        self.tiles
            .iter()
            .position(|&t| t == Tile::Down(child))
            .map(|i| Pos::new(i as i32 % MAP_W, i as i32 / MAP_W))
    }

    pub fn stairs_up(&self) -> Option<Pos> {
        self.tiles
            .iter()
            .position(|&t| t == Tile::Up)
            .map(|i| Pos::new(i as i32 % MAP_W, i as i32 / MAP_W))
    }
}

/// How strong a function is, 1..=10, by its length's rank in the repository.
/// Ranking (not raw length) keeps a repo of 2,000-line functions as playable
/// as one of 20-line functions.
pub struct Strength {
    lengths: Vec<usize>,
}

impl Strength {
    pub fn new(repo: &Repo) -> Strength {
        let mut lengths: Vec<usize> = repo
            .files
            .iter()
            .flat_map(|f| f.functions.iter().map(|g| g.len))
            .collect();
        lengths.sort_unstable();
        Strength { lengths }
    }

    pub fn tier(&self, len: usize) -> i32 {
        if self.lengths.is_empty() {
            return 1;
        }
        let below = self.lengths.partition_point(|&l| l < len);
        1 + (9 * below / self.lengths.len().max(1)) as i32
    }
}

pub fn make_monster(
    repo: &Repo,
    strength: &Strength,
    file: usize,
    func: usize,
    pos: Pos,
) -> Monster {
    let f = &repo.files[file];
    let g = &f.functions[func];
    let boss = repo.boss == Some((file, func));
    let tier = strength.tier(g.len);
    let (hp, atk, def, xp) = if boss {
        // the boss grows with the repository, so small repos stay winnable
        let n = repo.function_count() as i32;
        (
            40 + (n / 5).min(80),
            4 + (n / 80).min(4),
            1 + i32::from(n > 300),
            200,
        )
    } else {
        (
            3 + tier * 2,
            1 + tier / 3,
            i32::from(tier >= 8),
            (2 + tier * 2) as u32,
        )
    };
    Monster {
        name: g.name.clone(),
        creature: f.lang.creature(),
        file,
        line: g.line,
        len: g.len,
        pos,
        hp,
        max_hp: hp,
        atk,
        def,
        xp,
        boss,
        awake: false,
    }
}

/// Generate the floor for a folder. The same repository, folder and seed
/// always give the same floor.
pub fn generate(repo: &Repo, dir: usize, seed: u64, strength: &Strength) -> Floor {
    let d = &repo.dirs[dir];
    let mut rng = Rng::new(seed ^ hash(&d.path).rotate_left(17));

    // stairs: the biggest subfolders, plus the way to the boss if it is not among them
    let boss_dir = repo.boss.map(|(fi, _)| repo.dir_of_file(fi));
    let boss_path = boss_dir.map(|b| repo.path_to(b)).unwrap_or_default();
    let mut stairs: Vec<usize> = (0..d.children.len()).take(MAX_STAIRS).collect();
    if let Some(i) = d.children.iter().position(|c| boss_path.contains(c)) {
        if !stairs.contains(&i) {
            stairs.pop();
            stairs.push(i);
        }
    }
    let hidden_dirs = d.children.len() - stairs.len();

    // files: biggest first; the boss's file always gets a room
    let mut files = d.files.clone();
    files.sort_by(|&a, &b| {
        repo.files[b]
            .lines
            .cmp(&repo.files[a].lines)
            .then(a.cmp(&b))
    });
    let room_budget = MAX_ROOMS - 1 - stairs.len();
    let mut chosen: Vec<usize> = files.iter().copied().take(room_budget).collect();
    if let Some((bf, _)) = repo.boss {
        if files.contains(&bf) && !chosen.contains(&bf) {
            chosen.pop();
            chosen.push(bf);
        }
    }
    let hidden_files = files.len() - chosen.len();

    // which grid cells the rooms go in
    let mut cells: Vec<i32> = (0..GRID_COLS * GRID_ROWS).collect();
    rng.shuffle(&mut cells);
    let wanted = 1 + stairs.len() + chosen.len();
    let mut cells: Vec<i32> = cells.into_iter().take(wanted).collect();
    cells.sort_unstable();

    let mut floor = Floor {
        dir,
        tiles: vec![Tile::Void; (MAP_W * MAP_H) as usize],
        seen: vec![false; (MAP_W * MAP_H) as usize],
        rooms: vec![],
        monsters: vec![],
        items: vec![],
        ghosts: vec![],
        entry: Pos::new(0, 0),
        hidden_files,
        hidden_dirs,
    };

    // room contents in order: entrance, stairs, files (shuffled across the cells)
    #[derive(Clone, Copy)]
    enum Kind {
        Entry,
        Stairs(usize),
        File(usize),
    }
    let mut kinds: Vec<Kind> = vec![Kind::Entry];
    kinds.extend(stairs.iter().map(|&s| Kind::Stairs(s)));
    kinds.extend(chosen.iter().map(|&f| Kind::File(f)));
    let first = kinds.remove(0);
    rng.shuffle(&mut kinds);
    kinds.insert(rng.below(kinds.len() + 1), first);

    let cell_w = MAP_W / GRID_COLS;
    let biggest = chosen
        .iter()
        .map(|&f| repo.files[f].lines)
        .max()
        .unwrap_or(1)
        .max(1);
    for (&cell, &kind) in cells.iter().zip(kinds.iter()) {
        let cx = (cell % GRID_COLS) * cell_w;
        let row = cell / GRID_COLS;
        let cy = row * MAP_H / GRID_ROWS;
        let cell_h = (row + 1) * MAP_H / GRID_ROWS - cy;
        let (want_w, want_h) = match kind {
            Kind::File(f) => {
                // area grows with the log of the file's size
                let share = (repo.files[f].lines as f64).ln_1p() / (biggest as f64).ln_1p();
                (7 + (share * 7.0) as i32, 4 + (share * 2.5) as i32)
            }
            _ => (11, 4),
        };
        let w = want_w.min(cell_w - 2).max(5);
        let h = want_h.min(cell_h - 1).max(4);
        let x = cx + 1 + rng.below((cell_w - w - 1).max(1) as usize) as i32;
        let y = cy + rng.below((cell_h - h).max(1) as usize) as i32;
        let (file, label) = match kind {
            Kind::Entry if d.parent.is_some() => (None, "stairs up".to_string()),
            Kind::Entry => (None, "entrance".to_string()),
            Kind::Stairs(i) => {
                let child = &repo.dirs[d.children[i]];
                let short = child
                    .path
                    .strip_prefix(&format!("{}/", d.path))
                    .unwrap_or(&child.path);
                (None, format!("{short}/"))
            }
            Kind::File(f) => (Some(f), repo.files[f].name().to_string()),
        };
        let room = Room {
            x,
            y,
            w,
            h,
            file,
            label,
        };
        for yy in y..y + h {
            for xx in x..x + w {
                let edge = yy == y || yy == y + h - 1 || xx == x || xx == x + w - 1;
                floor.set(
                    Pos::new(xx, yy),
                    if edge { Tile::Wall } else { Tile::Floor },
                );
            }
        }
        match kind {
            Kind::Entry => {
                let c = room.center();
                if d.parent.is_some() {
                    floor.set(c, Tile::Up);
                    floor.entry = Pos::new(c.x + 1, c.y);
                } else {
                    floor.entry = c;
                }
            }
            Kind::Stairs(i) => floor.set(room.center(), Tile::Down(i)),
            Kind::File(_) => {}
        }
        floor.rooms.push(room);
    }

    connect(&mut floor, &mut rng);

    // monsters and loot
    let mut taken: Vec<Pos> = vec![floor.entry];
    for r in 0..floor.rooms.len() {
        let Some(fi) = floor.rooms[r].file else {
            continue;
        };
        let f = &repo.files[fi];
        let mut funcs: Vec<usize> = (0..f.functions.len()).collect();
        funcs.sort_by(|&a, &b| f.functions[b].len.cmp(&f.functions[a].len).then(a.cmp(&b)));
        let mut picked: Vec<usize> = funcs.iter().copied().take(MONSTERS_PER_ROOM).collect();
        if let Some((bf, bg)) = repo.boss {
            if bf == fi && !picked.contains(&bg) {
                picked.pop();
                picked.push(bg);
            }
        }
        for g in picked {
            if let Some(p) = free_spot(&floor, r, &taken, &mut rng) {
                taken.push(p);
                floor.monsters.push(make_monster(repo, strength, fi, g, p));
            }
        }
        // a variety of notes: one of each kind before seconds
        let mut notes: Vec<usize> = (0..f.notes.len()).collect();
        notes.sort_by_key(|&n| (f.notes[n].kind, f.notes[n].line));
        let mut seen_kinds = vec![];
        let mut ordered = vec![];
        for &n in &notes {
            if !seen_kinds.contains(&f.notes[n].kind) {
                seen_kinds.push(f.notes[n].kind);
                ordered.push(n);
            }
        }
        for &n in &notes {
            if !ordered.contains(&n) {
                ordered.push(n);
            }
        }
        // a codebase with no TODOs still needs something to drink
        if f.notes.is_empty() && rng.chance(35) {
            if let Some(p) = free_spot(&floor, r, &taken, &mut rng) {
                taken.push(p);
                floor.items.push((
                    p,
                    Item {
                        kind: None,
                        text: String::new(),
                        file: fi,
                        line: 0,
                    },
                ));
            }
        }
        for n in ordered.into_iter().take(ITEMS_PER_ROOM) {
            if let Some(p) = free_spot(&floor, r, &taken, &mut rng) {
                taken.push(p);
                let note = &f.notes[n];
                floor.items.push((
                    p,
                    Item {
                        kind: Some(note.kind),
                        text: note.text.clone(),
                        file: fi,
                        line: note.line,
                    },
                ));
            }
        }
    }

    // ghosts of the people who worked here
    let ghosts = repo.ghosts(dir);
    for (author, subjects) in ghosts.into_iter().take(2) {
        let candidates: Vec<usize> = (0..floor.rooms.len())
            .filter(|&r| !floor.rooms[r].contains(floor.entry))
            .collect();
        if candidates.is_empty() {
            break;
        }
        let r = candidates[rng.below(candidates.len())];
        if let Some(p) = free_spot(&floor, r, &taken, &mut rng) {
            taken.push(p);
            floor.ghosts.push(Ghost {
                author,
                subjects,
                pos: p,
                said: 0,
            });
        }
    }
    floor
}

fn free_spot(floor: &Floor, room: usize, taken: &[Pos], rng: &mut Rng) -> Option<Pos> {
    let r = &floor.rooms[room];
    for _ in 0..40 {
        let p = Pos::new(
            r.x + 1 + rng.below((r.w - 2) as usize) as i32,
            r.y + 1 + rng.below((r.h - 2) as usize) as i32,
        );
        if floor.tile(p) == Tile::Floor && !taken.contains(&p) {
            return Some(p);
        }
    }
    None
}

/// Join the rooms with corridors: a chain through the grid order, plus a
/// couple of extra loops so the floor is not a single path.
fn connect(floor: &mut Floor, rng: &mut Rng) {
    let n = floor.rooms.len();
    for i in 1..n {
        let a = floor.rooms[i - 1].center();
        let b = floor.rooms[i].center();
        corridor(floor, a, b, rng.chance(50));
    }
    let extra = if n > 3 { 1 + rng.below(2) } else { 0 };
    for _ in 0..extra {
        let i = rng.below(n);
        let j = rng.below(n);
        if i != j && (i as i32 - j as i32).abs() > 1 {
            let (a, b) = (floor.rooms[i].center(), floor.rooms[j].center());
            corridor(floor, a, b, rng.chance(50));
        }
    }
}

/// The tiles of an L-shaped path from `a` to `b` (excluding `a`).
fn l_path(a: Pos, b: Pos, horizontal_first: bool) -> Vec<Pos> {
    let step = |from: i32, to: i32| if to > from { 1 } else { -1 };
    let (mut x, mut y) = (a.x, a.y);
    let mut out = vec![];
    let go_x = |x: &mut i32, y: i32, out: &mut Vec<Pos>| {
        while *x != b.x {
            *x += step(*x, b.x);
            out.push(Pos::new(*x, y));
        }
    };
    if horizontal_first {
        go_x(&mut x, y, &mut out);
        while y != b.y {
            y += step(y, b.y);
            out.push(Pos::new(x, y));
        }
    } else {
        while y != b.y {
            y += step(y, b.y);
            out.push(Pos::new(x, y));
        }
        go_x(&mut x, y, &mut out);
    }
    out
}

fn corridor(floor: &mut Floor, a: Pos, b: Pos, horizontal_first: bool) {
    // prefer the bend that crosses walls rather than running along them
    let along = |path: &[Pos]| {
        path.windows(2)
            .filter(|w| floor.tile(w[0]) == Tile::Wall && floor.tile(w[1]) == Tile::Wall)
            .count()
    };
    let first = l_path(a, b, horizontal_first);
    let second = l_path(a, b, !horizontal_first);
    let path = if along(&second) < along(&first) {
        second
    } else {
        first
    };
    for p in path {
        match floor.tile(p) {
            Tile::Void => floor.set(p, Tile::Corridor),
            Tile::Wall => {
                let straight = floor.tile(Pos::new(p.x - 1, p.y)) == Tile::Wall
                    && floor.tile(Pos::new(p.x + 1, p.y)) == Tile::Wall
                    || floor.tile(Pos::new(p.x, p.y - 1)) == Tile::Wall
                        && floor.tile(Pos::new(p.x, p.y + 1)) == Tile::Wall;
                floor.set(p, if straight { Tile::Door } else { Tile::Floor });
            }
            _ => {}
        }
    }
}

/// Every walkable tile reachable from `from`.
pub fn reachable(floor: &Floor, from: Pos) -> Vec<Pos> {
    let mut seen = vec![false; (MAP_W * MAP_H) as usize];
    let mut stack = vec![from];
    let mut out = vec![];
    while let Some(p) = stack.pop() {
        if !floor.tile(p).walkable() {
            continue;
        }
        let i = (p.y * MAP_W + p.x) as usize;
        if seen[i] {
            continue;
        }
        seen[i] = true;
        out.push(p);
        for (dx, dy) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            stack.push(Pos::new(p.x + dx, p.y + dy));
        }
    }
    out
}

#[cfg(test)]
pub mod tests_support {
    use crate::repo::{Commit, Repo};

    pub fn sample_repo() -> Repo {
        let mut sources = vec![];
        for d in [
            "",
            "src",
            "src/net",
            "src/ui",
            "src/ui/widgets",
            "docs/examples",
            "tests",
        ] {
            for i in 0..7 {
                let body = "    work();\n".repeat(3 + i * 7);
                let notes = if i % 3 == 0 {
                    "    // TODO: tidy this\n    // FIXME: breaks on empty input\n"
                } else {
                    ""
                };
                let path = if d.is_empty() {
                    format!("f{i}.rs")
                } else {
                    format!("{d}/f{i}.rs")
                };
                sources.push((
                    path,
                    format!("fn a{i}() {{\n{body}{notes}}}\nfn b{i}() {{\n    x();\n}}\n"),
                ));
            }
        }
        sources.push((
            "src/ui/widgets/huge.rs".into(),
            format!("fn monolith() {{\n{}}}\n", "    y();\n".repeat(400)),
        ));
        Repo::from_sources(
            "sample",
            sources,
            vec![Commit {
                author: "ada".into(),
                subject: "make it fast".into(),
                files: vec![],
            }],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::sample_repo;
    use super::*;

    #[test]
    fn floors_are_connected_and_deterministic() {
        let repo = sample_repo();
        let s = Strength::new(&repo);
        for dir in 0..repo.dirs.len() {
            for seed in 0..20 {
                let f = generate(&repo, dir, seed, &s);
                let reach = reachable(&f, f.entry);
                for (i, child) in repo.dirs[dir].children.iter().enumerate() {
                    if let Some(p) = f.stairs_down(i) {
                        assert!(
                            reach.contains(&p),
                            "stairs to {} unreachable",
                            repo.dirs[*child].path
                        );
                    }
                }
                for m in &f.monsters {
                    assert!(
                        reach.contains(&m.pos),
                        "monster {} unreachable (dir {dir}, seed {seed})",
                        m.name
                    );
                }
                for (p, _) in &f.items {
                    assert!(reach.contains(p));
                }
                assert!(f.tile(f.entry).walkable());
                let again = generate(&repo, dir, seed, &s);
                assert_eq!(again.tiles, f.tiles);
            }
        }
    }

    #[test]
    fn boss_is_placed_and_reachable_by_stairs() {
        let repo = sample_repo();
        let s = Strength::new(&repo);
        let (file, func) = repo.boss_function().unwrap();
        assert_eq!(func.name, "monolith");
        let boss_dir = repo.dir_of_file(repo.boss.unwrap().0);
        assert_eq!(repo.dirs[boss_dir].path, "src/ui/widgets");
        // every floor on the way down has stairs to the next one
        let path = repo.path_to(boss_dir);
        for w in path.windows(2) {
            let f = generate(&repo, w[0], 3, &s);
            let i = repo.dirs[w[0]]
                .children
                .iter()
                .position(|&c| c == w[1])
                .unwrap();
            assert!(f.stairs_down(i).is_some());
        }
        let f = generate(&repo, boss_dir, 3, &s);
        assert!(f.monsters.iter().any(|m| m.boss && m.name == "monolith"));
        assert!(file.path.ends_with("huge.rs"));
    }

    #[test]
    fn strength_tiers() {
        let repo = sample_repo();
        let s = Strength::new(&repo);
        assert_eq!(s.tier(1), 1);
        assert_eq!(s.tier(10_000), 10);
    }
}
