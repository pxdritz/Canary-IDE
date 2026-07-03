use std::{
    collections::BTreeSet,
    env,
    io::{self, stdout},
    process::Command,
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Editor,
    Commit,
    Terminal,
    Ai,
    Extensions,
    Themes,
}

impl View {
    fn all() -> [View; 6] {
        [
            View::Editor,
            View::Commit,
            View::Terminal,
            View::Ai,
            View::Extensions,
            View::Themes,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            View::Editor => "Editor",
            View::Commit => "Commit",
            View::Terminal => "Terminal",
            View::Ai => "IA",
            View::Extensions => "Extensões",
            View::Themes => "Temas",
        }
    }
}

#[derive(Clone, Debug)]
struct Extension {
    name: &'static str,
    description: &'static str,
    enabled: bool,
}

#[derive(Clone, Debug)]
struct Theme {
    name: &'static str,
    accent: Color,
    panel: Color,
    surface: Color,
    text: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            name: "Canary",
            accent: Color::Cyan,
            panel: Color::Rgb(0x1f, 0x2a, 0x3d),
            surface: Color::Rgb(0x0f, 0x14, 0x1f),
            text: Color::White,
        }
    }
}

impl Theme {
    fn presets() -> Vec<Self> {
        vec![
            Self {
                name: "Canary",
                accent: Color::Cyan,
                panel: Color::Rgb(0x1f, 0x2a, 0x3d),
                surface: Color::Rgb(0x0f, 0x14, 0x1f),
                text: Color::White,
            },
            Self {
                name: "Nord",
                accent: Color::LightBlue,
                panel: Color::Rgb(0x2e, 0x34, 0x40),
                surface: Color::Rgb(0x3b, 0x42, 0x52),
                text: Color::Rgb(0xE5, 0xE9, 0xF0),
            },
            Self {
                name: "Solar",
                accent: Color::Yellow,
                panel: Color::Rgb(0x2d, 0x1b, 0x0b),
                surface: Color::Rgb(0x3d, 0x24, 0x12),
                text: Color::Rgb(0xFF, 0xF2, 0xCC),
            },
        ]
    }

    fn next(&mut self) {
        let presets = Self::presets();
        let current = presets.iter().position(|p| p.name == self.name).unwrap_or(0);
        let next = (current + 1) % presets.len();
        let preset = presets[next].clone();
        *self = preset;
    }

    fn previous(&mut self) {
        let presets = Self::presets();
        let current = presets.iter().position(|p| p.name == self.name).unwrap_or(0);
        let prev = if current == 0 { presets.len() - 1 } else { current - 1 };
        let preset = presets[prev].clone();
        *self = preset;
    }
}

#[derive(Debug)]
struct App {
    current_view: View,
    buffer_chars: Vec<char>,
    cursor: usize,
    commit_message: String,
    terminal_input: String,
    terminal_output: Vec<String>,
    ai_prompt: String,
    ai_response: String,
    extensions: Vec<Extension>,
    theme: Theme,
    git_status: String,
    status_line: String,
    learned_words: BTreeSet<String>,
}

impl Default for App {
    fn default() -> Self {
        let mut app = Self {
            current_view: View::Editor,
            buffer_chars: "fn main() {\n    println!(\"Hello from Canary IDE\");\n}\n".chars().collect(),
            cursor: 0,
            commit_message: String::new(),
            terminal_input: String::new(),
            terminal_output: vec![
                String::from("Canary terminal ready."),
                String::from("Type commands like 'git status' or 'ls'."),
            ],
            ai_prompt: String::new(),
            ai_response: String::from("Configure CANARY_AI_API_URL and CANARY_AI_API_TOKEN to enable requests."),
            extensions: vec![
                Extension { name: "zed-rust-analyzer", description: "Suporte inteligente para Rust", enabled: true },
                Extension { name: "zed-git-graph", description: "Visualização de commits", enabled: true },
                Extension { name: "zed-terminal-themes", description: "Temas para shell", enabled: false },
            ],
            theme: Theme::default(),
            git_status: String::new(),
            status_line: String::from("Canary IDE • terminal-first • leve e customizável"),
            learned_words: BTreeSet::new(),
        };
        app.refresh_learned_words();
        app
    }
}

