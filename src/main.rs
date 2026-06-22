//! waveline — TUI unifiée Mixcloud + SoundCloud.
//!
//! Ce module assemble les briques : il gère le cycle de vie du terminal,
//! traduit les événements clavier/souris en [`Action`] et redessine. Toute la
//! logique d'état vit dans [`app`], le rendu dans [`ui`].

mod app;
mod model;
mod theme;
mod ui;

use std::io::{self, Stdout};
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

use app::{Action, App, Focus};
use model::{Platform, Track};
use theme::Theme;
use ui::Regions;

type Tui = Terminal<CrosstermBackend<Stdout>>;

fn main() -> io::Result<()> {
    let mut terminal = setup_terminal()?;
    let theme = Theme::dark();
    let mut app = App::new();
    app.tracks = demo_tracks();

    let res = run(&mut terminal, &mut app, &theme);

    restore_terminal(&mut terminal)?;
    res
}

fn run(terminal: &mut Tui, app: &mut App, theme: &Theme) -> io::Result<()> {
    let mut regions = Regions::default();
    while !app.should_quit {
        terminal.draw(|f| regions = ui::draw(f, app, theme))?;

        // Bloque jusqu'à un événement (pas de busy-loop). Le timeout permettra
        // plus tard de rafraîchir la position de lecture.
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) => handle_key(app, key),
                Event::Mouse(m) => handle_mouse(app, &regions, m),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent) {
    if key.kind != KeyEventKind::Press {
        return;
    }
    // Ctrl-C quitte toujours.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.apply(Action::Quit);
        return;
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
        KeyCode::Char('?') => {
            app.status =
                "Aide : navigation j/k, tab change de panneau, enter joue, 1/2/3 filtre".into();
            None
        }
        _ => None,
    };
    if let Some(a) = action {
        app.apply(a);
    }
}

fn handle_mouse(app: &mut App, regions: &Regions, m: MouseEvent) {
    let (x, y) = (m.column, m.row);
    match m.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // Onglet de filtre ?
            if let Some(filter) = regions.filter_at(x, y) {
                match filter {
                    app::Filter::All => app.apply(Action::FilterAll),
                    app::Filter::Only(Platform::SoundCloud) => app.apply(Action::FilterSoundCloud),
                    app::Filter::Only(Platform::Mixcloud) => app.apply(Action::FilterMixcloud),
                }
                return;
            }
            // Bouton play/pause ?
            if regions.playpause_at(x, y) {
                app.apply(Action::PlayPause);
                return;
            }
            // Section de la sidebar ?
            if let Some(i) = regions.section_at(x, y) {
                app.focus = Focus::Sidebar;
                app.section_index = i;
                app.section = app::Section::ALL[i];
                app.apply(Action::Activate);
                return;
            }
            // Ligne de la liste ?
            if let Some(i) = regions.list_row_at(x, y) {
                app.focus = Focus::List;
                app.list_index = i;
            }
        }
        MouseEventKind::ScrollDown => {
            app.focus = Focus::List;
            app.apply(Action::Down);
        }
        MouseEventKind::ScrollUp => {
            app.focus = Focus::List;
            app.apply(Action::Up);
        }
        _ => {}
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
    let mk = |p: Platform, artist: &str, title: &str, ms: u64| Track {
        platform: p,
        id: format!("{artist}-{title}"),
        title: title.into(),
        artist: artist.into(),
        permalink: "https://example.com".into(),
        duration_ms: Some(ms),
    };
    vec![
        mk(Platform::SoundCloud, "Bonobo", "Kerala", 290_000),
        mk(Platform::Mixcloud, "Ben UFO", "Rinse FM b2b set", 3_731_000),
        mk(Platform::SoundCloud, "Four Tet", "Two Thousand and Seventeen", 250_000),
        mk(Platform::Mixcloud, "Gilles Peterson", "Worldwide Show", 7_200_000),
        mk(Platform::SoundCloud, "Floating Points", "LesAlpx", 380_000),
        mk(Platform::Mixcloud, "The Blessed Madonna", "We Still Believe", 5_400_000),
        mk(Platform::SoundCloud, "Jamie xx", "Gosh", 320_000),
    ]
}
