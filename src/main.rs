//! waveline — TUI unifiée Mixcloud + SoundCloud.
//!
//! Assemble les briques : cycle de vie du terminal, traduction des événements
//! clavier/souris en [`Action`]/[`Effect`], exécution des effets sur le moteur
//! audio et resynchronisation de l'affichage.

mod app;
mod audio;
mod b64;
mod http;
mod model;
mod providers;
mod theme;
mod ui;

use std::io::{self, Stdout};
use std::sync::atomic::Ordering;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{Action, App, Effect, Focus, Input};
use audio::Player;
use model::{Platform, Track};
use theme::Theme;
use ui::Regions;

type Tui = Terminal<CrosstermBackend<Stdout>>;

fn main() -> io::Result<()> {
    // Modes debug hors-TUI.
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("resolve") => return debug_resolve(args.get(2).map(|s| s.as_str())),
        Some("play") => {
            return debug_play(args.get(2).map(|s| s.as_str()), args.get(3).map(|s| s.as_str()))
        }
        _ => {}
    }

    let mut terminal = setup_terminal()?;
    let theme = Theme::dark();
    let mut app = App::new();
    app.tracks = demo_tracks();

    let res = run(&mut terminal, &mut app, &theme);

    restore_terminal(&mut terminal)?;
    res
}

fn run(terminal: &mut Tui, app: &mut App, theme: &Theme) -> io::Result<()> {
    let player = Player::new(app.playback.volume);
    let mut regions = Regions::default();
    let mut last_finished = player.shared().finished_generation.load(Ordering::Relaxed);

    while !app.should_quit {
        sync_playback(app, &player);

        // Enchaînement automatique quand un morceau se termine.
        let fin = player.shared().finished_generation.load(Ordering::Relaxed);
        if fin != last_finished {
            last_finished = fin;
            if let Some(eff) = app.apply(Action::Next) {
                exec(&player, eff);
            }
        }

        // Ne dessine que si le terminal a une taille exploitable (évite un
        // panic d'indexation sur un buffer 0×0, p. ex. sans vrai pty).
        let drawable = terminal
            .size()
            .map(|s| s.width > 0 && s.height > 0)
            .unwrap_or(false);
        if drawable {
            terminal.draw(|f| regions = ui::draw(f, app, theme))?;
        }

        // Timeout court : rafraîchit la position de lecture ~4×/s.
        if event::poll(Duration::from_millis(250))? {
            let effect = match event::read()? {
                Event::Key(key) => handle_key(app, key),
                Event::Mouse(m) => handle_mouse(app, &regions, m),
                _ => None,
            };
            if let Some(eff) = effect {
                exec(&player, eff);
            }
        }
    }
    Ok(())
}

/// Exécute un effet sur le moteur audio.
fn exec(player: &Player, effect: Effect) {
    match effect {
        Effect::Play(url) => player.play_url(url),
        Effect::Toggle => player.toggle(),
        Effect::Stop => player.stop(),
        Effect::SetVolume(v) => player.set_volume(v),
    }
}

/// Recopie l'état du moteur dans le modèle d'affichage.
fn sync_playback(app: &mut App, player: &Player) {
    let s = player.shared();
    app.playback.position_ms = s.position_ms.load(Ordering::Relaxed);
    app.playback.duration_ms = s.duration_ms.load(Ordering::Relaxed);
    app.playback.playing = s.playing.load(Ordering::Relaxed);
    app.playback.loading = s.loading.load(Ordering::Relaxed);
    if let Ok(now) = s.now.lock() {
        app.playback.current = now.clone();
    }
    if let Ok(mut err) = s.error.lock() {
        if let Some(e) = err.take() {
            app.status = format!("⚠ {e}");
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent) -> Option<Effect> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    // En mode saisie, les touches alimentent la ligne de commande.
    if matches!(app.input, Input::Command(_)) {
        return match key.code {
            KeyCode::Esc => {
                app.input_cancel();
                None
            }
            KeyCode::Enter => app.input_submit(),
            KeyCode::Backspace => {
                app.input_pop();
                None
            }
            KeyCode::Char(c) => {
                app.input_push(c);
                None
            }
            _ => None,
        };
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return app.apply(Action::Quit);
    }

    let action = match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Char(' ') => Some(Action::PlayPause),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::Down),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::Up),
        KeyCode::Char('g') => Some(Action::Top),
        KeyCode::Char('G') => Some(Action::Bottom),
        KeyCode::Enter => Some(Action::Activate),
        KeyCode::Tab => Some(Action::ToggleFocus),
        KeyCode::Char('h') | KeyCode::Left => Some(Action::FocusSidebar),
        KeyCode::Char('l') | KeyCode::Right => Some(Action::FocusList),
        KeyCode::Char('n') => Some(Action::Next),
        KeyCode::Char('p') => Some(Action::Prev),
        KeyCode::Char('+') | KeyCode::Char('=') => Some(Action::VolumeUp),
        KeyCode::Char('-') => Some(Action::VolumeDown),
        KeyCode::Char('1') => Some(Action::FilterAll),
        KeyCode::Char('2') => Some(Action::FilterSoundCloud),
        KeyCode::Char('3') => Some(Action::FilterMixcloud),
        KeyCode::Char(':') => {
            app.begin_command();
            None
        }
        KeyCode::Char('?') => {
            app.status =
                "Aide : ':' URL · j/k naviguer · enter/clic jouer · space pause · n/p · 1/2/3 filtre · q quitter".into();
            None
        }
        _ => None,
    };
    action.and_then(|a| app.apply(a))
}

