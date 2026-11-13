//! A simple bot that plays delve the way a careful person would: it heads
//! for the nearest monster, item or unexplored stairs, rests or drinks a
//! TODO when hurt, and climbs back up when a floor holds nothing more. Used
//! by the tests and by `delve bot` to check that a repository's dungeon is
//! fair.

use crate::analyze::NoteKind;
use crate::dungeon::{Floor, Pos, Tile};
use crate::game::{Action, Game, State, PACK_SIZE};
use std::collections::{HashMap, VecDeque};

const DIRS: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// Shortest paths from `from`: for each reachable tile, the first step.
/// Monsters and ghosts block the way except as the final step.
fn paths(floor: &Floor, from: Pos) -> HashMap<Pos, (i32, (i32, i32))> {
    let mut out: HashMap<Pos, (i32, (i32, i32))> = HashMap::new();
    let mut q = VecDeque::new();
    out.insert(from, (0, (0, 0)));
    q.push_back(from);
    while let Some(p) = q.pop_front() {
        let (d, first) = out[&p];
        if p != from && floor.occupied(p) {
            continue; // can attack it, not walk through it
        }
        for (dx, dy) in DIRS {
            let n = Pos::new(p.x + dx, p.y + dy);
            if floor.tile(n).walkable() && !out.contains_key(&n) {
                let step = if p == from { (dx, dy) } else { first };
                out.insert(n, (d + 1, step));
                q.push_back(n);
            }
        }
    }
    out
}

fn threatened(g: &Game) -> bool {
    let me = g.player.pos;
    g.floor()
        .monsters
        .iter()
        .any(|m| m.hp > 0 && (m.awake || m.pos.dist(me) <= 3) && m.pos.dist(me) <= 6)
}

/// One move for the bot.
pub fn choose(g: &Game) -> Action {
    let floor = g.floor();
    let me = g.player.pos;
    let hurt = g.player.hp * 2 < g.player.max_hp;
    if hurt {
        if let Some(i) = g
            .player
            .pack
            .iter()
            .position(|it| it.kind == Some(NoteKind::Todo))
        {
            return Action::Use(i);
        }
        if !threatened(g) && g.player.hp * 5 < g.player.max_hp * 4 {
            return Action::Wait;
        }
    }
    if g.player.hp * 5 < g.player.max_hp * 4 && !threatened(g) && g.player.hp < g.player.max_hp - 10
    {
        return Action::Wait;
    }
    if let Tile::Down(i) = floor.tile(me) {
        if !g.stats.floors.contains(&g.repo.dirs[g.dir].children[i]) {
            return Action::Descend;
        }
    }
    let reach = paths(floor, me);
    let mut targets: Vec<Pos> = floor
        .monsters
        .iter()
        .filter(|m| m.hp > 0)
        .map(|m| m.pos)
        .collect();
    if g.player.pack.len() < PACK_SIZE {
        targets.extend(floor.items.iter().map(|(p, _)| *p));
    } else {
        targets.extend(
            floor
                .items
                .iter()
                .filter(|(_, it)| it.kind.is_none() || it.kind == Some(NoteKind::Hack))
                .map(|(p, _)| *p),
        );
    }
    for (i, &child) in g.repo.dirs[g.dir].children.iter().enumerate() {
        if let Some(p) = floor.stairs_down(i) {
            if !g.stats.floors.contains(&child) || unfinished(g, child) {
                targets.push(p);
            }
        }
    }
    let best = targets
        .into_iter()
        .filter_map(|t| reach.get(&t).map(|&(d, step)| (d, step, t)))
        .filter(|&(d, _, _)| d > 0)
        .min_by_key(|&(d, _, t)| (d, t.y, t.x));
    if let Some((_, step, _)) = best {
        return Action::Move(step.0, step.1);
    }
    if let Tile::Down(i) = floor.tile(me) {
        let child = g.repo.dirs[g.dir].children[i];
        if unfinished(g, child) {
            return Action::Descend;
        }
    }
    // nothing left here: go back up
    if floor.tile(me) == Tile::Up {
        return Action::Ascend;
    }
    match floor.stairs_up().and_then(|up| reach.get(&up)) {
        Some(&(_, step)) => Action::Move(step.0, step.1),
        None => Action::Wait,
    }
}

/// Whether the boss's folder is at or below `dir` and not yet cleared.
fn unfinished(g: &Game, dir: usize) -> bool {
    let Some((fi, _)) = g.repo.boss else {
        return false;
    };
    let boss_dir = g.repo.dir_of_file(fi);
    g.repo.path_to(boss_dir).contains(&dir)
}

/// Play until the game ends or `max_turns` pass.
pub fn autoplay(mut g: Game, max_turns: u32) -> Game {
    let mut actions = 0;
    while g.state == State::Playing && g.stats.turns < max_turns && actions < max_turns * 2 {
        actions += 1;
        let a = choose(&g);
        g.act(a);
    }
    g
}
