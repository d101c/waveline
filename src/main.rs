//! waveline — TUI unifiée Mixcloud + SoundCloud.
//!
//! Assemble les briques : cycle de vie du terminal, traduction des événements
//! clavier/souris en [`Action`]/[`Effect`], exécution des effets sur le moteur
//! audio et resynchronisation de l'affichage.

mod app;
mod audio;
mod b64;
mod config;
mod http;
mod i18n;
mod keymap;
mod model;
mod mpris;
mod providers;
mod state;
mod theme;
mod ui;

use std::io::{self, Stdout};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{Action, App, Effect, FetchRequest, Focus, Input, Section};
use audio::Player;
use config::Config;
use model::{Platform, Track};
use mpris::MediaCommand;
use providers::Fetched;
use state::State;
use theme::Theme;
use ui::Regions;

type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Résultat d'une requête réseau, livré à la boucle principale par un thread.
type FetchResult = (u64, Fetched);

fn main() -> io::Result<()> {
    // Modes debug hors-TUI.
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("resolve") => return debug_resolve(args.get(2).map(|s| s.as_str())),
        Some("play") => {
            return debug_play(
                args.get(2).map(|s| s.as_str()),
                args.get(3).map(|s| s.as_str()),
            )
        }
        Some("search") => {
            let q = args[2..].join(" ");
            let agent = http::agent();
            let found = providers::search_all(&agent, &q, 8);
            for t in &found.tracks {
                println!(
                    "[{}] {} — {} ({})",
                    t.platform.tag(),
                    t.artist,
                    t.title,
                    t.duration_human()
                );
            }
            for (p, e) in &found.failures {
                eprintln!("warning: {p}: {e}");
            }
            return Ok(());
        }
        Some("lib") => {
            // waveline lib <likes|playlists|feed> <sc_handle|-> <mc_handle|->
            use providers::LibrarySection::*;
            let sec = match args.get(2).map(|s| s.as_str()) {
                Some("playlists") => Playlists,
                Some("feed") => Feed,
                _ => Likes,
            };
            let sc = args
                .get(3)
                .filter(|s| s.as_str() != "-")
                .map(|s| s.as_str());
            let mc = args
                .get(4)
                .filter(|s| s.as_str() != "-")
                .map(|s| s.as_str());
            let agent = http::agent();
            let found = providers::library(&agent, sc, mc, sec);
            println!("{} tracks", found.tracks.len());
            for t in found.tracks.iter().take(20) {
                println!(
                    "[{}] {} — {} ({})",
                    t.platform.tag(),
                    t.artist,
                    t.title,
                    t.duration_human()
                );
            }
            for (p, e) in &found.failures {
                eprintln!("warning: {p}: {e}");
            }
            return Ok(());
        }
        Some("--version" | "-V") => {
            println!("waveline {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        _ => {}
    }

    // Préférences (comptes, langue, volume, visualiseur) et données d'usage
    // (file, historique) avant toute chose : le moteur audio démarre au bon
    // volume et la première frame montre déjà où l'on en était.
    let mut config = Config::load();
    let saved = State::load();
    let theme = Theme::dark();
    let mut app = App::new();
    app.sc_handle = config.soundcloud.clone();
    app.mc_handle = config.mixcloud.clone();
    app.playback.volume = config.volume;
    app.viz = config.viz;
    app.set_startup_lang(config.lang);
    app.restore(saved.queue, saved.history, demo_tracks());

    // Si quoi que ce soit panique, le terminal est rendu à l'utilisateur
    // (mode brut coupé, écran alternatif quitté) avant l'affichage du message.
    install_panic_hook();
    let mut terminal = setup_terminal()?;

    let res = run(&mut terminal, &mut app, &mut config, &theme);

    restore_terminal(&mut terminal)?;
    // Les préférences modifiées pendant la session (volume, visualiseur…) sont
    // écrites une fois, à la sortie, plutôt qu'à chaque appui.
    persist_config(&app, &mut config);
    persist_state(&mut app);
    res
}

/// Tout ce dont la boucle a besoin pour exécuter un [`Effect`].
struct Runtime<'a> {
    player: &'a Player,
    config: &'a mut Config,
    /// Émetteur cloné dans chaque thread de requête réseau.
    fetch_tx: Sender<FetchResult>,
}