impl App {
    fn refresh_learned_words(&mut self) {
        let mut words = BTreeSet::new();
        for word in self.buffer_text().split(|c: char| !c.is_alphanumeric() && c != '_') {
            if !word.is_empty() {
                words.insert(word.to_lowercase());
            }
        }
        for keyword in ["fn", "let", "mut", "struct", "impl", "match", "if", "else", "for", "while", "loop", "use", "mod", "pub", "crate", "println"] {
            words.insert(keyword.to_string());
        }
        self.learned_words = words;
    }

    fn buffer_text(&self) -> String {
        self.buffer_chars.iter().collect()
    }

    fn insert_char(&mut self, ch: char) {
        self.buffer_chars.insert(self.cursor, ch);
        self.cursor += 1;
        self.refresh_learned_words();
    }

    fn remove_char(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.buffer_chars.remove(self.cursor);
            self.refresh_learned_words();
        }
    }

    fn new_line(&mut self) {
        self.insert_char('\n');
    }

    fn current_word(&self) -> String {
        let text: String = self.buffer_chars.iter().collect();
        let start = text[..self.cursor].rfind(|c: char| c.is_whitespace()).map_or(0, |idx| idx + 1);
        let end = self.cursor;
        text[start..end].to_string()
    }

    fn autocomplete(&mut self) {
        let prefix = self.current_word();
        let mut candidates: Vec<String> = self
            .learned_words
            .iter()
            .filter(|word| word.starts_with(&prefix.to_lowercase()))
            .cloned()
            .collect();
        candidates.sort();
        if let Some(best) = candidates.first() {
            let replacement = best.clone();
            let current = self.current_word();
            if !current.is_empty() {
                let text: String = self.buffer_chars.iter().collect();
                let start = text[..self.cursor].rfind(|c: char| c.is_whitespace()).map_or(0, |idx| idx + 1);
                let mut new_text = String::new();
                new_text.push_str(&text[..start]);
                new_text.push_str(&replacement);
                new_text.push_str(&text[self.cursor..]);
                self.buffer_chars = new_text.chars().collect();
                self.cursor = start + replacement.len();
                self.refresh_learned_words();
            }
        }
    }

    fn run_git_status(&mut self) {
        if let Ok(output) = Command::new("git").args(["status", "--short"]).output() {
            self.git_status = String::from_utf8_lossy(&output.stdout).to_string();
        } else {
            self.git_status = String::from("git não disponível ou não há repositório ativo.");
        }
    }

    fn commit_changes(&mut self) {
        if self.commit_message.trim().is_empty() {
            self.status_line = String::from("Escreva uma mensagem de commit antes de enviar.");
            return;
        }
        let _ = Command::new("git").args(["add", "-A"]).status();
        let outcome = Command::new("git")
            .args(["commit", "-m", &self.commit_message])
            .output();
        match outcome {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                self.status_line = if output.status.success() {
                    String::from("Commit criado com sucesso.")
                } else {
                    format!("Commit falhou: {stderr}")
                };
                self.terminal_output.push(stdout.trim().to_string());
                self.terminal_output.push(stderr.trim().to_string());
            }
            Err(err) => {
                self.status_line = format!("Erro ao executar commit: {err}");
            }
        }
    }

    fn run_terminal_command(&mut self) {
        let command = self.terminal_input.trim().to_string();
        if command.is_empty() {
            return;
        }
        self.terminal_output.push(format!("> {command}"));
        let output = Command::new("sh").arg("-c").arg(&command).output();
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                if !stdout.is_empty() {
                    self.terminal_output.push(stdout);
                }
                if !stderr.is_empty() {
                    self.terminal_output.push(stderr);
                }
            }
            Err(err) => self.terminal_output.push(format!("erro: {err}")),
        }
        self.terminal_input.clear();
    }

    fn ask_ai(&mut self) {
        let endpoint = env::var("CANARY_AI_API_URL").ok();
        let token = env::var("CANARY_AI_API_TOKEN").ok();
        if endpoint.is_none() || token.is_none() {
            self.ai_response = String::from("Configure CANARY_AI_API_URL e CANARY_AI_API_TOKEN para usar a integração com IA.");
            return;
        }

        #[derive(Serialize)]
        struct Payload<'a> {
            prompt: &'a str,
            model: &'a str,
        }

        let payload = Payload {
            prompt: self.ai_prompt.trim(),
            model: "gpt-4o-mini",
        };
        let client = reqwest::blocking::Client::new();
        let response = client
            .post(endpoint.unwrap())
            .header("Authorization", format!("Bearer {}", token.unwrap()))
            .json(&payload)
            .send();
        match response {
            Ok(resp) => {
                let text = resp.text().unwrap_or_else(|_| String::from("sem resposta"));
                self.ai_response = text;
            }
            Err(err) => self.ai_response = format!("Erro na requisição: {err}"),
        }
    }

    fn next_view(&mut self) {
        let views = View::all();
        let current = views.iter().position(|view| *view == self.current_view).unwrap_or(0);
        self.current_view = views[(current + 1) % views.len()];
    }

    fn previous_view(&mut self) {
        let views = View::all();
        let current = views.iter().position(|view| *view == self.current_view).unwrap_or(0);
        self.current_view = views[(current + views.len() - 1) % views.len()];
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        match self.current_view {
            View::Editor => match key.code {
                KeyCode::Tab => self.autocomplete(),
                KeyCode::Enter => self.new_line(),
                KeyCode::Backspace => self.remove_char(),
                KeyCode::Char(c) => self.insert_char(c),
                KeyCode::Right => self.cursor = (self.cursor + 1).min(self.buffer_chars.len()),
                KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
                KeyCode::Up => self.previous_view(),
                KeyCode::Down => self.next_view(),
                KeyCode::Esc => self.current_view = View::Themes,
                _ => {}
            },
            View::Commit => match key.code {
                KeyCode::Char(c) => self.commit_message.push(c),
                KeyCode::Backspace => {
                    self.commit_message.pop();
                }
                KeyCode::Enter => self.commit_changes(),
                KeyCode::Tab => self.next_view(),
                _ => {}
            },
            View::Terminal => match key.code {
                KeyCode::Char(c) => self.terminal_input.push(c),
                KeyCode::Backspace => {
                    self.terminal_input.pop();
                }
                KeyCode::Enter => self.run_terminal_command(),
                KeyCode::Tab => self.next_view(),
                _ => {}
            },
            View::Ai => match key.code {
                KeyCode::Char(c) => self.ai_prompt.push(c),
                KeyCode::Backspace => {
                    self.ai_prompt.pop();
                }
                KeyCode::Enter => self.ask_ai(),
                KeyCode::Tab => self.next_view(),
                _ => {}
            },
            View::Extensions => match key.code {
                KeyCode::Up => {
                    if let Some(selected) = self.extensions.iter_mut().find(|ext| ext.enabled) {
                        selected.enabled = false;
                    }
                }
                KeyCode::Down => {
                    if let Some(selected) = self.extensions.iter_mut().find(|ext| !ext.enabled) {
                        selected.enabled = true;
                    }
                }
                KeyCode::Enter => self.next_view(),
                KeyCode::Tab => self.next_view(),
                _ => {}
            },
            View::Themes => match key.code {
                KeyCode::Right => self.theme.next(),
                KeyCode::Left => self.theme.previous(),
                KeyCode::Tab => self.next_view(),
                _ => {}
            },
        }

        if matches!(key.code, KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL)) {
            self.status_line = String::from("Saindo da Canary IDE");
            std::process::exit(0);
        }
    }
}

