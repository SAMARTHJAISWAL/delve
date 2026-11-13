//! The rules: moving, fighting, loot, levels, ghosts, winning and dying.

use crate::analyze::NoteKind;
use crate::dungeon::{generate, Floor, Item, Pos, Strength, Tile, MAP_H, MAP_W};
use crate::repo::Repo;
use crate::rng::{hash, Rng};
use std::collections::HashMap;

pub const PACK_SIZE: usize = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Move(i32, i32),
    Wait,
    Descend,
    Ascend,
    Use(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Playing,
    Won,
    Dead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Good,
    Bad,
    Loot,
    Ghost,
    Boss,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub text: String,
    pub tone: Tone,
    pub turn: u32,
}

#[derive(Clone, Debug)]
pub struct Player {
    pub pos: Pos,
    pub hp: i32,
    pub max_hp: i32,
    pub atk: i32,
    pub def: i32,
    pub level: u32,
    pub xp: u32,
    pub pack: Vec<Item>,
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub turns: u32,
    pub kills: u32,
    pub lines_slain: usize,
    pub floors: Vec<usize>,
    pub deepest: usize,
    pub items_used: u32,
    pub ghosts_met: u32,
    /// What killed you: (name, file path, line, length).
    pub killer: Option<(String, String, usize, usize)>,
}

pub struct Game {
    pub repo: Repo,
    pub seed: u64,
    pub floors: HashMap<usize, Floor>,
    pub dir: usize,
    pub player: Player,
    pub log: Vec<Message>,
    pub state: State,
    pub stats: Stats,
    strength: Strength,
    rng: Rng,
}

pub fn xp_for_level(level: u32) -> u32 {
    8 + level * level * 6
}

/// Every this many turns you heal one point.
const REGEN_EVERY: u32 = 6;

impl Game {
    pub fn new(repo: Repo, seed: u64) -> Game {
        let strength = Strength::new(&repo);
        let rng = Rng::new(seed ^ hash(&repo.name));
        let mut game = Game {
            repo,
            seed,
            floors: HashMap::new(),
            dir: 0,
            player: Player {
                pos: Pos::new(0, 0),
                hp: 40,
                max_hp: 40,
                atk: 4,
                def: 0,
                level: 1,
                xp: 0,
                pack: vec![],
            },
            log: vec![],
            state: State::Playing,
            stats: Stats::default(),
            strength,
            rng,
        };
        game.enter(0, None);
        let intro = match game.repo.boss_function() {
            Some((file, f)) => format!(
                "You descend into {}. Somewhere below lurks `{}`, {} lines long, in {}.",
                game.repo.name, f.name, f.len, file.path
            ),
            None => format!(
                "You descend into {}. It is strangely quiet: no functions live here.",
                game.repo.name
            ),
        };
        game.say(intro, Tone::Boss);
        game.say(
            "Arrows or hjkl to move, bump to fight, > to go down a folder. ? for help.",
            Tone::Plain,
        );
        game
    }

    /// Mark the whole current floor as seen (what a scroll of FIXME does).
    pub fn reveal(&mut self) {
        let floor = self.floor_mut();
        for y in 0..MAP_H {
            for x in 0..MAP_W {
                let p = Pos::new(x, y);
                if floor.tile(p) != Tile::Void {
                    floor.see(p);
                }
            }
        }
    }

    pub fn floor(&self) -> &Floor {
        &self.floors[&self.dir]
    }

    fn floor_mut(&mut self) -> &mut Floor {
        self.floors.get_mut(&self.dir).expect("current floor")
    }

    pub fn say(&mut self, text: impl Into<String>, tone: Tone) {
        self.log.push(Message {
            text: text.into(),
            tone,
            turn: self.stats.turns,
        });
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    /// Move to a floor. `from_child` is the folder we climbed out of, so we
    /// arrive on its stairs.
    fn enter(&mut self, dir: usize, from_child: Option<usize>) {
        if !self.floors.contains_key(&dir) {
            let floor = generate(&self.repo, dir, self.seed, &self.strength);
            self.floors.insert(dir, floor);
        }
        self.dir = dir;
        let floor = &self.floors[&dir];
        self.player.pos = match from_child
            .and_then(|c| self.repo.dirs[dir].children.iter().position(|&x| x == c))
        {
            Some(i) => floor.stairs_down(i).unwrap_or(floor.entry),
            None => floor.entry,
        };
        if !self.stats.floors.contains(&dir) {
            self.stats.floors.push(dir);
        }
        self.stats.deepest = self.stats.deepest.max(self.repo.dirs[dir].depth);
        self.look();
    }

    /// Mark what the player can see: the room they are in, and a step around.
    fn look(&mut self) {
        let pos = self.player.pos;
        let floor = self.floor_mut();
        if let Some(r) = floor.room_at(pos) {
            let room = floor.rooms[r].clone();
            for y in room.y..room.y + room.h {
                for x in room.x..room.x + room.w {
                    floor.see(Pos::new(x, y));
                }
            }
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                floor.see(Pos::new(pos.x + dx, pos.y + dy));
            }
        }
    }