fn run(terminal: &mut Tui, app: &mut App, config: &mut Config, theme: &Theme) -> io::Result<()> {
    let player = Player::new(app.playback.volume);
    let mut regions = Regions::default();
    let mut last_finished = player.shared().finished_generation.load(Ordering::Relaxed);
    let (fetch_tx, fetch_rx) = mpsc::channel::<FetchResult>();
    let mut rt = Runtime {
        player: &player,
        config,
        fetch_tx,
    };

    // Intégration MPRIS : touches média / contrôles bureau. No-op sans D-Bus.
    let (media_tx, media_rx) = mpsc::channel::<MediaCommand>();
    let _mpris = mpris::start(player.shared().clone(), media_tx);

    while !app.should_quit {
        app.tick(Instant::now());
        sync_playback(app, &player);

        // Commandes média externes (touches clavier, panneau, écran de verrou).
        while let Ok(cmd) = media_rx.try_recv() {
            if let Some(eff) = handle_media(app, cmd) {
                dispatch_effect(eff, app, &mut rt);
            }
        }

        // Résultats réseau prêts ? (les réponses périmées sont filtrées par l'app)
        while let Ok((id, fetched)) = fetch_rx.try_recv() {
            app.deliver(id, fetched);
        }

        // Enchaînement automatique quand un morceau se termine.
        let fin = player.shared().finished_generation.load(Ordering::Relaxed);
        if fin != last_finished {
            last_finished = fin;
            if let Some(eff) = app.apply(Action::Next) {
                dispatch_effect(eff, app, &mut rt);
            }
        }

        // File/historique modifiés : on persiste (petit JSON, écriture atomique).
        if app.take_state_dirty() {
            persist_state(app);
        }

        // Ne dessine que si le terminal a une taille exploitable (évite un
        // panic d'indexation sur un buffer 0×0, p. ex. sans vrai pty).
        let drawable = terminal
            .size()
            .map(|s| s.width > 0 && s.height > 0)
            .unwrap_or(false);
        if drawable {
            terminal.draw(|f| regions = ui::draw(f, app, theme))?;
            app.set_viewport_rows(regions.list_inner.height);
        }

        // Cadence adaptative : ~30 fps en lecture (visualiseurs fluides) ou
        // pendant une requête (spinner), ~4 fps au repos (CPU minimal).
        let animated = app.playback.playing || app.pending.is_some();
        let tick = if animated { 33 } else { 250 };
        if event::poll(Duration::from_millis(tick))? {
            let effect = match event::read()? {
                Event::Key(key) => handle_key(app, key),
                Event::Mouse(m) => handle_mouse(app, &regions, m),
                Event::Paste(text) => handle_paste(app, &text),
                _ => None,
            };
            if let Some(eff) = effect {
                dispatch_effect(eff, app, &mut rt);
            }
        }
    }
    Ok(())
}

/// Exécute un effet : audio direct, requête réseau en thread, ou sauvegarde.
/// Partagé par le clavier, la souris et MPRIS.
fn dispatch_effect(eff: Effect, app: &App, rt: &mut Runtime<'_>) {
    match eff {
        Effect::Fetch { id, request } => spawn_fetch(
            id,
            request,
            app.sc_handle.clone(),
            app.mc_handle.clone(),
            rt.fetch_tx.clone(),
        ),
        Effect::SaveConfig => persist_config(app, rt.config),
        Effect::Play(url) => rt.player.play_url(url),
        Effect::Toggle => rt.player.toggle(),
        Effect::Stop => rt.player.stop(),
        Effect::SetVolume(v) => rt.player.set_volume(v),
        Effect::Seek(delta_ms) => rt.player.seek(delta_ms),
    }
}

/// Recopie les préférences courantes de l'app dans la config et l'écrit.
fn persist_config(app: &App, config: &mut Config) {
    config.soundcloud = app.sc_handle.clone();
    config.mixcloud = app.mc_handle.clone();
    config.lang = app.lang;
    config.volume = app.playback.volume;
    config.viz = app.viz;
    config.save();
}

