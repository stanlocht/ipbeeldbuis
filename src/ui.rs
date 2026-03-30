use crate::cache::PlaylistEntry;
use crate::m3u::{Channel, ContentType};
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::io::{self, Stdout};

pub type Term = Terminal<CrosstermBackend<Stdout>>;

pub fn setup_terminal() -> Result<Term> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

pub fn restore_terminal(terminal: &mut Term) {
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
}

const ACCENT: Color = Color::Rgb(120, 200, 255);
const DIM: Color = Color::Rgb(100, 100, 120);
const GROUP_COLOR: Color = Color::Rgb(180, 140, 255);
const BG: Color = Color::Rgb(10, 10, 18);
const HIGHLIGHT_BG: Color = Color::Rgb(25, 40, 65);

pub enum Action {
    Play(Channel),
    Quit,
    AddPlaylist,
    OpenSettings,
}

// ─── Content filter ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum ContentFilter {
    All,
    Live,
    Movie,
    Series,
}

impl ContentFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Live => "Live Channels",
            Self::Movie => "Movies",
            Self::Series => "Series",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::All => Self::Live,
            Self::Live => Self::Movie,
            Self::Movie => Self::Series,
            Self::Series => Self::All,
        }
    }

    fn matches(self, ct: &ContentType) -> bool {
        match self {
            Self::All => true,
            Self::Live => *ct == ContentType::Live,
            Self::Movie => *ct == ContentType::Movie,
            Self::Series => *ct == ContentType::Series,
        }
    }
}

// ─── Main app state ───────────────────────────────────────────────────────────

struct AppState<'a> {
    all_channels: &'a [Channel],
    groups: Vec<String>,
    group_idx: usize,
    tab_offset: usize,
    filtered: Vec<usize>,
    list_state: ListState,
    search: String,
    search_mode: bool,
    content_filter: ContentFilter,
}

impl<'a> AppState<'a> {
    fn new(channels: &'a [Channel]) -> Self {
        let mut state = Self {
            all_channels: channels,
            groups: Vec::new(),
            group_idx: 0,
            tab_offset: 0,
            filtered: Vec::new(),
            list_state: ListState::default(),
            search: String::new(),
            search_mode: false,
            content_filter: ContentFilter::All,
        };
        state.rebuild_groups();
        state
    }

    fn rebuild_groups(&mut self) {
        let mut groups = vec!["All".to_string()];
        let mut seen = std::collections::HashSet::new();
        for ch in self.all_channels {
            if self.content_filter.matches(&ch.content_type) {
                let g = ch.display_group().to_string();
                if seen.insert(g.clone()) {
                    groups.push(g);
                }
            }
        }
        self.groups = groups;
        self.group_idx = 0;
        self.tab_offset = 0;
        self.refresh_filter();
    }

    fn refresh_filter(&mut self) {
        let group = self.groups[self.group_idx].clone();
        let query = self.search.to_lowercase();

        self.filtered = self
            .all_channels
            .iter()
            .enumerate()
            .filter(|(_, ch)| {
                let content_ok = self.content_filter.matches(&ch.content_type);
                let group_ok = group == "All" || ch.display_group() == group;
                let search_ok = query.is_empty()
                    || ch.name.to_lowercase().contains(&query)
                    || ch.display_group().to_lowercase().contains(&query);
                content_ok && group_ok && search_ok
            })
            .map(|(i, _)| i)
            .collect();

        if self.filtered.is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(0));
        }
    }

    fn selected_channel(&self) -> Option<&Channel> {
        self.list_state
            .selected()
            .and_then(|i| self.filtered.get(i))
            .map(|&idx| &self.all_channels[idx])
    }

    fn next(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state
            .select(Some((i + 1).min(self.filtered.len() - 1)));
    }

    fn prev(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some(i.saturating_sub(1)));
    }

    fn next_group(&mut self) {
        self.group_idx = (self.group_idx + 1) % self.groups.len();
        self.refresh_filter();
    }

    fn prev_group(&mut self) {
        self.group_idx = self
            .group_idx
            .checked_sub(1)
            .unwrap_or(self.groups.len() - 1);
        self.refresh_filter();
    }

    fn cycle_content_filter(&mut self) {
        self.content_filter = self.content_filter.next();
        self.search.clear();
        self.search_mode = false;
        self.rebuild_groups();
    }
}