    /// Whether the player can see a spot right now (for drawing monsters).
    pub fn visible(&self, p: Pos) -> bool {
        let floor = self.floor();
        let here = floor.room_at(self.player.pos);
        p.dist(self.player.pos) <= 1 || here.is_some_and(|r| floor.rooms[r].covers(p))
    }

    pub fn act(&mut self, action: Action) {
        if self.state != State::Playing {
            return;
        }
        let took_turn = match action {
            Action::Move(dx, dy) => self.step(dx, dy),
            Action::Wait => true,
            Action::Descend => self.descend(),
            Action::Ascend => self.ascend(),
            Action::Use(i) => self.use_item(i),
        };
        if took_turn && self.state == State::Playing {
            self.stats.turns += 1;
            if self.stats.turns.is_multiple_of(REGEN_EVERY) && self.player.hp < self.player.max_hp {
                self.player.hp += 1;
            }
            self.monsters_turn();
            self.look();
        }
    }

    fn step(&mut self, dx: i32, dy: i32) -> bool {
        let to = Pos::new(self.player.pos.x + dx, self.player.pos.y + dy);
        if let Some(m) = self.floor().monster_at(to) {
            self.attack(m);
            return true;
        }
        if let Some(g) = self.floor().ghost_at(to) {
            self.talk(g);
            return true;
        }
        let tile = self.floor().tile(to);
        if !tile.walkable() {
            return false;
        }
        self.player.pos = to;
        self.pick_up();
        match tile {
            Tile::Down(i) => {
                let child = self.repo.dirs[self.dir].children[i];
                let label = self.repo.dirs[child].label().to_string();
                self.say(
                    format!("Stairs down into {label}/. Press > to descend."),
                    Tone::Plain,
                );
            }
            Tile::Up => {
                if let Some(p) = self.repo.dirs[self.dir].parent {
                    let label = self.repo.dirs[p].label().to_string();
                    self.say(
                        format!("Stairs up to {label}. Press < to climb."),
                        Tone::Plain,
                    );
                }
            }
            _ => {}
        }
        // entering a room: say whose file it is
        let floor = self.floor();
        if let (Some(r), true) = (
            floor.room_at(to),
            floor.room_at(self.player.pos.offset(-dx, -dy)).is_none(),
        ) {
            if let Some(fi) = floor.rooms[r].file {
                let f = &self.repo.files[fi];
                let text = format!(
                    "You enter {} — {} lines of {}, {} functions.",
                    f.path,
                    f.lines,
                    f.lang.name(),
                    f.functions.len()
                );
                self.say(text, Tone::Plain);
            }
        }
        true
    }

    fn descend(&mut self) -> bool {
        match self.floor().tile(self.player.pos) {
            Tile::Down(i) => {
                let child = self.repo.dirs[self.dir].children[i];
                self.enter(child, None);
                let d = &self.repo.dirs[child];
                let text = format!(
                    "You descend into {}/ ({} lines of code).",
                    d.label(),
                    d.total_lines
                );
                self.say(text, Tone::Plain);
                true
            }
            _ => {
                self.say("There are no stairs down here.", Tone::Plain);
                false
            }
        }
    }

    fn ascend(&mut self) -> bool {
        match (
            self.floor().tile(self.player.pos),
            self.repo.dirs[self.dir].parent,
        ) {
            (Tile::Up, Some(parent)) => {
                let from = self.dir;
                self.enter(parent, Some(from));
                let text = format!("You climb back up to {}.", self.repo.dirs[parent].label());
                self.say(text, Tone::Plain);
                true
            }
            _ => {
                self.say("There are no stairs up here.", Tone::Plain);
                false
            }
        }
    }

