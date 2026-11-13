//! `delve`: play the repository you are standing in.

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{self, Color as TermColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use delve_core::analyze::NoteKind;
use delve_core::rng::hash;
use delve_core::screen::{Color, HEIGHT, WIDTH};
use delve_core::{draw, Action, Commit, Game, Overlay, Repo, Screen, State};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "delve — play any codebase as a roguelike

Usage:
  delve [PATH]              play the repository at PATH (default: here)
  delve stats [PATH]        what the dungeon would hold, without playing
  delve print [PATH]        draw the first screen to stdout and exit
  delve bot [PATH]          let a bot play 20 runs and report how it went

Options:
  --seed N      play a specific dungeon (same repo + seed = same game)
  --daily       today's seed, the same for everyone
  -h, --help    show this help
  -V, --version show the version

Folders are floors, files are rooms, functions are monsters (longer means
tougher) and the longest function in the repository is the boss.
TODO, FIXME, HACK, XXX and BUG comments are loot; committers are ghosts.
";

struct Options {
    command: String,
    path: PathBuf,
    seed: Option<u64>,
}

fn parse_args() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let mut command = "play".to_string();
    let mut path: Option<PathBuf> = None;
    let mut seed = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("delve {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--seed" => {
                let v = args.next().ok_or("--seed needs a number")?;
                seed = Some(v.parse::<u64>().map_err(|_| format!("not a seed: {v}"))?);
            }
            "--daily" => {
                let days = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() / 86_400)
                    .unwrap_or(0);
                seed = Some(days);
            }
            "stats" | "print" | "play" | "bot" if path.is_none() && command == "play" => {
                command = a
            }
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => {
                if path.is_some() {
                    return Err(format!("unexpected argument {a}"));
                }
                path = Some(PathBuf::from(a));
            }
        }
    }
    Ok(Options {
        command,
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        seed,
    })
}

// ---------------------------------------------------------------------------
// Reading the repository
// ---------------------------------------------------------------------------

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        None
    }
}

const MAX_FILE_BYTES: u64 = 512 * 1024;

fn list_files(root: &Path) -> Vec<String> {
    if let Some(out) = git(
        root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    ) {
        return out
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
    }
    // not a git repository: walk the folder
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if delve_core::repo::ignored(&rel) {
                continue;
            }
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(path),
                Ok(t) if t.is_file() => out.push(rel),
                _ => {}
            }
        }
    }
    out
}

fn read_sources(root: &Path) -> Vec<(String, String)> {
    list_files(root)
        .into_iter()
        .filter(|p| delve_core::lang::Lang::from_path(p).is_some() && !delve_core::repo::ignored(p))
        .filter_map(|p| {
            let full = root.join(&p);
            let meta = std::fs::metadata(&full).ok()?;
            if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
                return None;
            }
            let bytes = std::fs::read(&full).ok()?;
            if bytes.contains(&0) {
                return None;
            }
            Some((p, String::from_utf8_lossy(&bytes).into_owned()))
        })
        .collect()
}

fn read_commits(root: &Path) -> Vec<Commit> {
    let Some(out) = git(
        root,
        &[
            "log",
            "-n",
            "4000",
            "--no-merges",
            "--format=%x1e%an%x1f%s",
            "--name-only",
        ],
    ) else {
        return vec![];
    };
    out.split('\u{1e}')
        .filter_map(|record| {
            let mut lines = record.lines();
            let head = lines.next()?;
            let (author, subject) = head.split_once('\u{1f}')?;
            let files = lines
                .filter(|l| !l.trim().is_empty())
                .map(str::to_string)
                .collect();
            Some(Commit {
                author: author.to_string(),
                subject: subject.to_string(),
                files,
            })
        })
        .collect()
}

fn repo_name(root: &Path) -> String {
    if let Some(url) = git(root, &["remote", "get-url", "origin"]) {
        let url = url.trim().trim_end_matches(".git").trim_end_matches('/');
        let parts: Vec<&str> = url.rsplit(['/', ':']).take(2).collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return format!("{}/{}", parts[1], parts[0]);
        }
    }
    let top = git(root, &["rev-parse", "--show-toplevel"]).map(|s| PathBuf::from(s.trim()));
    let dir = top.unwrap_or_else(|| root.canonicalize().unwrap_or_else(|_| root.to_path_buf()));
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".into())
}