/// Écrit la file et l'historique sur disque.
fn persist_state(app: &mut App) {
    app.take_state_dirty();
    State {
        queue: app.queue.clone(),
        history: app.history.clone(),
    }
    .save();
}

/// Lance une requête réseau dans un thread ; le résultat (étiqueté par `id`)
/// revient par le canal, où la boucle le livre à l'app.
fn spawn_fetch(
    id: u64,
    request: FetchRequest,
    sc: Option<String>,
    mc: Option<String>,
    tx: Sender<FetchResult>,
) {
    std::thread::Builder::new()
        .name("waveline-fetch".into())
        .spawn(move || {
            let agent = http::agent();
            let found = match request {
                FetchRequest::Search(q) => providers::search_all(&agent, &q, 20),
                FetchRequest::Library(sec) => {
                    providers::library(&agent, sc.as_deref(), mc.as_deref(), sec)
                }
            };
            let _ = tx.send((id, found));
        })
        .expect("thread fetch");
}

/// Traduit une commande média externe (MPRIS) en action, comme une touche.
fn handle_media(app: &mut App, cmd: MediaCommand) -> Option<Effect> {
    match cmd {
        MediaCommand::PlayPause => app.apply(Action::PlayPause),
        MediaCommand::Next => app.apply(Action::Next),
        MediaCommand::Prev => app.apply(Action::Prev),
        MediaCommand::Play => {
            if !app.playback.playing {
                app.apply(Action::PlayPause)
            } else {
                None
            }
        }
        MediaCommand::Pause => {
            if app.playback.playing {
                app.apply(Action::PlayPause)
            } else {
                None
            }
        }
        MediaCommand::Stop => app.apply(Action::Stop),
        MediaCommand::SetVolume(v) => {
            app.playback.volume = v.min(100);
            Some(Effect::SetVolume(app.playback.volume))
        }
        MediaCommand::Seek(delta_ms) => app.seek(delta_ms),
        MediaCommand::SetPosition(ms) => app.seek_to(ms),
        MediaCommand::Quit => app.apply(Action::Quit),
    }
}