// ─── Main TUI entry point ─────────────────────────────────────────────────────

pub fn run(terminal: &mut Term, channels: &[Channel]) -> Result<Action> {
    let mut app = AppState::new(channels);
    event_loop(terminal, &mut app)
}

fn event_loop(terminal: &mut Term, app: &mut AppState) -> Result<Action> {
    loop {
        terminal.draw(|f| draw(f, app))?;

        if let Event::Key(key) = event::read()? {
            if app.search_mode {
                match key.code {
                    KeyCode::Esc => {
                        app.search_mode = false;
                        app.search.clear();
                        app.refresh_filter();
                    }
                    KeyCode::Backspace => {
                        app.search.pop();
                        app.refresh_filter();
                    }
                    KeyCode::Char(c) => {
                        app.search.push(c);
                        app.refresh_filter();
                    }
                    KeyCode::Enter => {
                        app.search_mode = false;
                    }
                    KeyCode::Down => app.next(),
                    KeyCode::Up => app.prev(),
                    _ => {}
                }
            } else {
                match (key.modifiers, key.code) {
                    (_, KeyCode::Char('q')) | (_, KeyCode::Esc) => return Ok(Action::Quit),
                    (KeyModifiers::CONTROL, KeyCode::Char('c')) => return Ok(Action::Quit),
                    (_, KeyCode::Down) | (_, KeyCode::Char('j')) => app.next(),
                    (_, KeyCode::Up) | (_, KeyCode::Char('k')) => app.prev(),
                    (_, KeyCode::Right) | (_, KeyCode::Char('l')) | (_, KeyCode::Tab) => {
                        app.next_group()
                    }
                    (_, KeyCode::Left)
                    | (_, KeyCode::Char('h'))
                    | (KeyModifiers::SHIFT, KeyCode::BackTab) => app.prev_group(),
                    (_, KeyCode::Char('/')) => {
                        app.search_mode = true;
                    }
                    (_, KeyCode::Char('t')) => {
                        app.cycle_content_filter();
                    }
                    (_, KeyCode::Char('s')) => return Ok(Action::OpenSettings),
                    (_, KeyCode::Char('a')) => return Ok(Action::AddPlaylist),
                    (_, KeyCode::Enter) => {
                        if let Some(ch) = app.selected_channel() {
                            return Ok(Action::Play(ch.clone()));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

// ─── Settings TUI ─────────────────────────────────────────────────────────────

struct SettingsState {
    selected: usize,
}

pub fn run_settings(terminal: &mut Term, playlists: &mut Vec<PlaylistEntry>) -> Result<()> {
    settings_loop(terminal, playlists)
}

fn settings_loop(terminal: &mut Term, playlists: &mut Vec<PlaylistEntry>) -> Result<()> {
    let mut state = SettingsState { selected: 0 };

    loop {
        terminal.draw(|f| draw_settings(f, playlists, &state))?;

        if let Event::Key(key) = event::read()? {
            match (key.modifiers, key.code) {
                (_, KeyCode::Char('q')) | (_, KeyCode::Esc) => return Ok(()),
                (KeyModifiers::CONTROL, KeyCode::Char('c')) => return Ok(()),
                (_, KeyCode::Down) | (_, KeyCode::Char('j')) => {
                    if !playlists.is_empty() {
                        state.selected =
                            (state.selected + 1).min(playlists.len().saturating_sub(1));
                    }
                }
                (_, KeyCode::Up) | (_, KeyCode::Char('k')) => {
                    state.selected = state.selected.saturating_sub(1);
                }
                (_, KeyCode::Char('d')) => {
                    if !playlists.is_empty() {
                        playlists.remove(state.selected);
                        if state.selected > 0 && state.selected >= playlists.len() {
                            state.selected -= 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn draw_settings(f: &mut Frame, playlists: &[PlaylistEntry], state: &SettingsState) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BG)), area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    // Title
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " Settings",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" — Playlists", Style::default().fg(DIM)),
        ]))
        .style(Style::default().bg(BG)),
        layout[0],
    );

    // Playlist list
    let inner_w = area.width.saturating_sub(8) as usize;
    let mut lines: Vec<Line> = vec![Line::raw("")];

    if playlists.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No saved playlists.",
            Style::default().fg(DIM),
        )));
    } else {
        for (i, entry) in playlists.iter().enumerate() {
            let is_sel = i == state.selected;
            let bg = if is_sel { HIGHLIGHT_BG } else { BG };
            let prefix = if is_sel { "▶ " } else { "  " };

            lines.push(Line::from(vec![Span::styled(
                format!("  {prefix}{}", entry.name),
                if is_sel {
                    Style::default()
                        .fg(ACCENT)
                        .add_modifier(Modifier::BOLD)
                        .bg(bg)
                } else {
                    Style::default().fg(Color::White).bg(bg)
                },
            )]));
            lines.push(Line::from(vec![
                Span::styled("    URL  ", Style::default().fg(DIM).bg(bg)),
                Span::styled(
                    truncate(&entry.url, inner_w),
                    Style::default().fg(Color::White).bg(bg),
                ),
            ]));
            lines.push(Line::raw(""));
        }
    }

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(DIM))
                .style(Style::default().bg(BG))
                .title(Span::styled(" Playlists ", Style::default().fg(ACCENT))),
        ),
        layout[1],
    );

    f.render_widget(
        Paragraph::new(" ↑↓/jk navigate   d delete   Esc/q back")
            .style(Style::default().fg(DIM).bg(BG))
            .alignment(Alignment::Center),
        layout[2],
    );
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let end = s
            .char_indices()
            .nth(max_chars.saturating_sub(1))
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        format!("{}…", &s[..end])
    }
}

