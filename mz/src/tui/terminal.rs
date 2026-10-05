use crate::{config::Language, Result};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};
use std::{
    future::Future,
    io::{self, IsTerminal, Write},
    time::Duration,
};
use unicode_width::UnicodeWidthChar;

pub struct Row {
    pub label: String,
    pub hint: String,
}
impl Row {
    pub fn new(label: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            hint: hint.into(),
        }
    }
}
pub struct Ui {
    pub lang: Language,
    pub quit: bool,
    active: bool,
}
impl Ui {
    pub fn open(lang: Language) -> Result<Self> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(
                "mz config requires an interactive terminal / Нужен интерактивный терминал".into(),
            );
        }
        let mut ui = Self {
            lang,
            quit: false,
            active: false,
        };
        ui.resume()?;
        Ok(ui)
    }
    pub fn tr<'a>(&self, ru: &'a str, en: &'a str) -> &'a str {
        self.lang.text(ru, en)
    }
    pub fn resume(&mut self) -> Result<()> {
        terminal::enable_raw_mode()?;
        self.active = true;
        execute!(
            io::stdout(),
            terminal::EnterAlternateScreen,
            cursor::Hide,
            event::EnableBracketedPaste
        )?;
        Ok(())
    }
    pub fn suspend(&mut self) -> Result<()> {
        if self.active {
            self.active = false;
            let result = execute!(
                io::stdout(),
                ResetColor,
                cursor::Show,
                event::DisableBracketedPaste,
                terminal::LeaveAlternateScreen
            );
            terminal::disable_raw_mode()?;
            result?;
        }
        Ok(())
    }
    async fn event(&mut self) -> Result<Event> {
        loop {
            if event::poll(Duration::ZERO)? {
                let value = event::read()?;
                if let Event::Key(key) = &value {
                    if key.kind == KeyEventKind::Release {
                        continue;
                    }
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        self.quit = true;
                    }
                }
                return Ok(value);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    fn frame(&self, title: &str, rows: &[Row], selected: usize, footer: &str) -> Result<()> {
        let (width, height) = terminal::size()?;
        let width = width.saturating_sub(2) as usize;
        let mut out = io::stdout().lock();
        queue!(
            out,
            cursor::MoveTo(0, 0),
            Clear(ClearType::All),
            SetForegroundColor(Color::Cyan),
            Print(fit(&format!(" MusicZero  /  {title}"), width)),
            ResetColor
        )?;
        if height < 8 || width < 24 {
            queue!(
                out,
                cursor::MoveTo(0, 2.min(height.saturating_sub(1))),
                Print(fit(
                    self.tr(
                        "Увеличьте окно; Esc — назад",
                        "Enlarge terminal; Esc — back"
                    ),
                    width
                ))
            )?;
            out.flush()?;
            return Ok(());
        }
        let count = height.saturating_sub(7).max(1) as usize;
        let offset = selected.saturating_sub(count - 1);
        for (index, row) in rows.iter().enumerate().skip(offset).take(count) {
            queue!(out, cursor::MoveTo(1, (index - offset + 2) as u16))?;
            if selected == index {
                queue!(
                    out,
                    SetBackgroundColor(Color::DarkCyan),
                    SetForegroundColor(Color::White)
                )?;
            }
            queue!(
                out,
                Print(fit(
                    &format!(
                        "{} {}",
                        if index == selected { ">" } else { " " },
                        row.label
                    ),
                    width
                )),
                ResetColor
            )?;
        }
        let hint = rows
            .get(selected)
            .map(|row| row.hint.as_str())
            .unwrap_or("");
        for (i, line) in wrap(hint, width).iter().take(3).enumerate() {
            queue!(
                out,
                cursor::MoveTo(1, height - 4 + i as u16),
                SetForegroundColor(Color::DarkGrey),
                Print(line),
                ResetColor
            )?;
        }
        queue!(
            out,
            cursor::MoveTo(1, height - 1),
            Print(fit(footer, width))
        )?;
        out.flush()?;
        Ok(())
    }
    pub async fn choose(
        &mut self,
        title: &str,
        rows: &[Row],
        selected: &mut usize,
    ) -> Result<Option<usize>> {
        if rows.is_empty() || self.quit {
            return Ok(None);
        }
        *selected = (*selected).min(rows.len() - 1);
        loop {
            self.frame(
                title,
                rows,
                *selected,
                self.tr(
                    "↑↓ / j k — выбор · Enter — открыть · Esc / q — назад",
                    "↑↓ / j k select · Enter open · Esc / q back",
                ),
            )?;
            if let Event::Key(key) = self.event().await? {
                if self.quit {
                    return Ok(None);
                }
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                    KeyCode::Enter | KeyCode::Right => return Ok(Some(*selected)),
                    KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => {
                        *selected = (*selected + 1).min(rows.len() - 1)
                    }
                    KeyCode::Home => *selected = 0,
                    KeyCode::End => *selected = rows.len() - 1,
                    KeyCode::PageUp => *selected = selected.saturating_sub(10),
                    KeyCode::PageDown => *selected = (*selected + 10).min(rows.len() - 1),
                    _ => {}
                }
            }
        }
    }
    pub async fn input(
        &mut self,
        title: &str,
        hint: &str,
        initial: &str,
        secret: bool,
    ) -> Result<Option<String>> {
        let mut text: Vec<char> = if secret {
            Vec::new()
        } else {
            initial.chars().collect()
        };
        let mut cursor = text.len();
        let mut changed = false;
        loop {
            let shown: String = if secret {
                "*".repeat(text.len())
            } else {
                text.iter().collect()
            };
            let width = terminal::size()?.0.saturating_sub(8) as usize;
            let before: String = shown.chars().take(cursor).collect();
            let start = before.chars().count().saturating_sub(width / 2);
            let shown: String = shown.chars().skip(start).collect();
            let caret = before
                .chars()
                .skip(start)
                .map(|c| c.width().unwrap_or(0))
                .sum::<usize>();
            let rows = [
                Row::new(shown, hint),
                Row::new(format!("{}^", " ".repeat(caret.min(width))), ""),
            ];
            self.frame(
                title,
                &rows,
                0,
                self.tr(
                    "Enter — сохранить · Esc — отмена · Ctrl+U — очистить",
                    "Enter save · Esc cancel · Ctrl+U clear",
                ),
            )?;
            match self.event().await? {
                Event::Key(key) => {
                    if self.quit {
                        return Ok(None);
                    }
                    match key.code {
                        KeyCode::Esc => return Ok(None),
                        KeyCode::Enter => return Ok(changed.then(|| text.iter().collect())),
                        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            text.clear();
                            cursor = 0;
                            changed = true;
                        }
                        KeyCode::Left => cursor = cursor.saturating_sub(1),
                        KeyCode::Right => cursor = (cursor + 1).min(text.len()),
                        KeyCode::Home => cursor = 0,
                        KeyCode::End => cursor = text.len(),
                        KeyCode::Backspace if cursor > 0 => {
                            cursor -= 1;
                            text.remove(cursor);
                            changed = true;
                        }
                        KeyCode::Delete if cursor < text.len() => {
                            text.remove(cursor);
                            changed = true;
                        }
                        KeyCode::Char(c)
                            if !key
                                .modifiers
                                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                                && !c.is_control()
                                && text.len() < 32768 =>
                        {
                            text.insert(cursor, c);
                            cursor += 1;
                            changed = true;
                        }
                        _ => {}
                    }
                }
                Event::Paste(value) => {
                    for c in value
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(32768usize.saturating_sub(text.len()))
                    {
                        text.insert(cursor, c);
                        cursor += 1;
                        changed = true;
                    }
                }
                _ => {}
            }
        }
    }
    pub async fn message(&mut self, title: &str, message: &str) -> Result<()> {
        let width = terminal::size()?.0.saturating_sub(6).max(10) as usize;
        let rows: Vec<_> = wrap(message, width)
            .into_iter()
            .map(|line| Row::new(line, self.tr("Enter / Esc — закрыть", "Enter / Esc close")))
            .collect();
        self.choose(title, &rows, &mut 0).await?;
        Ok(())
    }
    pub async fn wait<T>(&mut self, future: impl Future<Output = Result<T>>) -> Result<T> {
        tokio::pin!(future);
        loop {
            self.frame(
                self.tr("Загрузка", "Loading"),
                &[Row::new(self.tr("Подождите…", "Please wait…"), "")],
                0,
                self.tr("Esc — отмена", "Esc cancel"),
            )?;
            tokio::select! {
                result = &mut future => return result,
                event = self.event() => {
                    if self.quit || matches!(event?, Event::Key(key) if key.code == KeyCode::Esc) {
                        return Err(self.tr("Операция отменена", "Operation cancelled").into());
                    }
                }
            }
        }
    }
}
impl Drop for Ui {
    fn drop(&mut self) {
        let _ = self.suspend();
    }
}

fn fit(text: &str, max: usize) -> String {
    let mut result = String::new();
    let mut width = 0;
    for c in text.chars().filter(|c| !c.is_control()) {
        width += c.width().unwrap_or(0);
        if width > max {
            break;
        }
        result.push(c);
    }
    result
}
fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        let mut width = 0;
        for c in paragraph.chars().filter(|c| !c.is_control()) {
            let size = c.width().unwrap_or(0);
            if width + size > max && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                width = 0;
            }
            line.push(c);
            width += size;
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