fn load(path: &Path) -> Result<Repo, String> {
    if !path.is_dir() {
        return Err(format!("{} is not a folder", path.display()));
    }
    // play the whole repository even when started in a subfolder
    let root = git(path, &["rev-parse", "--show-toplevel"])
        .map(|s| PathBuf::from(s.trim()))
        .unwrap_or_else(|| path.to_path_buf());
    let sources = read_sources(&root);
    let repo = Repo::from_sources(&repo_name(&root), sources, read_commits(&root));
    if repo.files.is_empty() {
        return Err(format!("no source code found in {}", root.display()));
    }
    Ok(repo)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn stats(repo: &Repo) {
    let notes: usize = repo.files.iter().map(|f| f.notes.len()).sum();
    let count = |k: NoteKind| {
        repo.files
            .iter()
            .flat_map(|f| &f.notes)
            .filter(|n| n.kind == k)
            .count()
    };
    let mut authors: Vec<&str> = repo.commits.iter().map(|c| c.author.as_str()).collect();
    authors.sort_unstable();
    authors.dedup();
    println!("{}", repo.name);
    println!(
        "  floors    {} folders, {} deep",
        repo.dirs.len(),
        repo.dirs.iter().map(|d| d.depth).max().unwrap_or(0) + 1
    );
    println!(
        "  rooms     {} source files, {} lines",
        repo.files.len(),
        repo.total_lines()
    );
    println!("  monsters  {} functions", repo.function_count());
    println!(
        "  loot      {} notes: {} TODO, {} FIXME, {} HACK, {} XXX, {} BUG",
        notes,
        count(NoteKind::Todo),
        count(NoteKind::Fixme),
        count(NoteKind::Hack),
        count(NoteKind::Xxx),
        count(NoteKind::Bug)
    );
    println!("  ghosts    {} committers", authors.len());
    match repo.boss_function() {
        Some((file, f)) => println!(
            "  boss      `{}`, {} lines, {}:{}",
            f.name, f.len, file.path, f.line
        ),
        None => println!("  boss      none"),
    }
    let mut biggest: Vec<(&str, &str, usize)> = repo
        .files
        .iter()
        .flat_map(|file| {
            file.functions
                .iter()
                .map(move |f| (f.name.as_str(), file.path.as_str(), f.len))
        })
        .collect();
    biggest.sort_by_key(|b| std::cmp::Reverse(b.2));
    if biggest.len() > 1 {
        println!(
            "  elites    {}",
            biggest[1..biggest.len().min(4)]
                .iter()
                .map(|(n, _, l)| format!("`{n}` ({l})"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

fn color(c: Color) -> TermColor {
    match c {
        Color::Default => TermColor::Reset,
        Color::Dim => TermColor::Rgb {
            r: 110,
            g: 110,
            b: 120,
        },
        Color::Wall => TermColor::Rgb {
            r: 95,
            g: 90,
            b: 110,
        },
        Color::Floor => TermColor::Rgb {
            r: 70,
            g: 70,
            b: 80,
        },
        Color::Player => TermColor::Rgb {
            r: 255,
            g: 255,
            b: 255,
        },
        Color::Red => TermColor::Rgb {
            r: 235,
            g: 95,
            b: 85,
        },
        Color::Green => TermColor::Rgb {
            r: 120,
            g: 210,
            b: 120,
        },
        Color::Yellow => TermColor::Rgb {
            r: 220,
            g: 180,
            b: 90,
        },
        Color::Blue => TermColor::Rgb {
            r: 110,
            g: 150,
            b: 245,
        },
        Color::Magenta => TermColor::Rgb {
            r: 200,
            g: 140,
            b: 255,
        },
        Color::Cyan => TermColor::Rgb {
            r: 110,
            g: 200,
            b: 220,
        },
        Color::White => TermColor::Rgb {
            r: 225,
            g: 225,
            b: 230,
        },
        Color::Boss => TermColor::Rgb {
            r: 255,
            g: 70,
            b: 110,
        },
        Color::Bar => TermColor::Rgb {
            r: 30,
            g: 30,
            b: 40,
        },
        Color::Gold => TermColor::Rgb {
            r: 245,
            g: 200,
            b: 80,
        },
    }
}

fn paint(out: &mut impl Write, screen: &Screen) -> io::Result<()> {
    let mut fg = None;
    let mut bg = None;
    for y in 0..HEIGHT {
        queue!(out, cursor::MoveTo(0, y as u16))?;
        for x in 0..WIDTH {
            let c = screen.cells[y * WIDTH + x];
            if fg != Some(c.fg) {
                queue!(out, SetForegroundColor(color(c.fg)))?;
                fg = Some(c.fg);
            }
            if bg != Some(c.bg) {
                queue!(out, SetBackgroundColor(color(c.bg)))?;
                bg = Some(c.bg);
            }
            queue!(out, style::Print(c.ch))?;
        }
    }
    queue!(out, style::ResetColor)?;
    out.flush()
}

fn key_action(key: KeyEvent) -> Option<Action> {
    let KeyEvent {
        code, modifiers, ..
    } = key;
    Some(match code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('w') => Action::Move(0, -1),
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('s') => Action::Move(0, 1),
        KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('a') => Action::Move(-1, 0),
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('d') => Action::Move(1, 0),
        KeyCode::Char('y') => Action::Move(-1, -1),
        KeyCode::Char('u') => Action::Move(1, -1),
        KeyCode::Char('b') => Action::Move(-1, 1),
        KeyCode::Char('n') => Action::Move(1, 1),
        KeyCode::Char('.') | KeyCode::Char(' ') => Action::Wait,
        KeyCode::Char('>') => Action::Descend,
        KeyCode::Char('<') => Action::Ascend,
        KeyCode::Char(c @ '1'..='9') if !modifiers.contains(KeyModifiers::CONTROL) => {
            Action::Use(c as usize - '1' as usize)
        }
        _ => return None,
    })
}

fn play(repo: Repo, seed: u64) -> io::Result<String> {
    let mut out = io::stdout();
    terminal::enable_raw_mode()?;
    execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), terminal::LeaveAlternateScreen, cursor::Show);
        default_hook(info);
    }));

    let mut seed = seed;
    let mut game = Game::new(repo.clone(), seed);
    let mut overlay = Overlay::None;
    let mut screen = Screen::new();
    let result = loop {
        let (cols, rows) = terminal::size()?;
        if (cols as usize) < WIDTH || (rows as usize) < HEIGHT {
            queue!(out, terminal::Clear(ClearType::All), cursor::MoveTo(0, 0))?;
            queue!(out, style::Print(format!("delve needs a {WIDTH}x{HEIGHT} terminal (this one is {cols}x{rows}). Enlarge it, or press q.")))?;
            out.flush()?;
        } else {
            draw(&game, overlay, &mut screen);
            paint(&mut out, &screen)?;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            break game.summary();
        }
        if overlay != Overlay::None {
            overlay = Overlay::None;
            continue;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => break game.summary(),
            KeyCode::Char('?') => overlay = Overlay::Help,
            KeyCode::Char('m') => overlay = Overlay::Map,
            KeyCode::Char('r') if game.state != State::Playing => {
                seed = seed.wrapping_add(1);
                game = Game::new(repo.clone(), seed);
            }
            _ => {
                if let Some(action) = key_action(key) {
                    game.act(action);
                }
            }
        }
    };
    execute!(out, terminal::LeaveAlternateScreen, cursor::Show)?;
    terminal::disable_raw_mode()?;
    Ok(result)
}

