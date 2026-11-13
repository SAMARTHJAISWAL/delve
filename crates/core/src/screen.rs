//! Drawing the game into a grid of coloured characters. The terminal and the
//! browser both just copy this grid to the screen, so they look the same.

use crate::dungeon::{Pos, Tile, MAP_H, MAP_W};
use crate::game::{item_glyph, item_name, xp_for_level, Game, State, Tone, PACK_SIZE};

pub const WIDTH: usize = 96;
pub const HEIGHT: usize = 30;
const SIDE_X: usize = 66;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Default = 0,
    Dim = 1,
    Wall = 2,
    Floor = 3,
    Player = 4,
    Red = 5,
    Green = 6,
    Yellow = 7,
    Blue = 8,
    Magenta = 9,
    Cyan = 10,
    White = 11,
    Boss = 12,
    Bar = 13,
    Gold = 14,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
}

impl Default for Cell {
    fn default() -> Cell {
        Cell {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    Map,
}

pub struct Screen {
    pub cells: Vec<Cell>,
}

impl Default for Screen {
    fn default() -> Self {
        Screen::new()
    }
}

impl Screen {
    pub fn new() -> Screen {
        Screen {
            cells: vec![Cell::default(); WIDTH * HEIGHT],
        }
    }

    pub fn clear(&mut self) {
        self.cells.fill(Cell::default());
    }

    pub fn put(&mut self, x: usize, y: usize, ch: char, fg: Color, bg: Color) {
        if x < WIDTH && y < HEIGHT {
            self.cells[y * WIDTH + x] = Cell { ch, fg, bg };
        }
    }

    /// Write text, clipped to `max` characters. Returns the next column.
    pub fn text(&mut self, x: usize, y: usize, text: &str, fg: Color, max: usize) -> usize {
        let mut col = x;
        for ch in text.chars().take(max) {
            self.put(col, y, ch, fg, Color::Default);
            col += 1;
        }
        col
    }

    pub fn fill_row(&mut self, y: usize, bg: Color) {
        for x in 0..WIDTH {
            self.cells[y * WIDTH + x].bg = bg;
        }
    }

    pub fn row_text(&self, y: usize) -> String {
        self.cells[y * WIDTH..(y + 1) * WIDTH]
            .iter()
            .map(|c| c.ch)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    /// The whole screen as plain text (for tests and screenshots).
    pub fn to_text(&self) -> String {
        (0..HEIGHT)
            .map(|y| self.row_text(y))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Wrap text into lines of at most `width` characters.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![];
    let mut line = String::new();
    for word in text.split_whitespace() {
        let word: String = if word.chars().count() > width {
            word.chars().take(width).collect()
        } else {
            word.to_string()
        };
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn group(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn draw(game: &Game, overlay: Overlay, screen: &mut Screen) {
    screen.clear();
    header(game, screen);
    match overlay {
        Overlay::Help => help(screen),
        Overlay::Map => tree(game, screen),
        Overlay::None => map(game, screen),
    }
    sidebar(game, screen);
    log(game, screen);
    if game.state != State::Playing {
        ending(game, screen);
    }
}

fn header(game: &Game, s: &mut Screen) {
    s.fill_row(0, Color::Bar);
    let mut x = s.text(1, 0, "delve", Color::Gold, 5);
    x = s.text(x + 2, 0, &game.repo.name, Color::White, 30);
    let path = game.repo.dirs[game.dir].label();
    x = s.text(x + 1, 0, "›", Color::Dim, 1);
    s.text(x + 1, 0, path, Color::Cyan, 40);
    let depth = format!("floor {}", game.repo.dirs[game.dir].depth + 1);
    s.text(WIDTH - depth.len() - 1, 0, &depth, Color::Dim, 20);
    for x in 0..WIDTH {
        s.cells[x].bg = Color::Bar;
    }
}

fn map(game: &Game, s: &mut Screen) {
    let floor = game.floor();
    let oy = 1;
    for y in 0..MAP_H {
        for x in 0..MAP_W {
            let p = Pos::new(x, y);
            if !floor.is_seen(p) {
                continue;
            }
            let (ch, fg) = match floor.tile(p) {
                Tile::Void => continue,
                Tile::Wall => ('#', Color::Wall),
                Tile::Floor => ('.', Color::Floor),
                Tile::Door => ('+', Color::Yellow),
                Tile::Corridor => ('░', Color::Wall),
                Tile::Down(_) => ('>', Color::Gold),
                Tile::Up => ('<', Color::Gold),
            };
            s.put(x as usize, y as usize + oy, ch, fg, Color::Default);
        }
    }
    // file names on the top walls of the rooms you have seen
    for room in &floor.rooms {
        if !floor.is_seen(Pos::new(room.x, room.y)) {
            continue;
        }
        let max = (room.w - 2).max(0) as usize;
        let room_label = if room.label.chars().count() + 2 <= max {
            format!(" {} ", room.label)
        } else {
            room.label.clone()
        };
        let label: String = if room_label.chars().count() > max {
            let cut: String = room.label.chars().take(max.saturating_sub(1)).collect();
            format!("{cut}…")
        } else {
            room_label
        };
        let color = if room.file.is_some() {
            Color::Cyan
        } else {
            Color::Gold
        };
        s.text(
            (room.x + 1) as usize,
            room.y as usize + oy,
            &label,
            color,
            max,
        );
    }
    for (p, item) in &floor.items {
        if floor.is_seen(*p) {
            let color = if item.kind.is_none() {
                Color::Green
            } else {
                Color::Gold
            };
            s.put(
                p.x as usize,
                p.y as usize + oy,
                item_glyph(item.kind),
                color,
                Color::Default,
            );
        }
    }
    for g in &floor.ghosts {
        if game.visible(g.pos) {
            s.put(
                g.pos.x as usize,
                g.pos.y as usize + oy,
                '&',
                Color::Magenta,
                Color::Default,
            );
        }
    }
    for m in &floor.monsters {
        if game.visible(m.pos) {
            let first = m.creature.chars().next().unwrap_or('m');
            let (ch, fg) = if m.boss {
                (first.to_ascii_uppercase(), Color::Boss)
            } else {
                (first, Color::Red)
            };
            s.put(
                m.pos.x as usize,
                m.pos.y as usize + oy,
                ch,
                fg,
                Color::Default,
            );
        }
    }
    let p = game.player.pos;
    s.put(
        p.x as usize,
        p.y as usize + oy,
        '@',
        Color::Player,
        Color::Default,
    );
}

fn bar(s: &mut Screen, x: usize, y: usize, width: usize, value: i32, max: i32, color: Color) {
    let filled = if max > 0 {
        (value.max(0) as usize * width).div_ceil(max as usize)
    } else {
        0
    };
    for i in 0..width {
        let ch = if i < filled { '█' } else { '░' };
        s.put(
            x + i,
            y,
            ch,
            if i < filled { color } else { Color::Dim },
            Color::Default,
        );
    }
}

fn sidebar(game: &Game, s: &mut Screen) {
    let x = SIDE_X;
    let w = WIDTH - SIDE_X - 1;
    let p = &game.player;
    let mut y = 2;
    s.text(x, y, &format!("Level {}", p.level), Color::White, w);
    let next = format!("XP {}/{}", p.xp, xp_for_level(p.level));
    s.text(WIDTH - 1 - next.len(), y, &next, Color::Dim, w);
    y += 1;
    s.text(x, y, "HP", Color::Dim, 2);
    let hp_color = if p.hp * 3 < p.max_hp {
        Color::Red
    } else {
        Color::Green
    };
    bar(s, x + 3, y, 16, p.hp, p.max_hp, hp_color);
    s.text(
        x + 20,
        y,
        &format!("{}/{}", p.hp.max(0), p.max_hp),
        Color::White,
        9,
    );
    y += 1;
    s.text(
        x,
        y,
        &format!("ATK {}  DEF {}", p.atk, p.def),
        Color::Dim,
        w,
    );
    y += 2;

    s.text(x, y, "QUEST", Color::Gold, w);
    y += 1;
    match game.quest() {
        Some((name, len, path)) => {
            s.text(x, y, &format!("Slay `{name}`"), Color::Boss, w);
            y += 1;
            s.text(x, y, &format!("{} lines", group(len)), Color::Dim, w);
            y += 1;
            for line in wrap(&path, w).into_iter().take(2) {
                s.text(x, y, &line, Color::Dim, w);
                y += 1;
            }
        }
        None => {
            s.text(x, y, "Explore. Nothing hunts here.", Color::Dim, w);
            y += 1;
        }
    }
    y += 1;

    let floor = game.floor();
    s.text(x, y, "HERE", Color::Gold, w);
    y += 1;
    match floor.room_at(p.pos).and_then(|r| floor.rooms[r].file) {
        Some(fi) => {
            let f = &game.repo.files[fi];
            s.text(x, y, f.name(), Color::Cyan, w);
            y += 1;
            s.text(
                x,
                y,
                &format!("{} · {} lines", f.lang.name(), group(f.lines)),
                Color::Dim,
                w,
            );
            y += 1;
            let alive = floor
                .monsters
                .iter()
                .filter(|m| m.hp > 0 && floor.rooms[floor.room_at(p.pos).unwrap()].contains(m.pos))
                .count();
            s.text(
                x,
                y,
                &format!("{} functions, {} awake here", f.functions.len(), alive),
                Color::Dim,
                w,
            );
            y += 1;
        }
        None => {
            let d = &game.repo.dirs[game.dir];
            s.text(
                x,
                y,
                &format!("{} files, {} subfolders", d.files.len(), d.children.len()),
                Color::Dim,
                w,
            );
            y += 1;
            if floor.hidden_files + floor.hidden_dirs > 0 {
                s.text(
                    x,
                    y,
                    &format!(
                        "({} small rooms unmapped)",
                        floor.hidden_files + floor.hidden_dirs
                    ),
                    Color::Dim,
                    w,
                );
                y += 1;
            }
        }
    }
    y += 1;

    s.text(
        x,
        y,
        &format!("PACK {}/{}", p.pack.len(), PACK_SIZE),
        Color::Gold,
        w,
    );
    y += 1;
    if p.pack.is_empty() {
        s.text(x, y, "TODOs and FIXMEs are loot.", Color::Dim, w);
    }
    for (i, item) in p.pack.iter().enumerate() {
        if y >= 22 {
            break;
        }
        let line = format!(
            "{} {} {}",
            i + 1,
            item_glyph(item.kind),
            item_name(item.kind)
        );
        s.text(x, y, &line, Color::White, w);
        y += 1;
    }
}

fn log(game: &Game, s: &mut Screen) {
    for x in 0..WIDTH {
        s.put(x, 23, '─', Color::Dim, Color::Default);
    }
    let lines: Vec<(String, Tone, bool)> = game
        .log
        .iter()
        .rev()
        .flat_map(|m| {
            let fresh = m.turn + 1 >= game.stats.turns;
            let mut ls: Vec<(String, Tone, bool)> = wrap(&m.text, WIDTH - 2)
                .into_iter()
                .map(|l| (l, m.tone, fresh))
                .collect();
            ls.reverse();
            ls
        })
        .take(5)
        .collect();
    for (i, (line, tone, fresh)) in lines.iter().rev().enumerate() {
        let color = match (tone, fresh) {
            (_, false) => Color::Dim,
            (Tone::Plain, true) => Color::White,
            (Tone::Good, true) => Color::Green,
            (Tone::Bad, true) => Color::Red,
            (Tone::Loot, true) => Color::Gold,
            (Tone::Ghost, true) => Color::Magenta,
            (Tone::Boss, true) => Color::Boss,
        };
        s.text(1, 24 + i, line, color, WIDTH - 2);
    }
    let keys = "arrows/hjkl move  bump attack  > down  < up  1-9 use  m map  ? help  q quit";
    s.fill_row(HEIGHT - 1, Color::Bar);
    s.text(1, HEIGHT - 1, keys, Color::Dim, WIDTH - 2);
    for x in 0..WIDTH {
        s.cells[(HEIGHT - 1) * WIDTH + x].bg = Color::Bar;
    }
}

fn help(s: &mut Screen) {
    let lines = [
        ("HOW TO PLAY", Color::Gold),
        ("", Color::Default),
        (
            "Your repository is the dungeon. Every folder is a floor, every",
            Color::White,
        ),
        (
            "file a room, every function a monster: the longer the function,",
            Color::White,
        ),
        (
            "the tougher it is. The longest function in the repo is the boss.",
            Color::White,
        ),
        ("", Color::Default),
        (
            "@  you              >  stairs into a subfolder   <  stairs up",
            Color::White,
        ),
        (
            "r  a Rust crab, s a Python serpent, g a JS goblin, t a TS troll...",
            Color::Red,
        ),
        (
            "&  the ghost of a committer: bump into them to hear their commits",
            Color::Magenta,
        ),
        ("!  potion of TODO      heals you", Color::Gold),
        (
            "?  scroll of FIXME     reveals the whole floor",
            Color::Gold,
        ),
        (
            "*  XXX bomb            damages everything nearby",
            Color::Gold,
        ),
        (
            "%  bug in a jar        feature or bite: who knows",
            Color::Gold,
        ),
        (
            "/  hacky blade         +2 attack, picked up at once",
            Color::Gold,
        ),
        (
            "!  cup of coffee       (green) heals; found where there are no TODOs",
            Color::Green,
        ),
        (
            "Move with arrows, hjkl or WASD (yubn for diagonals). Walk into a",
            Color::White,
        ),
        (
            "monster to fight it. Press a number to use an item from your pack.",
            Color::White,
        ),
        (
            "m shows the folder tree and where the boss waits.",
            Color::White,
        ),
        ("", Color::Default),
        ("Press any key to return.", Color::Dim),
    ];
    for (i, (text, color)) in lines.iter().enumerate() {
        s.text(2, 2 + i, text, *color, 63);
    }
}

fn tree(game: &Game, s: &mut Screen) {
    s.text(2, 2, "THE DUNGEON", Color::Gold, 60);
    let boss_dir = game.repo.boss.map(|(fi, _)| game.repo.dir_of_file(fi));
    let mut y = 4;
    let mut order = vec![(0usize, 0usize)];
    let mut rows = vec![];
    while let Some((d, depth)) = order.pop() {
        rows.push((d, depth));
        for &c in game.repo.dirs[d].children.iter().rev() {
            order.push((c, depth + 1));
        }
    }
    let total = rows.len();
    for (d, depth) in rows.into_iter().take(17) {
        let dir = &game.repo.dirs[d];
        let name = if d == 0 {
            game.repo.name.as_str()
        } else {
            dir.path.rsplit_once('/').map(|x| x.1).unwrap_or(&dir.path)
        };
        let name = if d != 0
            && dir.parent.is_some_and(|p| {
                game.repo.dirs[p].path.is_empty()
                    || !dir
                        .path
                        .starts_with(&format!("{}/", game.repo.dirs[p].path))
            }) {
            dir.path.as_str()
        } else if d != 0 && dir.parent.is_some() {
            dir.path
                .strip_prefix(&format!("{}/", game.repo.dirs[dir.parent.unwrap()].path))
                .unwrap_or(name)
        } else {
            name
        };
        let marker = if d == game.dir {
            ('@', Color::Player)
        } else if Some(d) == boss_dir {
            ('☠', Color::Boss)
        } else if game.stats.floors.contains(&d) {
            ('✓', Color::Green)
        } else {
            ('·', Color::Dim)
        };
        let x = 3 + depth * 2;
        s.put(x, y, marker.0, marker.1, Color::Default);
        let end = s.text(
            x + 2,
            y,
            &format!("{name}/"),
            if d == game.dir {
                Color::White
            } else {
                Color::Cyan
            },
            40,
        );
        s.text(
            end + 1,
            y,
            &format!("{} lines", group(dir.total_lines)),
            Color::Dim,
            16,
        );
        y += 1;
    }
    if total > 17 {
        s.text(
            3,
            y,
            &format!("… and {} more folders", total - 17),
            Color::Dim,
            40,
        );
    }
    s.text(
        2,
        22,
        "@ you   ☠ the boss   ✓ explored        any key to return",
        Color::Dim,
        62,
    );
}

fn ending(game: &Game, s: &mut Screen) {
    let (title, color) = match game.state {
        State::Won => ("VICTORY", Color::Gold),
        _ => ("YOU HAVE FALLEN", Color::Red),
    };
    let (x0, y0, w, h) = (8usize, 5usize, 50usize, 13usize);
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            let edge = y == y0 || y == y0 + h - 1 || x == x0 || x == x0 + w - 1;
            s.put(
                x,
                y,
                if edge { '█' } else { ' ' },
                if edge { color } else { Color::Default },
                Color::Default,
            );
        }
    }
    s.text(x0 + (w - title.len()) / 2, y0 + 1, title, color, w);
    let mut y = y0 + 3;
    for line in wrap(&game.summary(), w - 4) {
        s.text(x0 + 2, y, &line, Color::White, w - 4);
        y += 1;
    }
    y += 1;
    let stats = format!(
        "{} turns · {} kills · {} lines slain · {} floors",
        game.stats.turns,
        game.stats.kills,
        group(game.stats.lines_slain),
        game.stats.floors.len()
    );
    s.text(x0 + 2, y.min(y0 + h - 3), &stats, Color::Dim, w - 4);
    s.text(
        x0 + 2,
        y0 + h - 2,
        "r: delve again (new seed)   q: quit",
        Color::Dim,
        w - 4,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dungeon::tests_support::sample_repo;
    use crate::game::Game;

    #[test]
    fn draws_every_view() {
        let g = Game::new(sample_repo(), 2);
        let mut s = Screen::new();
        draw(&g, Overlay::None, &mut s);
        let text = s.to_text();
        assert!(text.contains('@'));
        assert!(text.contains("QUEST"));
        assert!(text.contains("`monolith`"));
        draw(&g, Overlay::Map, &mut s);
        assert!(s.to_text().contains("THE DUNGEON"));
        assert!(s.to_text().contains('☠'));
        draw(&g, Overlay::Help, &mut s);
        assert!(s.to_text().contains("HOW TO PLAY"));
    }

    #[test]
    fn wrapping() {
        assert_eq!(wrap("a bb ccc dddd", 6), vec!["a bb", "ccc", "dddd"]);
        assert_eq!(group(1234567), "1,234,567");
    }
}