fn handle_mouse(app: &mut App, regions: &Regions, m: MouseEvent) -> Option<Effect> {
    let (x, y) = (m.column, m.row);
    match m.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(filter) = regions.filter_at(x, y) {
                return match filter {
                    app::Filter::All => app.apply(Action::FilterAll),
                    app::Filter::Only(Platform::SoundCloud) => app.apply(Action::FilterSoundCloud),
                    app::Filter::Only(Platform::Mixcloud) => app.apply(Action::FilterMixcloud),
                };
            }
            if regions.playpause_at(x, y) {
                return app.apply(Action::PlayPause);
            }
            if let Some(i) = regions.section_at(x, y) {
                app.focus = Focus::Sidebar;
                app.section_index = i;
                app.section = app::Section::ALL[i];
                return app.apply(Action::Activate);
            }
            if let Some(i) = regions.list_row_at(x, y) {
                // Clic sur un morceau = sélection + lecture immédiate.
                app.focus = Focus::List;
                app.list_index = i;
                return app.apply(Action::Activate);
            }
            None
        }
        MouseEventKind::ScrollDown => {
            app.focus = Focus::List;
            app.apply(Action::Down)
        }
        MouseEventKind::ScrollUp => {
            app.focus = Focus::List;
            app.apply(Action::Up)
        }
        _ => None,
    }
}

// --- Modes debug --------------------------------------------------------------

fn debug_resolve(url: Option<&str>) -> io::Result<()> {
    let Some(url) = url else {
        eprintln!("usage: waveline resolve <url soundcloud|mixcloud>");
        std::process::exit(2);
    };
    let agent = http::agent();
    match providers::resolve_url(&agent, url) {
        Ok((track, source)) => {
            println!("Plateforme : {}", track.platform);
            println!("Titre      : {}", track.title);
            println!("Artiste    : {}", track.artist);
            println!("Durée      : {}", track.duration_human());
            println!("Conteneur  : {:?}", source.container);
            match source.kind {
                providers::StreamKind::Progressive(u) => {
                    println!("Flux       : progressif");
                    println!("URL        : {}", truncate(&u, 100));
                }
                providers::StreamKind::HlsSegments(segs) => {
                    println!("Flux       : HLS, {} segments", segs.len());
                    if let Some(first) = segs.first() {
                        println!("Segment 0  : {}", truncate(first, 100));
                    }
                }
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Échec de résolution : {e}");
            std::process::exit(1);
        }
    }
}

/// `waveline play <url> [secondes]` : joue le flux N secondes (test du moteur).
fn debug_play(url: Option<&str>, secs: Option<&str>) -> io::Result<()> {
    let Some(url) = url else {
        eprintln!("usage: waveline play <url> [secondes]");
        std::process::exit(2);
    };
    let limit = secs.and_then(|s| s.parse::<u64>().ok()).unwrap_or(10);
    let player = audio::Player::new(80);
    player.play_url(url);
    let shared = player.shared();
    let start = std::time::Instant::now();
    println!("Lecture {limit}s de : {url}");
    loop {
        std::thread::sleep(Duration::from_millis(300));
        if let Some(err) = shared.error.lock().unwrap().clone() {
            eprintln!("Erreur : {err}");
            std::process::exit(1);
        }
        let pos = shared.position_ms.load(Ordering::Relaxed);
        let dur = shared.duration_ms.load(Ordering::Relaxed);
        let loading = shared.loading.load(Ordering::Relaxed);
        let playing = shared.playing.load(Ordering::Relaxed);
        print!(
            "\r  {} pos={}.{:02}s / {}s   ",
            if loading {
                "⏳"
            } else if playing {
                "▶"
            } else {
                "⏸"
            },
            pos / 1000,
            (pos % 1000) / 10,
            dur / 1000
        );
        use std::io::Write as _;
        let _ = io::stdout().flush();
        if start.elapsed().as_secs() >= limit {
            println!("\nFin du test.");
            break;
        }
    }
    Ok(())
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

// --- Cycle de vie du terminal -------------------------------------------------

fn setup_terminal() -> io::Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let mut term = Terminal::new(CrosstermBackend::new(stdout))?;
    term.hide_cursor()?;
    Ok(term)
}

fn restore_terminal(terminal: &mut Tui) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

// --- Données de démonstration (remplacées par les providers ensuite) ----------

fn demo_tracks() -> Vec<Track> {
    // Vraies URLs jouables (libres de DRM) pour un premier contact concret.
    let mk = |p: Platform, artist: &str, title: &str, url: &str, ms: u64| Track {
        platform: p,
        id: url.into(),
        title: title.into(),
        artist: artist.into(),
        permalink: url.into(),
        duration_ms: Some(ms),
    };
    vec![
        mk(
            Platform::SoundCloud,
            "Hidaka",
            "90's J-POP NON STOP DJ MIX",
            "https://soundcloud.com/user-885578460/90s-j-pop-non-stop-mix",
            4_233_000,
        ),
        mk(
            Platform::Mixcloud,
            "NTS Radio",
            "Andy Butler / Hercules & Love Affair",
            "https://www.mixcloud.com/NTSRadio/andy-butler-hercules-love-affair-19th-june-2026/",
            3_584_000,
        ),
    ]
}
