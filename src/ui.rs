//! Rendu de l'interface + cartographie des zones cliquables.
//!
//! `draw` retourne un [`Regions`] qui mémorise où chaque élément interactif a
//! été dessiné, afin que la boucle d'événements puisse traduire un clic
//! (colonne, ligne) en [`Action`](crate::app::Action) ou en sélection.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, LineGauge, Paragraph, Row, Table, TableState,
};
use ratatui::Frame;

use crate::app::{App, Filter, Focus, Input, Section};
use crate::i18n;
use crate::keymap::{self, Group};
use crate::model::{fmt_duration, Platform};
use crate::theme::Theme;

/// Zones interactives mémorisées au dernier rendu, pour le hit-test souris.
#[derive(Default, Clone)]
pub struct Regions {
    pub sidebar_rows: Vec<Rect>,
    /// (rect de l'onglet, filtre associé)
    pub filter_tabs: Vec<(Rect, Filter)>,
    /// Zone interne de la liste (sans bordure) + premier index visible.
    pub list_inner: Rect,
    pub list_first: usize,
    pub list_len: usize,
    /// Bouton play/pause de la barre de lecture.
    pub playpause_btn: Rect,
    /// Barre de lecture entière (molette = volume).
    pub playbar: Rect,
    /// Partie « trait » de la barre de progression (clic = saut).
    pub progress: Rect,
    /// Ligne « comptes (c) » de la sidebar (clic = connexion).
    pub accounts_btn: Rect,
    /// Ligne de bascule de langue, en bas de la sidebar.
    pub lang_btn: Rect,
}

impl Regions {
    /// Retrouve la section cliquée dans la sidebar.
    pub fn section_at(&self, x: u16, y: u16) -> Option<usize> {
        self.sidebar_rows.iter().position(|r| contains(r, x, y))
    }

    /// Retrouve le filtre dont l'onglet a été cliqué.
    pub fn filter_at(&self, x: u16, y: u16) -> Option<Filter> {
        self.filter_tabs
            .iter()
            .find(|(r, _)| contains(r, x, y))
            .map(|(_, f)| *f)
    }

    /// Index (dans la liste filtrée) de la ligne cliquée.
    pub fn list_row_at(&self, x: u16, y: u16) -> Option<usize> {
        if !contains(&self.list_inner, x, y) {
            return None;
        }
        let offset = (y - self.list_inner.y) as usize + self.list_first;
        if offset < self.list_len {
            Some(offset)
        } else {
            None
        }
    }

    pub fn playpause_at(&self, x: u16, y: u16) -> bool {
        contains(&self.playpause_btn, x, y)
    }

    pub fn lang_at(&self, x: u16, y: u16) -> bool {
        contains(&self.lang_btn, x, y)
    }

    pub fn accounts_at(&self, x: u16, y: u16) -> bool {
        contains(&self.accounts_btn, x, y)
    }

    pub fn playbar_at(&self, x: u16, y: u16) -> bool {
        contains(&self.playbar, x, y)
    }

    /// Position relative (0..1) d'un clic sur la barre de progression.
    pub fn progress_ratio_at(&self, x: u16, y: u16) -> Option<f64> {
        if self.progress.width == 0 || !contains(&self.progress, x, y) {
            return None;
        }
        Some(f64::from(x - self.progress.x) / f64::from(self.progress.width))
    }
}

fn contains(r: &Rect, x: u16, y: u16) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

pub fn draw(f: &mut Frame, app: &App, theme: &Theme) -> Regions {
    let mut regions = Regions::default();

    // Terminal dégénéré (trop petit / taille nulle) : on évite tout rendu qui
    // indexerait hors du buffer, et on invite à agrandir si la place le permet.
    let area = f.area();
    if area.width < 24 || area.height < 14 {
        if area.width >= 1 && area.height >= 1 {
            f.render_widget(
                Paragraph::new(i18n::resize_hint(app.lang)).style(Style::default().fg(theme.fg)),
                area,
            );
        }
        return regions;
    }

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(3),    // corps
            Constraint::Length(7), // barre de lecture + analyseur (3 lignes)
            Constraint::Length(1), // ligne de statut + raccourcis
        ])
        .split(f.area());

    draw_header(f, root[0], app, theme, &mut regions);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(20)])
        .split(root[1]);

    draw_sidebar(f, body[0], app, theme, &mut regions);
    draw_list(f, body[1], app, theme, &mut regions);
    draw_playbar(f, root[2], app, theme, &mut regions);
    draw_status(f, root[3], app, theme);

    if app.show_help {
        draw_help(f, area, app, theme);
    }

    regions
}