/// Recopie l'état du moteur dans le modèle d'affichage.
fn sync_playback(app: &mut App, player: &Player) {
    let s = player.shared();
    app.playback.position_ms = s.position_ms.load(Ordering::Relaxed);
    app.playback.duration_ms = s.duration_ms.load(Ordering::Relaxed);
    app.playback.playing = s.playing.load(Ordering::Relaxed);
    app.playback.loading = s.loading.load(Ordering::Relaxed);
    app.playback.seekable = s.seekable.load(Ordering::Relaxed);
    let now = s.now.lock().ok().map(|n| n.clone());
    if let Some(now) = now {
        app.sync_current(now);
    }
    if let Ok(spec) = s.spectrum.lock() {
        app.playback.spectrum = spec.to_vec();
    }
    if let Ok(wave) = s.waveform.lock() {
        app.playback.waveform = wave.clone();
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
    // L'aide est modale : n'importe quelle touche la referme.
    if app.show_help {
        app.show_help = false;
        return None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    // En mode saisie (URL, recherche, pseudos), les touches alimentent la ligne.
    if !matches!(app.input, Input::Normal) {
        return match (key.code, ctrl) {
            (KeyCode::Esc, _) => {
                app.input_cancel();
                None
            }
            (KeyCode::Enter, _) => app.input_submit(),
            (KeyCode::Backspace, _) => {
                app.input_pop();
                None
            }
            (KeyCode::Char('c'), true) => app.apply(Action::Quit),
            (KeyCode::Char('u'), true) => {
                app.input_clear();
                None
            }
            (KeyCode::Char('w'), true) => {
                app.input_pop_word();
                None
            }
            (KeyCode::Char(c), false) => {
                app.input_push(c);
                None
            }
            _ => None,
        };
    }
    keymap::action_for(&key).and_then(|a| app.apply(a))
}

fn handle_mouse(app: &mut App, regions: &Regions, m: MouseEvent) -> Option<Effect> {
    let (x, y) = (m.column, m.row);
    match m.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if app.show_help {
                app.show_help = false;
                return None;
            }
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
            if let Some(ratio) = regions.progress_ratio_at(x, y) {
                // Clic sur la barre de progression = saut à cette position.
                let target = (ratio * app.playback.duration_ms as f64) as u64;
                return app.seek_to(target);
            }
            if regions.lang_at(x, y) {
                return app.apply(Action::ToggleLang);
            }
            if regions.accounts_at(x, y) {
                return app.apply(Action::BeginConnect);
            }
            if let Some(i) = regions.section_at(x, y) {
                app.focus = Focus::Sidebar;
                app.select_section(Section::ALL[i]);
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
        // Molette sur la barre de lecture : volume. Ailleurs : défilement.
        MouseEventKind::ScrollDown if regions.playbar_at(x, y) => app.apply(Action::VolumeDown),
        MouseEventKind::ScrollUp if regions.playbar_at(x, y) => app.apply(Action::VolumeUp),
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

/// Texte collé (bracketed paste) : en saisie, il alimente la ligne d'un bloc ;
/// en mode normal, une URL SoundCloud/Mixcloud collée ouvre directement
/// l'invite `:` pré-remplie — il ne reste qu'à valider.
fn handle_paste(app: &mut App, text: &str) -> Option<Effect> {
    let clean: String = text.chars().filter(|c| !c.is_control()).collect();
    if clean.is_empty() {
        return None;
    }
    if matches!(app.input, Input::Normal) {
        // Hors saisie, seul un lien reconnu a un sens.
        providers::platform_of(clean.trim())?;
        app.begin_command();
    }
    for c in clean.chars() {
        app.input_push(c);
    }
    None
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
            println!("Platform   : {}", track.platform);
            println!("Title      : {}", track.title);
            println!("Artist     : {}", track.artist);
            println!("Duration   : {}", track.duration_human());
            println!("Container  : {:?}", source.container);
            match source.kind {
                providers::StreamKind::Progressive(u) => {
                    println!("Stream     : progressive");
                    println!("URL        : {}", truncate(&u, 100));
                }
                providers::StreamKind::HlsSegments(segs) => {
                    println!("Stream     : HLS, {} segments", segs.len());
                    if let Some(first) = segs.first() {
                        println!("Segment 0  : {}", truncate(first, 100));
                    }
                }
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Resolution failed: {e}");
            std::process::exit(1);
        }
    }
}

/// `waveline play <url> [seconds]` : joue le flux N secondes (test du moteur).
fn debug_play(url: Option<&str>, secs: Option<&str>) -> io::Result<()> {
    let Some(url) = url else {
        eprintln!("usage: waveline play <url> [seconds]");
        std::process::exit(2);
    };
    let limit = secs.and_then(|s| s.parse::<u64>().ok()).unwrap_or(10);
    let player = audio::Player::new(80);
    player.play_url(url);
    let shared = player.shared();
    let start = std::time::Instant::now();
    println!("Playing {limit}s of: {url}");
    loop {
        std::thread::sleep(Duration::from_millis(300));
        if let Some(err) = shared.error.lock().unwrap().clone() {
            eprintln!("Error: {err}");
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
            println!("\nTest finished.");
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
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let mut term = Terminal::new(CrosstermBackend::new(stdout))?;
    term.hide_cursor()?;
    Ok(term)
}

/// Rend le terminal à l'utilisateur. Idempotent et sans état : appelable
/// depuis le hook de panique comme depuis la sortie normale.
fn reset_terminal_modes() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        io::stdout(),
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen,
        crossterm::cursor::Show
    )
}

fn restore_terminal(terminal: &mut Tui) -> io::Result<()> {
    reset_terminal_modes()?;
    terminal.show_cursor()
}

/// Installe un hook de panique qui restaure le terminal avant d'afficher le
/// message : sans lui, un panic laisserait le shell en mode brut, sans écho
/// clavier et avec la capture souris active.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = reset_terminal_modes();
        default_hook(info);
    }));
}

// --- Données de démonstration (premier contact, sans compte) -----------------

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