fn main() {
    let options = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("delve: {e}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    let repo = match load(&options.path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("delve: {e}");
            std::process::exit(1);
        }
    };
    let seed = options.seed.unwrap_or_else(|| hash(&repo.name));
    match options.command.as_str() {
        "stats" => stats(&repo),
        "bot" => {
            let runs = 20;
            let (mut won, mut dead, mut turns, mut levels) = (0, 0, 0u32, 0u32);
            let mut killers: Vec<String> = vec![];
            let mut best: Option<(u32, String)> = None;
            for i in 0..runs {
                let g =
                    delve_core::bot::autoplay(Game::new(repo.clone(), seed.wrapping_add(i)), 5000);
                match g.state {
                    State::Won => {
                        won += 1;
                        if best.as_ref().is_none_or(|b| g.stats.turns < b.0) {
                            best = Some((g.stats.turns, g.summary()));
                        }
                    }
                    State::Dead => {
                        dead += 1;
                        if let Some((name, _, _, _)) = &g.stats.killer {
                            killers.push(name.clone());
                        }
                    }
                    State::Playing => {}
                }
                turns += g.stats.turns;
                levels += g.player.level;
            }
            killers.sort();
            let mut common: Vec<(usize, String)> = vec![];
            for k in killers.iter() {
                match common.iter_mut().find(|c| &c.1 == k) {
                    Some(c) => c.0 += 1,
                    None => common.push((1, k.clone())),
                }
            }
            common.sort_by_key(|c| std::cmp::Reverse(c.0));
            println!("{}: {runs} bot runs", repo.name);
            println!(
                "  won {won}, died {dead}, unfinished {}",
                runs as u32 - won - dead
            );
            println!(
                "  average {} turns, level {}",
                turns / runs as u32,
                levels / runs as u32
            );
            if let Some((_, summary)) = &best {
                println!("  fastest win: {summary}");
            }
            if !common.is_empty() {
                println!(
                    "  deadliest: {}",
                    common
                        .iter()
                        .take(3)
                        .map(|(n, k)| format!("`{k}` ({n})"))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
        "print" => {
            let game = Game::new(repo, seed);
            let mut screen = Screen::new();
            draw(&game, Overlay::None, &mut screen);
            println!("{}", screen.to_text());
        }
        _ => match play(repo, seed) {
            Ok(summary) => {
                println!("{summary}");
                println!("Play yours: https://github.com/SAMARTHJAISWAL/delve");
            }
            Err(e) => {
                eprintln!("delve: {e}");
                std::process::exit(1);
            }
        },
    }
}