fn draw_header(f: &mut Frame, area: Rect, app: &App, theme: &Theme, reg: &mut Regions) {
    // Titre à gauche.
    let title = Span::styled(
        " waveline ",
        Style::default()
            .fg(theme.bg)
            .bg(theme.accent)
            .add_modifier(Modifier::BOLD),
    );
    f.render_widget(Paragraph::new(Line::from(title)), area);

    // Onglets de filtre à droite : [All/Tout] [SC] [MC].
    let tabs = [
        (Filter::All.label(app.lang), Filter::All),
        ("SC".to_string(), Filter::Only(Platform::SoundCloud)),
        ("MC".to_string(), Filter::Only(Platform::Mixcloud)),
    ];
    // Calcule la largeur totale puis pose les onglets en partant de la droite.
    let labels: Vec<String> = tabs.iter().map(|(t, _)| format!(" {t} ")).collect();
    let total: u16 =
        labels.iter().map(|s| s.chars().count() as u16).sum::<u16>() + (tabs.len() as u16 - 1);
    let mut x = area.x + area.width.saturating_sub(total);
    for (i, (_, filt)) in tabs.iter().enumerate() {
        let w = labels[i].chars().count() as u16;
        let r = Rect::new(x, area.y, w, 1);
        let active = *filt == app.filter;
        let style = if active {
            Style::default()
                .fg(theme.bg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.dim)
        };
        f.render_widget(Paragraph::new(Span::styled(labels[i].clone(), style)), r);
        reg.filter_tabs.push((r, *filt));
        x += w + 1;
    }
}

fn draw_sidebar(f: &mut Frame, area: Rect, app: &App, theme: &Theme, reg: &mut Regions) {
    let active = app.focus == Focus::Sidebar;
    let block = panel_block("Sources", active, theme);
    let inner = block.inner(area);
    f.render_widget(block, area);

    for (i, section) in Section::ALL.iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.y + inner.height {
            break;
        }
        let row = Rect::new(inner.x, y, inner.width, 1);
        let selected = i == app.section_index;
        let mut style = Style::default().fg(theme.fg);
        if selected {
            style = style.bg(theme.selection_bg).add_modifier(Modifier::BOLD);
            if active {
                style = style.fg(theme.accent);
            }
        }
        let mut label = format!(" {}", section.label(app.lang));
        // La file affiche son nombre d'éléments : on sait d'un coup d'œil ce
        // qui attend, sans l'ouvrir.
        if *section == Section::Queue && !app.queue.is_empty() {
            label.push_str(&format!(" ({})", app.queue.len()));
        }
        f.render_widget(Paragraph::new(Span::styled(label, style)), row);
        reg.sidebar_rows.push(row);
    }

    // Comptes connectés + langue, en bas de la sidebar.
    if inner.height >= 5 {
        let base = inner.y + inner.height - 4;
        let sc = app.sc_handle.as_deref().unwrap_or("—");
        let mc = app.mc_handle.as_deref().unwrap_or("—");
        let lines = [
            Span::styled(
                i18n::accounts_hint(app.lang),
                Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" SC {sc}"), Style::default().fg(theme.soundcloud)),
            Span::styled(format!(" MC {mc}"), Style::default().fg(theme.mixcloud)),
            Span::styled(
                i18n::lang_hint(app.lang),
                Style::default().fg(theme.dim).add_modifier(Modifier::BOLD),
            ),
        ];
        for (k, span) in lines.into_iter().enumerate() {
            let y = base + k as u16;
            if y < inner.y + inner.height {
                let row = Rect::new(inner.x, y, inner.width, 1);
                f.render_widget(Paragraph::new(span), row);
                match k {
                    0 => reg.accounts_btn = row,
                    3 => reg.lang_btn = row,
                    _ => {}
                }
            }
        }
    }
}

