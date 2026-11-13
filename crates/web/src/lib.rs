//! The browser build: delve-core compiled to WebAssembly behind a tiny C ABI.
//! JavaScript fetches a repository, hands over its files, sends key presses
//! and copies the screen grid onto a canvas.

use delve_core::screen::{HEIGHT, WIDTH};
use delve_core::{draw, Action, Commit, Game, Overlay, Repo, Screen, State};
use std::cell::RefCell;

struct App {
    repo: Repo,
    game: Game,
    seed: u64,
    overlay: Overlay,
    screen: Screen,
    chars: Vec<u32>,
    colors: Vec<u8>,
    text: Vec<u8>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Make room for `len` bytes of input and return where to write them.
#[no_mangle]
pub extern "C" fn input_buffer(len: usize) -> *mut u8 {
    INPUT.with(|b| {
        let mut b = b.borrow_mut();
        b.clear();
        b.resize(len, 0);
        b.as_mut_ptr()
    })
}

/// Parse the input buffer: records separated by NUL. The first record is
/// the repository name; then `F<US>path<US>contents` for files and
/// `C<US>author<US>subject` for commits (US is the 0x1F unit separator).
fn parse_input(bytes: &[u8]) -> Repo {
    let text = String::from_utf8_lossy(bytes);
    let mut records = text.split('\0');
    let name = records.next().unwrap_or("repo").to_string();
    let mut files = vec![];
    let mut commits = vec![];
    for r in records {
        let mut parts = r.splitn(3, '\u{1f}');
        match (parts.next(), parts.next(), parts.next()) {
            (Some("F"), Some(path), Some(content)) => {
                files.push((path.to_string(), content.to_string()))
            }
            (Some("C"), Some(author), Some(subject)) => commits.push(Commit {
                author: author.to_string(),
                subject: subject.to_string(),
                files: vec![],
            }),
            _ => {}
        }
    }
    Repo::from_sources(&name, files, commits)
}

/// Start a game from the input buffer. Returns how many source files were
/// understood (0 means there is nothing to play).
#[no_mangle]
pub extern "C" fn start(seed_hi: u32, seed_lo: u32) -> u32 {
    let repo = INPUT.with(|b| parse_input(&b.borrow()));
    let count = repo.files.len() as u32;
    if count == 0 {
        return 0;
    }
    let seed = ((seed_hi as u64) << 32) | seed_lo as u64;
    let game = Game::new(repo.clone(), seed);
    APP.with(|a| {
        *a.borrow_mut() = Some(App {
            repo,
            game,
            seed,
            overlay: Overlay::None,
            screen: Screen::new(),
            chars: vec![0; WIDTH * HEIGHT],
            colors: vec![0; WIDTH * HEIGHT * 2],
            text: vec![],
        })
    });
    render();
    count
}

/// Key codes: 1-8 directions (N, S, W, E, NW, NE, SW, SE), 9 wait,
/// 10 descend, 11 ascend, 21-29 use pack slot 1-9, 30 help, 31 map,
/// 32 play again, 0 any other key (closes overlays).
#[no_mangle]
pub extern "C" fn key(code: u32) {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        let Some(app) = a.as_mut() else { return };
        if app.overlay != Overlay::None {
            app.overlay = Overlay::None;
            return;
        }
        let action = match code {
            1 => Some(Action::Move(0, -1)),
            2 => Some(Action::Move(0, 1)),
            3 => Some(Action::Move(-1, 0)),
            4 => Some(Action::Move(1, 0)),
            5 => Some(Action::Move(-1, -1)),
            6 => Some(Action::Move(1, -1)),
            7 => Some(Action::Move(-1, 1)),
            8 => Some(Action::Move(1, 1)),
            9 => Some(Action::Wait),
            10 => Some(Action::Descend),
            11 => Some(Action::Ascend),
            21..=29 => Some(Action::Use(code as usize - 21)),
            30 => {
                app.overlay = Overlay::Help;
                None
            }
            31 => {
                app.overlay = Overlay::Map;
                None
            }
            32 if app.game.state != State::Playing => {
                app.seed = app.seed.wrapping_add(1);
                app.game = Game::new(app.repo.clone(), app.seed);
                None
            }
            _ => None,
        };
        if let Some(action) = action {
            app.game.act(action);
        }
    });
    render();
}

/// Let the built-in bot make one move (the page's "watch a bot play").
#[no_mangle]
pub extern "C" fn bot_step() {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        let Some(app) = a.as_mut() else { return };
        app.overlay = Overlay::None;
        if app.game.state == State::Playing {
            let action = delve_core::bot::choose(&app.game);
            app.game.act(action);
        }
    });
    render();
}

/// Reveal the current floor (used to render the preview image) and return
/// the folder depth, so a script can wait for a deeper floor.
#[no_mangle]
pub extern "C" fn reveal() -> u32 {
    let depth = APP.with(|a| {
        let mut a = a.borrow_mut();
        let Some(app) = a.as_mut() else { return 0 };
        app.game.reveal();
        app.repo.dirs[app.game.dir].depth as u32
    });
    render();
    depth
}

/// Redraw into the shared buffers.
#[no_mangle]
pub extern "C" fn render() {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        let Some(app) = a.as_mut() else { return };
        draw(&app.game, app.overlay, &mut app.screen);
        for (i, c) in app.screen.cells.iter().enumerate() {
            app.chars[i] = c.ch as u32;
            app.colors[i * 2] = c.fg as u8;
            app.colors[i * 2 + 1] = c.bg as u8;
        }
    });
}

#[no_mangle]
pub extern "C" fn width() -> u32 {
    WIDTH as u32
}

#[no_mangle]
pub extern "C" fn height() -> u32 {
    HEIGHT as u32
}

/// Code points of the screen, row by row.
#[no_mangle]
pub extern "C" fn chars() -> *const u32 {
    APP.with(|a| {
        a.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |app| app.chars.as_ptr())
    })
}

/// Foreground and background colour indexes, two bytes per cell.
#[no_mangle]
pub extern "C" fn colors() -> *const u8 {
    APP.with(|a| {
        a.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |app| app.colors.as_ptr())
    })
}

/// 0 playing, 1 won, 2 dead.
#[no_mangle]
pub extern "C" fn state() -> u32 {
    APP.with(|a| match a.borrow().as_ref().map(|app| app.game.state) {
        Some(State::Won) => 1,
        Some(State::Dead) => 2,
        _ => 0,
    })
}

/// The run's share text, as UTF-8; its length comes from `text_len`.
#[no_mangle]
pub extern "C" fn summary() -> *const u8 {
    APP.with(|a| {
        let mut a = a.borrow_mut();
        let Some(app) = a.as_mut() else {
            return std::ptr::null();
        };
        app.text = app.game.summary().into_bytes();
        app.text.as_ptr()
    })
}

#[no_mangle]
pub extern "C" fn text_len() -> usize {
    APP.with(|a| a.borrow().as_ref().map_or(0, |app| app.text.len()))
}