// ─── Main draw ────────────────────────────────────────────────────────────────

fn adjust_tab_offset(app: &mut AppState, avail_w: usize) {
    if app.group_idx < app.tab_offset {
        app.tab_offset = app.group_idx;
        return;
    }

    loop {
        if app.tab_offset >= app.groups.len() {
            app.tab_offset = app.groups.len().saturating_sub(1);
            break;
        }

        let left_w: usize = if app.tab_offset > 0 { 2 } else { 0 };
        let mut used_w = left_w;
        let mut last_vis = app.tab_offset;

        for i in app.tab_offset..app.groups.len() {
            let tab_w = app.groups[i].len() + 2;
            let div_w = if i + 1 < app.groups.len() { 1 } else { 0 };
            let right_w = if i + 1 < app.groups.len() { 2 } else { 0 };

            if used_w + tab_w + div_w + right_w <= avail_w || i == app.tab_offset {
                used_w += tab_w + div_w;
                last_vis = i + 1;
            } else {
                break;
            }
        }

        if app.group_idx < last_vis {
            break;
        }

        app.tab_offset += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::m3u::{Channel, ContentType};

    fn make_channel(name: &str, group: &str, ct: ContentType) -> Channel {
        Channel {
            name: name.to_string(),
            url: format!("http://example.com/{name}"),
            group: group.to_string(),
            logo: None,
            tvg_id: None,
            content_type: ct,
        }
    }

    fn sample_channels() -> Vec<Channel> {
        vec![
            make_channel("CNN", "News", ContentType::Live),
            make_channel("BBC News", "News", ContentType::Live),
            make_channel("Inception", "Movies", ContentType::Movie),
            make_channel("Breaking Bad S01E01", "US Series", ContentType::Series),
            make_channel("ESPN", "Sports", ContentType::Live),
        ]
    }

    // ── ContentFilter ────────────────────────────────────────────────────────

    #[test]
    fn content_filter_labels() {
        assert_eq!(ContentFilter::All.label(), "All");
        assert_eq!(ContentFilter::Live.label(), "Live Channels");
        assert_eq!(ContentFilter::Movie.label(), "Movies");
        assert_eq!(ContentFilter::Series.label(), "Series");
    }

    #[test]
    fn content_filter_next_cycles_all_variants() {
        assert_eq!(ContentFilter::All.next(), ContentFilter::Live);
        assert_eq!(ContentFilter::Live.next(), ContentFilter::Movie);
        assert_eq!(ContentFilter::Movie.next(), ContentFilter::Series);
        assert_eq!(ContentFilter::Series.next(), ContentFilter::All);
    }

    #[test]
    fn content_filter_matches_all_accepts_any() {
        assert!(ContentFilter::All.matches(&ContentType::Live));
        assert!(ContentFilter::All.matches(&ContentType::Movie));
        assert!(ContentFilter::All.matches(&ContentType::Series));
    }

    #[test]
    fn content_filter_matches_live_only() {
        assert!(ContentFilter::Live.matches(&ContentType::Live));
        assert!(!ContentFilter::Live.matches(&ContentType::Movie));
        assert!(!ContentFilter::Live.matches(&ContentType::Series));
    }

    #[test]
    fn content_filter_matches_movie_only() {
        assert!(ContentFilter::Movie.matches(&ContentType::Movie));
        assert!(!ContentFilter::Movie.matches(&ContentType::Live));
        assert!(!ContentFilter::Movie.matches(&ContentType::Series));
    }

    #[test]
    fn content_filter_matches_series_only() {
        assert!(ContentFilter::Series.matches(&ContentType::Series));
        assert!(!ContentFilter::Series.matches(&ContentType::Live));
        assert!(!ContentFilter::Series.matches(&ContentType::Movie));
    }

    // ── AppState::new ────────────────────────────────────────────────────────

    #[test]
    fn app_state_new_empty_channels() {
        let channels: Vec<Channel> = vec![];
        let app = AppState::new(&channels);
        assert_eq!(app.filtered.len(), 0);
        // groups should at least contain "All"
        assert!(app.groups.contains(&"All".to_string()));
    }

    #[test]
    fn app_state_new_with_channels_selects_first() {
        let channels = sample_channels();
        let app = AppState::new(&channels);
        assert_eq!(app.filtered.len(), channels.len());
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn app_state_new_builds_groups_from_channels() {
        let channels = sample_channels();
        let app = AppState::new(&channels);
        // "All" + unique groups
        assert!(app.groups.contains(&"All".to_string()));
        assert!(app.groups.contains(&"News".to_string()));
        assert!(app.groups.contains(&"Movies".to_string()));
        assert!(app.groups.contains(&"US Series".to_string()));
        assert!(app.groups.contains(&"Sports".to_string()));
    }

    // ── AppState navigation ──────────────────────────────────────────────────

    #[test]
    fn app_state_next_advances_selection() {
        let channels = sample_channels();
        let mut app = AppState::new(&channels);
        app.next();
        assert_eq!(app.list_state.selected(), Some(1));
    }

    #[test]
    fn app_state_next_stops_at_last_item() {
        let channels = sample_channels();
        let mut app = AppState::new(&channels);
        for _ in 0..100 {
            app.next();
        }
        assert_eq!(app.list_state.selected(), Some(channels.len() - 1));
    }

    #[test]
    fn app_state_prev_stops_at_zero() {
        let channels = sample_channels();
        let mut app = AppState::new(&channels);
        app.prev();
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn app_state_prev_goes_back() {
        let channels = sample_channels();
        let mut app = AppState::new(&channels);
        app.next();
        app.next();
        app.prev();
        assert_eq!(app.list_state.selected(), Some(1));
    }

    #[test]
    fn app_state_next_on_empty_list_does_nothing() {
        let channels: Vec<Channel> = vec![];
        let mut app = AppState::new(&channels);
        app.next(); // should not panic
        assert_eq!(app.list_state.selected(), None);
    }

    #[test]
    fn app_state_prev_on_empty_list_does_nothing() {
        let channels: Vec<Channel> = vec![];
        let mut app = AppState::new(&channels);
        app.prev(); // should not panic
        assert_eq!(app.list_state.selected(), None);
    }

    // ── Group navigation ─────────────────────────────────────────────────────

    #[test]
    fn app_state_next_group_advances() {
        let channels = sample_channels();
        let mut app = AppState::new(&channels);
        assert_eq!(app.group_idx, 0); // starts at "All"
        app.next_group();
        assert_eq!(app.group_idx, 1);
    }

    #[test]
    fn app_state_next_group_wraps_around() {
        let channels = sample_channels();
        let mut app = AppState::new(&channels);
        let total_groups = app.groups.len();
        for _ in 0..total_groups {
            app.next_group();
        }
        assert_eq!(app.group_idx, 0);
    }

    #[test]
    fn app_state_prev_group_wraps_around() {
        let channels = sample_channels();
        let mut app = AppState::new(&channels);
        app.prev_group();
        assert_eq!(app.group_idx, app.groups.len() - 1);
    }

    #[test]
    fn app_state_group_filter_restricts_channels() {
        let channels = sample_channels();
        let mut app = AppState::new(&channels);
        // Navigate to the "News" group
        let news_idx = app
            .groups
            .iter()
            .position(|g| g == "News")
            .expect("News group should exist");
        app.group_idx = news_idx;
        app.refresh_filter();
        // Only CNN and BBC News are in "News"
        assert_eq!(app.filtered.len(), 2);
        let names: Vec<&str> = app
            .filtered
            .iter()
            .map(|&i| channels[i].name.as_str())
            .collect();
        assert!(names.contains(&"CNN"));
        assert!(names.contains(&"BBC News"));
    }

    // ── Search filtering ─────────────────────────────────────────────────────

    #[test]
    fn app_state_search_filters_by_name() {
        let channels = sample_channels();
        let mut app = AppState::new(&channels);
        app.search = "bbc".to_string();
        app.refresh_filter();
        assert_eq!(app.filtered.len(), 1);
        assert_eq!(channels[app.filtered[0]].name, "BBC News");
    }

    #[test]
    fn app_state_search_filters_by_group() {
        let channels = sample_channels();
        let mut app = AppState::new(&channels);
        app.search = "news".to_string();
        app.refresh_filter();
        // CNN, BBC News (group "News"), and "BBC News" (name contains news)
        // Both channels in the "News" group match, plus ESPN doesn't match
        let names: Vec<&str> = app
            .filtered
            .iter()
            .map(|&i| channels[i].name.as_str())
            .collect();
        assert!(names.contains(&"CNN"));
        assert!(names.contains(&"BBC News"));
    }

    #[test]
    fn app_state_search_empty_shows_all() {
        let channels = sample_channels();
        let mut app = AppState::new(&channels);
        app.search = String::new();
        app.refresh_filter();
        assert_eq!(app.filtered.len(), channels.len());
    }

    #[test]
    fn app_state_search_no_match_gives_empty_selection() {
        let channels = sample_channels();
        let mut app = AppState::new(&channels);
        app.search = "zzznomatch".to_string();
        app.refresh_filter();
        assert!(app.filtered.is_empty());
        assert_eq!(app.list_state.selected(), None);
    }

    // ── Content filter cycling ───────────────────────────────────────────────

    #[test]
    fn cycle_content_filter_changes_filter() {
        let channels = sample_channels();
        let mut app = AppState::new(&channels);
        assert_eq!(app.content_filter, ContentFilter::All);
        app.cycle_content_filter();
        assert_eq!(app.content_filter, ContentFilter::Live);
    }

    #[test]
    fn cycle_content_filter_restricts_channels() {
        let channels = sample_channels();
        let mut app = AppState::new(&channels);
        // Skip to Movie filter
        app.cycle_content_filter(); // Live
        app.cycle_content_filter(); // Movie
        assert_eq!(app.content_filter, ContentFilter::Movie);
        assert_eq!(app.filtered.len(), 1);
        assert_eq!(channels[app.filtered[0]].name, "Inception");
    }

    #[test]
    fn cycle_content_filter_clears_search() {
        let channels = sample_channels();
        let mut app = AppState::new(&channels);
        app.search = "cnn".to_string();
        app.search_mode = true;
        app.cycle_content_filter();
        assert!(app.search.is_empty());
        assert!(!app.search_mode);
    }

    // ── selected_channel ─────────────────────────────────────────────────────

    #[test]
    fn selected_channel_returns_correct_channel() {
        let channels = sample_channels();
        let app = AppState::new(&channels);
        let selected = app.selected_channel().unwrap();
        assert_eq!(selected.name, channels[0].name);
    }

    #[test]
    fn selected_channel_returns_none_when_empty() {
        let channels: Vec<Channel> = vec![];
        let app = AppState::new(&channels);
        assert!(app.selected_channel().is_none());
    }

    // ── truncate ─────────────────────────────────────────────────────────────

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length_unchanged() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_string_gets_ellipsis() {
        let result = truncate("hello world", 5);
        assert!(result.ends_with('…'));
        assert!(result.chars().count() <= 6); // 5 chars + ellipsis
    }

    #[test]
    fn truncate_empty_string() {
        assert_eq!(truncate("", 5), "");
    }

    #[test]
    fn truncate_handles_unicode() {
        // "héllo" is 5 chars
        let result = truncate("héllo world", 5);
        assert!(result.ends_with('…'));
    }
}

fn draw(f: &mut Frame, app: &mut AppState) {
    let area = f.area();

    f.render_widget(Block::default().style(Style::default().bg(BG)), area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    // Title bar
    let count = app.filtered.len();
    let total = app.all_channels.len();
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " ipbeeldbuis",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  [{}]", app.content_filter.label()),
                Style::default().fg(GROUP_COLOR),
            ),
            Span::styled(
                format!("  {count}/{total} channels"),
                Style::default().fg(DIM),
            ),
        ]))
        .style(Style::default().bg(BG)),
        layout[0],
    );

    // Scrollable group tabs
    let tab_area = layout[1];
    let avail_w = tab_area.width as usize;
    adjust_tab_offset(app, avail_w);

    let has_left = app.tab_offset > 0;
    let left_w: usize = if has_left { 2 } else { 0 };
    let mut used_w = left_w;
    let mut last_vis = app.tab_offset;

    for i in app.tab_offset..app.groups.len() {
        let tab_w = app.groups[i].len() + 2;
        let div_w = if i + 1 < app.groups.len() { 1 } else { 0 };
        let right_w = if i + 1 < app.groups.len() { 2 } else { 0 };
        if used_w + tab_w + div_w + right_w <= avail_w || i == app.tab_offset {
            used_w += tab_w + div_w;
            last_vis = i + 1;
        } else {
            break;
        }
    }

    let has_right = last_vis < app.groups.len();
    let mut tab_spans: Vec<Span> = Vec::new();
    if has_left {
        tab_spans.push(Span::styled("◀ ", Style::default().fg(DIM)));
    }
    for i in app.tab_offset..last_vis {
        let name = app.groups[i].clone();
        let is_selected = i == app.group_idx;
        let style = if is_selected {
            Style::default()
                .fg(ACCENT)
                .add_modifier(Modifier::BOLD)
                .bg(HIGHLIGHT_BG)
        } else {
            Style::default().fg(DIM)
        };
        tab_spans.push(Span::styled(format!(" {name} "), style));
        if i + 1 < last_vis {
            tab_spans.push(Span::styled("│", Style::default().fg(DIM)));
        }
    }
    if has_right {
        tab_spans.push(Span::styled(" ▶", Style::default().fg(DIM)));
    }

    f.render_widget(
        Paragraph::new(Line::from(tab_spans)).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(DIM))
                .style(Style::default().bg(BG)),
        ),
        tab_area,
    );

    // Search bar
    let search_content = if app.search.is_empty() && !app.search_mode {
        Span::styled("press / to search", Style::default().fg(DIM))
    } else {
        Span::styled(app.search.as_str(), Style::default().fg(Color::White))
    };
    let cursor = if app.search_mode {
        Span::styled("█", Style::default().fg(ACCENT))
    } else {
        Span::raw("")
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Search: ", Style::default().fg(DIM)),
            search_content,
            cursor,
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if app.search_mode { ACCENT } else { DIM }))
                .style(Style::default().bg(BG)),
        ),
        layout[2],
    );

    let list_area = layout[3];

    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .map(|&idx| {
            let ch = &app.all_channels[idx];
            ListItem::new(Line::from(vec![
                Span::raw(" "),
                Span::styled(ch.name.clone(), Style::default().fg(Color::White)),
                Span::styled(
                    format!("  [{}]", ch.display_group()),
                    Style::default().fg(GROUP_COLOR),
                ),
            ]))
        })
        .collect();

    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(DIM))
                    .style(Style::default().bg(BG))
                    .title(Span::styled(" Channels ", Style::default().fg(ACCENT))),
            )
            .highlight_style(
                Style::default()
                    .bg(HIGHLIGHT_BG)
                    .fg(ACCENT)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ "),
        list_area,
        &mut app.list_state,
    );

    // Keybinding hints
    let hints = if app.search_mode {
        " ↑↓ navigate   Esc clear   Enter confirm"
    } else {
        " ↑↓/jk navigate   ←→/hl tabs   / search   t content   s settings   a add   Enter play   q quit"
    };
    f.render_widget(
        Paragraph::new(hints)
            .style(Style::default().fg(DIM).bg(BG))
            .alignment(Alignment::Center),
        layout[4],
    );
}