fn draw_list(f: &mut Frame, area: Rect, app: &App, theme: &Theme, reg: &mut Regions) {
    let active = app.focus == Focus::List;
    let title = format!(
        "{}  ·  {}",
        app.section.name(app.lang),
        app.filter.label(app.lang)
    );
    let block = panel_block(&title, active, theme);
    let inner = block.inner(area);
    f.render_widget(&block, area);

    let visible = app.visible_indices();
    reg.list_inner = inner;
    reg.list_len = visible.len();

    if visible.is_empty() {
        let hint = Paragraph::new(Line::from(Span::styled(
            i18n::empty_hint(app.lang, app.section, app.has_account()),
            Style::default().fg(theme.dim),
        )));
        f.render_widget(hint, inner);
        return;
    }

    // Fenêtre de défilement simple centrée sur la sélection.
    let height = inner.height as usize;
    let first = scroll_first(app.list_index, visible.len(), height);
    reg.list_first = first;

    let tracks = app.tracks();
    let rows: Vec<Row> = visible
        .iter()
        .skip(first)
        .take(height)
        .map(|&track_i| {
            let t = &tracks[track_i];
            let is_current = app
                .playback
                .current
                .as_ref()
                .map(|c| c.same_as(t))
                .unwrap_or(false);
            let marker = if is_current {
                if app.playback.playing {
                    "▶ "
                } else {
                    "⏸ "
                }
            } else {
                "  "
            };
            let title_style = if is_current {
                Style::default()
                    .fg(theme.playing)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.fg)
            };
            let plat = Span::styled(
                t.platform.tag(),
                Style::default()
                    .fg(theme.platform(t.platform))
                    .add_modifier(Modifier::BOLD),
            );
            Row::new(vec![
                Cell::from(Span::styled(format!("{marker}{}", t.title), title_style)),
                Cell::from(Span::styled(
                    t.artist.clone(),
                    Style::default().fg(theme.dim),
                )),
                Cell::from(Span::styled(
                    t.duration_human(),
                    Style::default().fg(theme.dim),
                )),
                Cell::from(plat),
            ])
        })
        .collect();

    let widths = [
        Constraint::Percentage(50),
        Constraint::Percentage(34),
        Constraint::Length(8),
        Constraint::Length(3),
    ];
    let mut state = TableState::default();
    state.select(Some(app.list_index.saturating_sub(first)));
    let table = Table::new(rows, widths)
        .row_highlight_style(
            Style::default()
                .bg(theme.selection_bg)
                .add_modifier(Modifier::BOLD),
        )
        .column_spacing(1);
    f.render_stateful_widget(table, inner, &mut state);
}