fn draw_ui(frame: &mut Frame, app: &App) {
    let theme = &app.theme;
    let base_style = Style::default().fg(theme.text).bg(theme.surface);
    frame.render_widget(Clear, frame.area());
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent))
            .title(" Canary IDE ")
            .style(base_style),
        frame.area(),
    );

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(8), Constraint::Length(3)])
        .split(frame.area());

    let title = Paragraph::new("terminal-first • leve • bonita • customizável")
        .block(Block::default().borders(Borders::ALL).style(Style::default().fg(theme.accent).bg(theme.panel)))
        .style(Style::default().fg(theme.text).bg(theme.panel));
    frame.render_widget(title, outer[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(20)])
        .split(outer[1]);

    let nav_items: Vec<ListItem> = View::all()
        .iter()
        .map(|view| {
            let label = if *view == app.current_view { format!("> {}", view.label()) } else { view.label().to_string() };
            ListItem::new(label)
        })
        .collect();

    let nav = List::new(nav_items)
        .block(Block::default().borders(Borders::ALL).title(" Navegação ").style(Style::default().fg(theme.accent).bg(theme.panel)));
    frame.render_widget(nav, body[0]);

    let content_area = body[1];
    let content_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", app.current_view.label()))
        .style(Style::default().fg(theme.text).bg(theme.panel));
    frame.render_widget(content_block, content_area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(3)])
        .margin(1)
        .split(content_area);

    match app.current_view {
        View::Editor => {
            let text = app.buffer_text();
            let suggestions = app
                .learned_words
                .iter()
                .filter(|word| word.starts_with(&app.current_word().to_lowercase()))
                .take(5)
                .cloned()
                .collect::<Vec<_>>();
            let editor = Paragraph::new(format!("{text}\n\nSugestões: {}", suggestions.join(", ")))
                .block(Block::default().borders(Borders::NONE).style(Style::default().fg(theme.text).bg(theme.panel)));
            frame.render_widget(editor, inner[0]);
        }
        View::Commit => {
            let commit_block = Paragraph::new(format!("Status do repositório:\n{}\n\nMensagem:\n{}", app.git_status, app.commit_message))
                .block(Block::default().borders(Borders::NONE).style(Style::default().fg(theme.text).bg(theme.panel)));
            frame.render_widget(commit_block, inner[0]);
        }
        View::Terminal => {
            let terminal_block = Paragraph::new(app.terminal_output.join("\n"))
                .block(Block::default().borders(Borders::NONE).style(Style::default().fg(theme.text).bg(theme.panel)));
            frame.render_widget(terminal_block, inner[0]);
            let prompt = Paragraph::new(format!("> {}", app.terminal_input));
            frame.render_widget(prompt, inner[1]);
        }
        View::Ai => {
            let ai_block = Paragraph::new(format!("Prompt:\n{}\n\nResposta:\n{}", app.ai_prompt, app.ai_response))
                .block(Block::default().borders(Borders::NONE).style(Style::default().fg(theme.text).bg(theme.panel)));
            frame.render_widget(ai_block, inner[0]);
        }
        View::Extensions => {
            let ext_lines: Vec<String> = app
                .extensions
                .iter()
                .map(|ext| format!("{} {} - {}", if ext.enabled { "[x]" } else { "[ ]" }, ext.name, ext.description))
                .collect();
            let ext_block = Paragraph::new(ext_lines.join("\n"))
                .block(Block::default().borders(Borders::NONE).style(Style::default().fg(theme.text).bg(theme.panel)));
            frame.render_widget(ext_block, inner[0]);
        }
        View::Themes => {
            let themes = Theme::presets();
            let theme_lines: Vec<String> = themes.iter().map(|t| format!("{} {}", if t.name == app.theme.name { "▶" } else { " " }, t.name)).collect();
            let theme_block = Paragraph::new(format!("Temas disponíveis:\n{}\n\nUse ←/→ para alternar e Tab para avançar.", theme_lines.join("\n")))
                .block(Block::default().borders(Borders::NONE).style(Style::default().fg(theme.text).bg(theme.panel)));
            frame.render_widget(theme_block, inner[0]);
        }
    }

    let footer = Paragraph::new(app.status_line.as_str())
        .block(Block::default().borders(Borders::ALL).style(Style::default().fg(theme.accent).bg(theme.panel)));
    frame.render_widget(footer, outer[2]);
}

