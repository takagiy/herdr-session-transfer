//! The popup where the user chooses the harness to transfer to.

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::{cursor, execute, style, terminal};
use std::io::{Write, stdout};
use txcript::HarnessId;

/// A key the picker responds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Enter,
    Escape,
    /// A 1-based choice number.
    Digit(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Chosen(HarnessId),
    Cancelled,
}

/// The harnesses on offer and the highlighted one.
pub struct Picker {
    choices: Vec<HarnessId>,
    selected: usize,
}

impl Picker {
    /// A picker offering every listed harness except `from`.
    pub fn new(from: HarnessId) -> Self {
        Self {
            choices: crate::harnesses::all().filter(|&h| h != from).collect(),
            selected: 0,
        }
    }

    #[cfg(test)]
    pub fn choices(&self) -> &[HarnessId] {
        &self.choices
    }

    /// Respond to `key`; the outcome once the user has chosen or cancelled.
    pub fn key(&mut self, key: Key) -> Option<Outcome> {
        match key {
            Key::Up => self.selected = self.selected.saturating_sub(1),
            Key::Down => {
                self.selected = (self.selected + 1).min(self.choices.len().saturating_sub(1))
            }
            Key::Enter => {
                return self
                    .choices
                    .get(self.selected)
                    .copied()
                    .map(Outcome::Chosen);
            }
            Key::Escape => return Some(Outcome::Cancelled),
            Key::Digit(n) => {
                let index = usize::try_from(n).ok()?.checked_sub(1)?;
                return self.choices.get(index).copied().map(Outcome::Chosen);
            }
        }
        None
    }

    fn draw(&self, from: HarnessId) -> std::io::Result<()> {
        let mut out = stdout();
        execute!(
            out,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )?;
        // Raw mode stops the terminal from turning \n into \r\n, so write both.
        write!(
            out,
            " Transfer this {} session to:\r\n\r\n",
            crate::harnesses::display_name(from)
        )?;
        for (i, &harness) in self.choices.iter().enumerate() {
            let line = format!(
                " {} {}  {}\r\n",
                if i == self.selected { '>' } else { ' ' },
                i + 1,
                crate::harnesses::display_name(harness)
            );
            if i == self.selected {
                execute!(
                    out,
                    style::SetAttribute(style::Attribute::Reverse),
                    style::Print(line),
                    style::SetAttribute(style::Attribute::Reset)
                )?;
            } else {
                write!(out, "{line}")?;
            }
        }
        write!(out, "\r\n Enter: transfer   Esc: cancel")?;
        out.flush()
    }
}

/// Ask the user which harness to transfer a `from` session to; `None` when
/// they cancel.
pub fn pick(from: HarnessId) -> Result<Option<HarnessId>, String> {
    let mut picker = Picker::new(from);
    terminal::enable_raw_mode().map_err(|e| format!("could not open the picker: {e}"))?;
    let result = (|| -> std::io::Result<Option<HarnessId>> {
        execute!(stdout(), cursor::Hide)?;
        loop {
            picker.draw(from)?;
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            let key = match key.code {
                KeyCode::Up | KeyCode::Char('k') => Key::Up,
                KeyCode::Down | KeyCode::Char('j') => Key::Down,
                KeyCode::Enter => Key::Enter,
                KeyCode::Esc | KeyCode::Char('q') => Key::Escape,
                KeyCode::Char(c) => match c.to_digit(10) {
                    Some(n) => Key::Digit(n),
                    None => continue,
                },
                _ => continue,
            };
            match picker.key(key) {
                Some(Outcome::Chosen(harness)) => return Ok(Some(harness)),
                Some(Outcome::Cancelled) => return Ok(None),
                None => {}
            }
        }
    })();
    let _ = terminal::disable_raw_mode();
    result.map_err(|e| format!("the picker failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_over_claude_code_offers_the_other_harnesses() {
        // Given a pane running Claude Code
        // When the picker opens over it
        let picker = Picker::new(HarnessId::ClaudeCode);
        // Then it offers every listed harness except Claude Code
        assert!(!picker.choices().is_empty());
        assert!(!picker.choices().contains(&HarnessId::ClaudeCode));
        assert!(picker.choices().contains(&HarnessId::Codex));
    }

    /// A picker offering Codex then Claude Code, as if more harnesses were
    /// listed than the one other than the pane's.
    fn two_choices() -> Picker {
        Picker {
            choices: vec![HarnessId::Codex, HarnessId::ClaudeCode],
            selected: 0,
        }
    }

    #[test]
    fn moving_down_and_confirming_chooses_the_highlighted_harness() {
        // Given a picker offering Codex then Claude Code
        let mut picker = two_choices();
        // When the user moves down and confirms
        let moved = picker.key(Key::Down);
        let confirmed = picker.key(Key::Enter);
        // Then nothing is chosen on the move, and Claude Code on confirming
        assert_eq!(moved, None);
        assert_eq!(confirmed, Some(Outcome::Chosen(HarnessId::ClaudeCode)));
    }

    #[test]
    fn pressing_a_number_chooses_that_harness() {
        // Given a picker offering Codex then Claude Code
        let mut picker = two_choices();
        // When the user presses 2
        let outcome = picker.key(Key::Digit(2));
        // Then Claude Code is chosen
        assert_eq!(outcome, Some(Outcome::Chosen(HarnessId::ClaudeCode)));
    }

    #[test]
    fn escape_closes_without_choosing() {
        // Given a picker offering Codex then Claude Code
        let mut picker = two_choices();
        // When the user presses Escape
        let outcome = picker.key(Key::Escape);
        // Then the picker closes without a choice
        assert_eq!(outcome, Some(Outcome::Cancelled));
    }
}