fn draw_playbar(f: &mut Frame, area: Rect, app: &App, theme: &Theme, reg: &mut Regions) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));
    let inner = block.inner(area);
    f.render_widget(block, area);
    reg.playbar = area;

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // morceau + volume
            Constraint::Min(1),    // analyseur de spectre
            Constraint::Length(1), // progression
        ])
        .split(inner);

    let pb = &app.playback;
    let (icon, line) = if pb.loading {
        (
            "⏳",
            Line::from(Span::styled(
                i18n::loading_playbar(app.lang),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
        )
    } else {
        match &pb.current {
            Some(t) => {
                let icon = if pb.playing { "▶" } else { "⏸" };
                let line = Line::from(vec![
                    Span::styled(
                        format!(" {icon} "),
                        Style::default()
                            .fg(theme.playing)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{} — {}", t.artist, t.title),
                        Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("   vol {}%", pb.volume),
                        Style::default().fg(theme.dim),
                    ),
                ]);
                (icon, line)
            }
            None => (
                "·",
                Line::from(Span::styled(
                    i18n::nothing_playing(app.lang),
                    Style::default().fg(theme.dim),
                )),
            ),
        }
    };
    let _ = icon;
    f.render_widget(Paragraph::new(line), rows[0]);
    // Le bouton play/pause = les 3 premières colonnes de la première ligne.
    reg.playpause_btn = Rect::new(rows[0].x, rows[0].y, 3, 1);

    // Barre de progression.
    let dur = pb.current.as_ref().and_then(|t| t.duration_ms).unwrap_or(0);
    let ratio = if dur > 0 {
        (pb.position_ms as f64 / dur as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let label = format!(
        "{} / {}",
        fmt_duration(pb.position_ms),
        if dur > 0 {
            fmt_duration(dur)
        } else {
            "--:--".into()
        }
    );
    // Le trait de la jauge commence après le libellé et un espace : seule
    // cette partie est cliquable pour sauter dans le morceau.
    let label_w = label.chars().count() as u16 + 1;
    reg.progress = Rect::new(
        rows[2].x + label_w.min(rows[2].width),
        rows[2].y,
        rows[2].width.saturating_sub(label_w),
        1,
    );
    let gauge = LineGauge::default()
        .filled_style(Style::default().fg(theme.accent))
        .unfilled_style(Style::default().fg(theme.border))
        .ratio(ratio)
        .label(Span::styled(label, Style::default().fg(theme.dim)));

    // Visualiseur au milieu (style choisi par l'utilisateur), progression en bas.
    draw_visualizer(f, rows[1], app, theme);
    f.render_widget(gauge, rows[2]);
}

const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const PER_ROW: usize = 8;

/// Dispatche vers le style de visualiseur courant.
fn draw_visualizer(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    match app.viz {
        crate::app::VizMode::Bars => draw_bars(f, area, &app.playback.spectrum),
        crate::app::VizMode::Mirror => draw_mirror(f, area, &app.playback.spectrum),
        crate::app::VizMode::Scope => draw_scope(f, area, &app.playback.waveform, theme),
    }
}

/// Barres de spectre ancrées en bas (blocs en huitièmes).
fn draw_bars(f: &mut Frame, area: Rect, bands: &[f32]) {
    if bands.is_empty() {
        return;
    }
    let n = bands.len();
    let bar_w = (area.width as usize / n).max(1);
    let total_levels = area.height as usize * PER_ROW;
    for row in 0..area.height {
        let from_bottom = (area.height - 1 - row) as usize;
        let base = from_bottom * PER_ROW;
        let mut spans: Vec<Span> = Vec::with_capacity(n);
        for (i, &v) in bands.iter().enumerate() {
            let filled = (v.clamp(0.0, 1.0) * total_levels as f32).round() as usize;
            let lvl = filled.saturating_sub(base).min(PER_ROW);
            let s: String = std::iter::repeat_n(BLOCKS[lvl], bar_w).collect();
            spans.push(Span::styled(s, Style::default().fg(band_color(i, n))));
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, area.y + row, area.width, 1),
        );
    }
}

/// Spectre symétrique autour de la ligne centrale (« waveline »).
fn draw_mirror(f: &mut Frame, area: Rect, bands: &[f32]) {
    if bands.is_empty() {
        return;
    }
    let n = bands.len();
    let bar_w = (area.width as usize / n).max(1);
    let h = area.height as usize;
    for row in 0..area.height {
        let r = row as usize;
        let mut spans: Vec<Span> = Vec::with_capacity(n);
        for (i, &v) in bands.iter().enumerate() {
            // Hauteur lumineuse centrée verticalement.
            let lit = (v.clamp(0.0, 1.0) * h as f32).round() as usize;
            let first = (h.saturating_sub(lit)) / 2;
            let on = r >= first && r < first + lit;
            let ch = if on { '█' } else { ' ' };
            let s: String = std::iter::repeat_n(ch, bar_w).collect();
            spans.push(Span::styled(s, Style::default().fg(band_color(i, n))));
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(area.x, area.y + row, area.width, 1),
        );
    }
}

/// Oscilloscope temporel : trace la forme d'onde colonne par colonne.
fn draw_scope(f: &mut Frame, area: Rect, wave: &[f32], theme: &Theme) {
    let w = area.width as usize;
    let h = area.height as usize;
    if w == 0 || h == 0 {
        return;
    }
    // Ligne médiane discrète quand il n'y a pas de signal.
    if wave.is_empty() {
        let mid = area.y + (area.height / 2);
        let line: String = std::iter::repeat_n('·', w).collect();
        f.render_widget(
            Paragraph::new(Span::styled(line, Style::default().fg(theme.border))),
            Rect::new(area.x, mid, area.width, 1),
        );
        return;
    }
    // Rangée cible (depuis le haut) pour chaque colonne.
    let len = wave.len();
    let target: Vec<usize> = (0..w)
        .map(|x| {
            let a = wave[x * len / w].clamp(-1.0, 1.0);
            let norm = 1.0 - (a + 1.0) / 2.0; // 0 = haut, 1 = bas
            (norm * (h as f32 - 1.0)).round() as usize
        })
        .collect();
    for row in 0..area.height {
        let r = row as usize;
        let s: String = (0..w)
            .map(|x| if target[x] == r { '█' } else { ' ' })
            .collect();
        f.render_widget(
            Paragraph::new(Span::styled(s, Style::default().fg(theme.accent))),
            Rect::new(area.x, area.y + row, area.width, 1),
        );
    }
}

/// Dégradé vert (graves) → rouge (aigus) pour colorer les bandes.
fn band_color(i: usize, n: usize) -> ratatui::style::Color {
    let t = if n <= 1 {
        0.0
    } else {
        i as f32 / (n - 1) as f32
    };
    let r = (40.0 + t * 215.0) as u8;
    let g = (220.0 - t * 150.0) as u8;
    let b = (120.0 - t * 80.0) as u8;
    ratatui::style::Color::Rgb(r, g, b)
}

fn draw_status(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    // En mode saisie : invite (« : » URL ou « / » recherche) + tampon + curseur.
    let prompt_hint = match &app.input {
        Input::Command(buf) => Some((":", buf, i18n::prompt_command_hint(app.lang))),
        Input::Search(buf) => Some(("/", buf, i18n::prompt_search_hint(app.lang))),
        Input::ConnectSoundCloud(buf) => {
            Some(("SoundCloud", buf, i18n::prompt_connect_sc_hint(app.lang)))
        }
        Input::ConnectMixcloud(buf) => {
            Some(("Mixcloud", buf, i18n::prompt_connect_mc_hint(app.lang)))
        }
        Input::Normal => None,
    };
    if let Some((prompt, buf, hint)) = prompt_hint {
        let line = Line::from(vec![
            Span::styled(
                format!(" {prompt} "),
                Style::default()
                    .fg(theme.bg)
                    .bg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {buf}▏"),
                Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  ({hint})"), Style::default().fg(theme.dim)),
        ]);
        f.render_widget(Paragraph::new(line), area);
        return;
    }
    let keys = i18n::keys_bar(app.lang);
    let mut spans = Vec::with_capacity(3);
    // Requête réseau en cours : un spinner animé devant le statut.
    if let Some(p) = app.pending {
        let elapsed = app.now().saturating_duration_since(p.started);
        spans.push(Span::styled(
            format!(" {}", spinner_frame(elapsed.as_millis() as u64)),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(
        format!(" {} ", app.status),
        Style::default().fg(theme.fg),
    ));
    spans.push(Span::styled(
        format!("  {keys}"),
        Style::default().fg(theme.dim),
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Image du spinner pour un temps écoulé donné (une image toutes les 80 ms).
fn spinner_frame(elapsed_ms: u64) -> char {
    SPINNER[(elapsed_ms / 80) as usize % SPINNER.len()]
}

/// Fenêtre d'aide centrée, générée depuis la table des raccourcis (`keymap`) :
/// elle ne peut pas diverger de ce que fait réellement le clavier.
///
/// Deux colonnes équilibrées dès que la largeur le permet (tient dans un
/// terminal de 24 lignes), une seule sinon.
fn draw_help(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let key_style = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let title_style = Style::default().fg(theme.dim).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(theme.fg);
    let lang = app.lang;

    // Une rubrique = un titre + une ligne par raccourci (touches alignées).
    let section = |title: &str, items: Vec<(String, &str)>| -> Vec<Line<'static>> {
        let mut lines = vec![Line::from(Span::styled(title.to_string(), title_style))];
        for (keys, desc) in items {
            lines.push(Line::from(vec![
                Span::styled(format!("  {keys:<9}"), key_style),
                Span::styled(desc.to_string(), text_style),
            ]));
        }
        lines
    };
    let group = |g: Group| -> Vec<Line<'static>> {
        section(
            g.title(lang),
            keymap::in_group(g)
                .map(|b| (b.keys_label(), b.help(lang)))
                .collect(),
        )
    };
    let mouse = section(
        i18n::help_mouse_title(lang),
        i18n::help_mouse_items(lang)
            .iter()
            .map(|(k, d)| (k.to_string(), *d))
            .collect(),
    );

    // Répartition : à gauche navigation + listes + souris, à droite lecture +
    // application — les deux colonnes font à peu près la même hauteur.
    let mut left = group(Group::Navigate);
    left.push(Line::default());
    left.extend(group(Group::Lists));
    left.push(Line::default());
    left.extend(mouse);
    let mut right = group(Group::Playback);
    right.push(Line::default());
    right.extend(group(Group::App));

    let two_cols = area.width >= 80;
    let (columns, content_h): (Vec<Vec<Line>>, u16) = if two_cols {
        let h = left.len().max(right.len()) as u16;
        (vec![left, right], h)
    } else {
        left.push(Line::default());
        left.extend(right);
        let h = left.len() as u16;
        (vec![left], h)
    };

    let width = if two_cols { 96u16 } else { 52u16 }.min(area.width.saturating_sub(2));
    // Repères de largeur : à 80 colonnes, chaque colonne garde 26 caractères
    // pour la description (les libellés de `keymap` s'y tiennent).
    let height = (content_h + 2).min(area.height.saturating_sub(1)).max(5);
    let popup = Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_active))
        .title(Span::styled(
            format!(" {} ", i18n::help_title(lang)),
            key_style,
        ))
        .title_bottom(
            Line::from(Span::styled(
                format!(" {} ", i18n::help_close(lang)),
                Style::default().fg(theme.dim),
            ))
            .right_aligned(),
        );
    let inner = block.inner(popup);
    f.render_widget(Clear, popup);
    f.render_widget(block, popup);

    let n = columns.len() as u32;
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, n); n as usize])
        .split(inner);
    for (col, lines) in columns.into_iter().enumerate() {
        let r = Rect::new(
            cols[col].x + 1,
            cols[col].y,
            cols[col].width.saturating_sub(1),
            cols[col].height,
        );
        f.render_widget(Paragraph::new(lines), r);
    }
}

/// Bloc encadré standard, surligné quand le panneau a le focus.
fn panel_block(title: &str, active: bool, theme: &Theme) -> Block<'static> {
    let border = if active {
        theme.border_active
    } else {
        theme.border
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(if active { theme.accent } else { theme.fg })
                .add_modifier(Modifier::BOLD),
        ))
}