    fn roll(&mut self, atk: i32, def: i32) -> i32 {
        (atk + self.rng.range(0, 2) - def).max(1)
    }

    fn attack(&mut self, m: usize) {
        let (atk, def) = (self.player.atk, self.floor().monsters[m].def);
        let dmg = self.roll(atk, def);
        let floor = self.floor_mut();
        let mon = &mut floor.monsters[m];
        mon.hp -= dmg;
        mon.awake = true;
        let label = format!("{} `{}`", mon.creature, mon.name);
        if mon.hp > 0 {
            let text = format!("You hit {label} for {dmg} ({}/{}).", mon.hp, mon.max_hp);
            self.say(text, Tone::Plain);
            return;
        }
        let mon = mon.clone();
        let path = self.repo.files[mon.file].path.clone();
        self.stats.kills += 1;
        self.stats.lines_slain += mon.len;
        if mon.boss {
            self.say(
                format!(
                    "You defeat `{}` — {} lines at {}:{}. The repository is yours!",
                    mon.name, mon.len, path, mon.line
                ),
                Tone::Boss,
            );
            self.state = State::Won;
            return;
        }
        self.say(
            format!(
                "You refactor {label} ({}:{}, {} lines) out of existence. +{} XP",
                path, mon.line, mon.len, mon.xp
            ),
            Tone::Good,
        );
        self.gain(mon.xp);
    }

    fn gain(&mut self, xp: u32) {
        self.player.xp += xp;
        while self.player.xp >= xp_for_level(self.player.level) {
            self.player.xp -= xp_for_level(self.player.level);
            self.player.level += 1;
            self.player.max_hp += 8;
            self.player.hp = (self.player.hp + self.player.max_hp / 2).min(self.player.max_hp);
            self.player.atk += 1;
            if self.player.level.is_multiple_of(3) {
                self.player.def += 1;
            }
            let text = format!(
                "You reach level {}. Code review has made you stronger.",
                self.player.level
            );
            self.say(text, Tone::Good);
        }
    }

    fn talk(&mut self, g: usize) {
        let ghost = &mut self.floor_mut().ghosts[g];
        let first = ghost.said == 0;
        let subject = if ghost.subjects.is_empty() {
            "...".to_string()
        } else {
            ghost.subjects[ghost.said % ghost.subjects.len()].clone()
        };
        ghost.said += 1;
        let author = ghost.author.clone();
        let commits = ghost.subjects.len();
        self.say(
            format!("The ghost of {author} whispers: \"{subject}\""),
            Tone::Ghost,
        );
        if first {
            self.stats.ghosts_met += 1;
            self.say(
                format!("{commits} commits of wisdom pass to you. +5 XP"),
                Tone::Ghost,
            );
            self.gain(5);
        }
    }

    fn pick_up(&mut self) {
        let pos = self.player.pos;
        let Some(i) = self.floor().items.iter().position(|(p, _)| *p == pos) else {
            return;
        };
        let item = self.floor().items[i].1.clone();
        if item.kind.is_none() {
            self.floor_mut().items.remove(i);
            let heal = 6 + self.player.level as i32;
            self.player.hp = (self.player.hp + heal).min(self.player.max_hp);
            self.say(
                format!("You find a cup of coffee and drink it on the spot. (+{heal} HP)"),
                Tone::Loot,
            );
            return;
        }
        let origin = format!("{}:{}", self.repo.files[item.file].path, item.line);
        let quote = if item.text.is_empty() {
            String::new()
        } else {
            format!(" \"{}\"", item.text)
        };
        if item.kind == Some(NoteKind::Hack) {
            self.floor_mut().items.remove(i);
            self.player.atk += 2;
            self.say(
                format!(
                    "You wield a hacky blade from {origin}.{quote} It works; don't ask how. ATK +2"
                ),
                Tone::Loot,
            );
            return;
        }
        if self.player.pack.len() >= PACK_SIZE {
            self.say(
                format!(
                    "Your pack is full; you leave the {} behind.",
                    item_name(item.kind)
                ),
                Tone::Plain,
            );
            return;
        }
        self.floor_mut().items.remove(i);
        self.say(
            format!(
                "You pick up a {} from {origin}.{quote}",
                item_name(item.kind)
            ),
            Tone::Loot,
        );
        self.player.pack.push(item);
    }