fn run_tui() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::default();
    app.run_git_status();

    loop {
        terminal.draw(|frame| draw_ui(frame, &app))?;
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if key.code == KeyCode::Char('q') {
                        break;
                    }
                    app.handle_key(key);
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn main() {
    let headless = env::args().any(|arg| arg == "--headless");
    if headless {
        let mut app = App::default();
        app.run_git_status();
        println!("Canary IDE ready");
        println!("Modo: {}", app.current_view.label());
        println!("Tema: {}", app.theme.name);
        println!("Editor:\n{}", app.buffer_text());
        return;
    }

    if let Err(err) = run_tui() {
        eprintln!("Canary IDE falhou: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autocomplete_learns_from_buffer() {
        let mut app = App::default();
        app.buffer_chars = "let canary_mode = true;\n".chars().collect();
        app.cursor = app.buffer_chars.len();
        app.refresh_learned_words();
        let suggestions = app
            .learned_words
            .iter()
            .filter(|word| word.starts_with("canary"))
            .cloned()
            .collect::<Vec<_>>();
        assert!(suggestions.contains(&"canary_mode".to_string()));
    }

    #[test]
    fn theme_cycle_changes_presets() {
        let mut theme = Theme::default();
        theme.next();
        assert_eq!(theme.name, "Nord");
        theme.previous();
        assert_eq!(theme.name, "Canary");
    }
}
