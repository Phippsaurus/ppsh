use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::fs::{read_dir, DirEntry};
use std::io::{self, stdin, stdout, Write};
use std::ops::{
    Bound::{Included, Unbounded},
    RangeFrom,
};
use std::path::Path;
use std::process::{Command, Stdio};
use termion::{
    clear, color, cursor,
    event::{Event, Key},
    input::TermRead,
    raw::IntoRawMode,
};

struct Prompt;

impl Display for Prompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}",
            color::Fg(color::LightGreen),
            color::Fg(color::Reset)
        )
    }
}

trait Model {
    fn render(&self, out: &mut dyn Write) -> io::Result<()>;
}

pub struct ModelVieUpdate<M, E> {
    model: M,
    update: fn(&mut M, E) -> Option<String>,
}

impl<M, E> ModelVieUpdate<M, E> {
    pub fn new(initial_model: M, update: fn(&mut M, E) -> Option<String>) -> Self {
        Self {
            model: initial_model,
            update,
        }
    }
}

impl<M> ModelVieUpdate<M, Key>
where
    M: Model,
{
    pub fn run(&mut self) -> io::Result<()> {
        let stdin = stdin();
        let stdin = stdin.lock();

        let stdout = stdout().into_raw_mode()?;
        let mut stdout = stdout.lock();

        write!(stdout, "{}", cursor::Save)?;
        self.model.render(&mut stdout)?;
        stdout.flush()?;

        for event in stdin.keys() {
            let event = event?;
            if event == Key::Ctrl('c') {
                write!(stdout, "\r\n")?;
                break;
            }

            if let Some(input) = (self.update)(&mut self.model, event) {
                let input_token = input.split_whitespace().collect::<Vec<_>>();

                if let Some(Ok(output)) = input_token.get(0).map(|command| Command::new(command)
                    .args(&input_token[1..])
                    .output())
                {
                    if !output.stdout.is_empty() {
                        let out = std::str::from_utf8(output.stdout.as_slice())
                            .unwrap()
                            .replace('\n', "\r\n");
                        write!(stdout, "\r\n{}", out)?;
                    }
                    if !output.stderr.is_empty() {
                        let errors = std::str::from_utf8(output.stderr.as_slice())
                            .unwrap()
                            .replace('\n', "\r\n");
                        write!(stdout, "\r\n{}{}{}",
                               color::Fg(color::LightRed),
                               errors,
                               color::Fg(color::Reset))?;
                    }
                    write!( stdout, "\r\n{}", cursor::Save)?;
                } else {
                    write!(stdout, "\r\n{}\r\n{}", input, cursor::Save)?;
                }
            } else {
                write!(
                    stdout,
                    "{}{}{}",
                    cursor::Restore,
                    clear::AfterCursor,
                    cursor::Save
                )?;
            }

            self.model.render(&mut stdout)?;
            stdout.flush()?;
        }
        Ok(())
    }
}

struct Position {
    x: u16,
    y: u16,
}

impl Position {
    fn origin() -> Self {
        Self { x: 0, y: 0 }
    }
}

struct Readline {
    cursor_pos: Position,
    buffer: String,
    dir_entries: BTreeSet<String>,
    suggestion: Option<String>,
}

impl Readline {
    fn update_suggestion(&mut self) {
        if self.buffer.is_empty() {
            self.suggestion.take();
            return;
        }
        if let Some(false) | None = self
            .suggestion
            .as_ref()
            .map(|suggestion| suggestion.starts_with(self.buffer.as_str()))
        {
            self.suggestion = self
                .dir_entries
                .range::<String, RangeFrom<&String>>(RangeFrom {
                    start: &self.buffer,
                })
                .filter(|suggestion| suggestion.starts_with(&self.buffer))
                .next()
                .map(String::clone);
        }
    }
}

impl Model for Readline {
    fn render(&self, out: &mut dyn Write) -> io::Result<()> {
        write!(
            out,
            "{}{} {}",
            color::Fg(color::LightGreen),
            color::Fg(color::Reset),
            self.buffer,
        )?;

        let cursor_back = if let Some(ref suggestion) = self.suggestion {
            write!(
                out,
                "{}{}",
                color::Fg(color::Cyan),
                &suggestion[self.buffer.len()..]
            )?;
            suggestion.len()
        } else {
            self.buffer.len()
        } as u16
            - self.cursor_pos.x;

        if cursor_back > 0 {
            write!(out, "{}", cursor::Left(cursor_back))?;
        }
        Ok(())
    }
}

fn update(model: &mut Readline, event: Key) -> Option<String> {
    match event {
        Key::Char('\n') => {
            model.cursor_pos = Position::origin();
            let mut result = String::new();
            std::mem::swap(&mut model.buffer, &mut result);
            return Some(result);
        }
        Key::Char(c) => {
            model.buffer.push(c);
            model.cursor_pos.x += 1;
            model.update_suggestion();
        }
        Key::Left if model.cursor_pos.x > 0 => {
            model.cursor_pos.x -= 1;
        }
        Key::Right if model.cursor_pos.x < model.buffer.len() as u16 => {
            model.cursor_pos.x += 1;
        }
        Key::Backspace if model.cursor_pos.x == model.buffer.len() as u16 => {
            if model.buffer.pop().is_some() {
                model.cursor_pos.x -= 1;
                model.update_suggestion();
            };
        }
        Key::Backspace if model.cursor_pos.x > 0 && !model.buffer.is_empty() => {
            model.cursor_pos.x -= 1;
            model.buffer.remove(model.cursor_pos.x as usize);
            model.update_suggestion();
        }
        _ => {}
    }
    None
}

impl Default for Readline {
    fn default() -> Self {
        Self {
            cursor_pos: Position::origin(),
            buffer: String::new(),
            dir_entries: Path::new("./")
                .read_dir()
                .unwrap()
                .filter_map(|entry| Some(entry.ok()?.path().file_name()?.to_str()?.to_owned()))
                .collect(),
            suggestion: None,
        }
    }
}

fn main() -> io::Result<()> {
    ModelVieUpdate::new(Readline::default(), update).run()
}