    fn use_item(&mut self, i: usize) -> bool {
        if i >= self.player.pack.len() {
            return false;
        }
        let item = self.player.pack.remove(i);
        self.stats.items_used += 1;
        let Some(kind) = item.kind else { return false };
        match kind {
            NoteKind::Todo => {
                let heal = 10 + self.player.level as i32 * 2;
                self.player.hp = (self.player.hp + heal).min(self.player.max_hp);
                self.say(
                    format!("You finally get to that TODO. You feel better. (+{heal} HP)"),
                    Tone::Good,
                );
            }
            NoteKind::Fixme => {
                self.reveal();
                self.say(
                    "You read the FIXME. The whole floor's layout is revealed.",
                    Tone::Good,
                );
            }
            NoteKind::Xxx => {
                let pos = self.player.pos;
                let dmg = 8 + self.player.level as i32 * 2;
                let mut hits = vec![];
                for m in self.floor_mut().monsters.iter_mut() {
                    if m.hp > 0 && m.pos.dist(pos) <= 4 {
                        m.hp -= dmg;
                        m.awake = true;
                        hits.push(m.clone());
                    }
                }
                self.say(
                    format!(
                        "The XXX goes off! {} monsters caught in the blast.",
                        hits.len()
                    ),
                    Tone::Good,
                );
                for m in hits.into_iter().filter(|m| m.hp <= 0) {
                    self.stats.kills += 1;
                    self.stats.lines_slain += m.len;
                    if m.boss {
                        self.say(
                            format!("`{}` falls to the blast. The repository is yours!", m.name),
                            Tone::Boss,
                        );
                        self.state = State::Won;
                        return true;
                    }
                    self.say(
                        format!("{} `{}` is blown away. +{} XP", m.creature, m.name, m.xp),
                        Tone::Good,
                    );
                    self.gain(m.xp);
                }
            }
            NoteKind::Bug => match self.rng.below(10) {
                0..=4 => {
                    self.player.hp = (self.player.hp + 8).min(self.player.max_hp);
                    self.say(
                        "You open the jar. The bug turns out to be a feature. (+8 HP)",
                        Tone::Good,
                    );
                }
                5..=7 => {
                    self.player.hp -= 4;
                    self.say("You open the jar. The bug bites! (-4 HP)", Tone::Bad);
                    if self.player.hp <= 0 {
                        self.stats.killer = Some((
                            "a bug in a jar".into(),
                            self.repo.files[item.file].path.clone(),
                            item.line,
                            1,
                        ));
                        self.state = State::Dead;
                    }
                }
                _ => {
                    let rooms = self.floor().rooms.len();
                    let r = self.rng.below(rooms);
                    let c = self.floor().rooms[r].center();
                    if self.floor().tile(c) == Tile::Floor && !self.floor().occupied(c) {
                        self.player.pos = c;
                    }
                    self.say(
                        "You open the jar. Reality glitches and you are somewhere else.",
                        Tone::Plain,
                    );
                }
            },
            NoteKind::Hack => {}
        }
        true
    }

    fn monsters_turn(&mut self) {
        let player = self.player.pos;
        let here = self.floor().room_at(player);
        for m in 0..self.floor().monsters.len() {
            let mon = self.floor().monsters[m].clone();
            if mon.hp <= 0 {
                continue;
            }
            let same_room = here.is_some_and(|r| self.floor().rooms[r].contains(mon.pos));
            let near = mon.pos.dist(player);
            let awake = mon.awake || (same_room && near <= 5) || near <= 2;
            if !awake || near > 12 {
                continue;
            }
            self.floor_mut().monsters[m].awake = true;
            if near <= 1 {
                let dmg = self.roll(mon.atk, self.player.def);
                self.player.hp -= dmg;
                let what = if mon.boss {
                    "unleashes its full length on you"
                } else {
                    "hits you"
                };
                self.say(
                    format!("{} `{}` {what} for {dmg}.", mon.creature, mon.name,),
                    Tone::Bad,
                );
                if self.player.hp <= 0 {
                    let path = self.repo.files[mon.file].path.clone();
                    self.stats.killer = Some((mon.name.clone(), path, mon.line, mon.len));
                    self.say("You have been defeated. Press r to delve again.", Tone::Bad);
                    self.state = State::Dead;
                    return;
                }
                continue;
            }
            // step toward the player
            let sx = (player.x - mon.pos.x).signum();
            let sy = (player.y - mon.pos.y).signum();
            for (dx, dy) in [(sx, sy), (sx, 0), (0, sy)] {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let to = Pos::new(mon.pos.x + dx, mon.pos.y + dy);
                if self.floor().tile(to).walkable() && !self.floor().occupied(to) && to != player {
                    self.floor_mut().monsters[m].pos = to;
                    break;
                }
            }
        }
        self.floor_mut().monsters.retain(|m| m.hp > 0);
    }