/// Détermine le premier index visible pour garder la sélection à l'écran.
fn scroll_first(selected: usize, total: usize, height: usize) -> usize {
    if total <= height || height == 0 {
        return 0;
    }
    let half = height / 2;
    let max_first = total - height;
    selected.saturating_sub(half).min(max_first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_garde_la_selection_visible() {
        // En haut : pas de décalage.
        assert_eq!(scroll_first(0, 100, 10), 0);
        // Au milieu : centré.
        assert_eq!(scroll_first(50, 100, 10), 45);
        // En bas : borné à max_first.
        assert_eq!(scroll_first(99, 100, 10), 90);
        // Liste plus courte que la fenêtre.
        assert_eq!(scroll_first(3, 5, 10), 0);
    }

    #[test]
    fn contains_teste_les_bornes() {
        let r = Rect::new(2, 3, 4, 2);
        assert!(contains(&r, 2, 3));
        assert!(contains(&r, 5, 4));
        assert!(!contains(&r, 6, 3));
        assert!(!contains(&r, 2, 5));
    }

    /// Concatène tous les symboles du buffer en une chaîne pour les assertions.
    fn buffer_text(term: &ratatui::Terminal<ratatui::backend::TestBackend>) -> String {
        term.backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn rend_les_zones_principales() {
        use crate::app::App;
        use crate::model::{Platform, Track};
        use crate::theme::Theme;

        let mut app = App::new();
        app.restore(
            Vec::new(),
            Vec::new(),
            vec![Track {
                platform: Platform::SoundCloud,
                id: "1".into(),
                title: "Mon Morceau Test".into(),
                artist: "Artiste Test".into(),
                permalink: "https://soundcloud.com/a/b".into(),
                duration_ms: Some(200_000),
            }],
        );
        let theme = Theme::dark();

        let backend = ratatui::backend::TestBackend::new(110, 30);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        term.draw(|f| {
            super::draw(f, &app, &theme);
        })
        .unwrap();

        let text = buffer_text(&term);
        assert!(text.contains("waveline"), "titre absent");
        assert!(text.contains("Sources"), "sidebar absente");
        assert!(text.contains("Likes"), "section Likes absente");
        assert!(text.contains("Mon Morceau Test"), "morceau absent");
        assert!(text.contains("Nothing playing"), "barre de lecture absente");
    }

    #[test]
    fn la_fenetre_d_aide_liste_les_raccourcis() {
        use crate::app::{Action, App};
        use crate::theme::Theme;
        let mut app = App::new();
        app.apply(Action::ToggleHelp);
        let theme = Theme::dark();
        let backend = ratatui::backend::TestBackend::new(100, 40);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        term.draw(|f| {
            super::draw(f, &app, &theme);
        })
        .unwrap();
        let text = buffer_text(&term);
        assert!(text.contains("Help"), "titre de l'aide absent");
        assert!(text.contains("Navigate"), "rubrique absente");
        assert!(
            text.contains("add selection to queue"),
            "raccourci 'a' absent"
        );
        assert!(text.contains("space"), "touche espace absente");
    }

    #[test]
    fn la_file_affiche_son_compteur_et_les_zones_souris_sont_posees() {
        use crate::app::App;
        use crate::model::{Platform, Track};
        use crate::theme::Theme;
        let t = Track {
            platform: Platform::Mixcloud,
            id: "q".into(),
            title: "Queued".into(),
            artist: "A".into(),
            permalink: "https://www.mixcloud.com/a/q/".into(),
            duration_ms: Some(1_000),
        };
        let mut app = App::new();
        app.restore(vec![t.clone(), t], Vec::new(), Vec::new());
        app.sync_current(Some(Track {
            platform: Platform::SoundCloud,
            id: "now".into(),
            title: "Now".into(),
            artist: "B".into(),
            permalink: "https://soundcloud.com/b/now".into(),
            duration_ms: Some(100_000),
        }));
        let theme = Theme::dark();
        let backend = ratatui::backend::TestBackend::new(110, 30);
        let mut term = ratatui::Terminal::new(backend).unwrap();
        let mut regions = Regions::default();
        term.draw(|f| {
            regions = super::draw(f, &app, &theme);
        })
        .unwrap();
        let text = buffer_text(&term);
        assert!(
            text.contains("Queue (2)"),
            "compteur de file absent: {text}"
        );
        assert!(
            regions.progress.width > 10,
            "barre de progression non posée"
        );
        assert!(regions.playbar.height > 0);
        assert!(regions.accounts_btn.height == 1);
        // Un clic au milieu du trait donne un ratio ≈ 0,5.
        let mid_x = regions.progress.x + regions.progress.width / 2;
        let r = regions
            .progress_ratio_at(mid_x, regions.progress.y)
            .unwrap();
        assert!((0.4..=0.6).contains(&r), "ratio {r}");
        // Hors du trait (sur le libellé) : aucun saut.
        assert!(regions
            .progress_ratio_at(regions.progress.x - 1, regions.progress.y)
            .is_none());
    }

    #[test]
    fn l_aide_ne_panique_pas_a_la_taille_minimale_dessinable() {
        use crate::app::{Action, App};
        use crate::theme::Theme;
        let mut app = App::new();
        app.apply(Action::ToggleHelp);
        let theme = Theme::dark();
        for (w, h) in [(24, 14), (30, 15), (79, 20), (80, 24), (200, 60)] {
            let backend = ratatui::backend::TestBackend::new(w, h);
            let mut term = ratatui::Terminal::new(backend).unwrap();
            term.draw(|f| {
                super::draw(f, &app, &theme);
            })
            .unwrap();
        }
    }

    #[test]
    fn le_spinner_tourne_avec_le_temps() {
        assert_eq!(spinner_frame(0), SPINNER[0]);
        assert_eq!(spinner_frame(80), SPINNER[1]);
        assert_eq!(spinner_frame(80 * 10), SPINNER[0]);
    }

    #[test]
    fn ne_panique_pas_sur_terminal_minuscule() {
        use crate::app::App;
        use crate::theme::Theme;
        let app = App::new();
        let theme = Theme::dark();
        for (w, h) in [(0, 0), (1, 1), (5, 3), (19, 7)] {
            let backend = ratatui::backend::TestBackend::new(w.max(1), h.max(1));
            let mut term = ratatui::Terminal::new(backend).unwrap();
            // Ne doit pas paniquer même en taille dégénérée.
            term.draw(|f| {
                super::draw(f, &app, &theme);
            })
            .unwrap();
        }
    }
}