    /// The quest line: where the boss is, and whether you are on its floor.
    pub fn quest(&self) -> Option<(String, usize, String)> {
        let (fi, gi) = self.repo.boss?;
        let f = &self.repo.files[fi];
        Some((
            f.functions[gi].name.clone(),
            f.functions[gi].len,
            f.path.clone(),
        ))
    }

    /// A one-line summary of the run, for sharing.
    pub fn summary(&self) -> String {
        let floors = self.stats.floors.len();
        match self.state {
            State::Won => {
                let (name, len, _) = self.quest().unwrap_or_default();
                format!(
                    "I delved into {} and defeated `{}` ({} lines) at level {} in {} turns, refactoring {} functions on the way.",
                    self.repo.name, name, len, self.player.level, self.stats.turns, self.stats.kills
                )
            }
            State::Dead => {
                let (name, path, _, len) = self.stats.killer.clone().unwrap_or_default();
                format!(
                    "I delved into {}, explored {} floor{} and fell to `{}` ({} lines, {}) at level {}.",
                    self.repo.name,
                    floors,
                    if floors == 1 { "" } else { "s" },
                    name,
                    len,
                    path,
                    self.player.level
                )
            }
            State::Playing => format!(
                "I'm delving into {}: level {}, {} functions refactored so far.",
                self.repo.name, self.player.level, self.stats.kills
            ),
        }
    }
}

trait Offset {
    fn offset(self, dx: i32, dy: i32) -> Pos;
}

impl Offset for Pos {
    fn offset(self, dx: i32, dy: i32) -> Pos {
        Pos::new(self.x + dx, self.y + dy)
    }
}

pub fn item_name(kind: Option<NoteKind>) -> &'static str {
    let Some(kind) = kind else {
        return "cup of coffee";
    };
    match kind {
        NoteKind::Todo => "potion of TODO",
        NoteKind::Fixme => "scroll of FIXME",
        NoteKind::Hack => "hacky blade",
        NoteKind::Xxx => "XXX bomb",
        NoteKind::Bug => "bug in a jar",
    }
}

pub fn item_glyph(kind: Option<NoteKind>) -> char {
    let Some(kind) = kind else { return '!' };
    match kind {
        NoteKind::Todo => '!',
        NoteKind::Fixme => '?',
        NoteKind::Hack => '/',
        NoteKind::Xxx => '*',
        NoteKind::Bug => '%',
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::autoplay;
    use crate::dungeon::tests_support::sample_repo;

    #[test]
    fn a_bot_can_play_to_an_end() {
        let mut outcomes = vec![];
        for seed in 0..12 {
            let g = autoplay(Game::new(sample_repo(), seed), 3000);
            assert!(g.player.hp <= g.player.max_hp);
            assert!(g.stats.turns > 0);
            outcomes.push(g.state);
        }
        // the game must be winnable and losable
        assert!(outcomes.contains(&State::Won) || outcomes.contains(&State::Dead));
    }

    #[test]
    fn same_seed_same_game() {
        let a = autoplay(Game::new(sample_repo(), 5), 400);
        let b = autoplay(Game::new(sample_repo(), 5), 400);
        assert_eq!(a.stats.turns, b.stats.turns);
        assert_eq!(a.player.hp, b.player.hp);
        assert_eq!(a.log.len(), b.log.len());
    }

    #[test]
    fn walls_block_and_stairs_work() {
        let mut g = Game::new(sample_repo(), 1);
        let start = g.player.pos;
        // walk into whatever is around until something blocks; position must stay walkable
        for _ in 0..30 {
            g.act(Action::Move(-1, 0));
        }
        assert!(g.floor().tile(g.player.pos).walkable());
        let _ = start;
        g.act(Action::Descend);
        assert!(g
            .log
            .iter()
            .any(|m| m.text.contains("no stairs down") || m.text.contains("descend")));
    }
}
