use crossterm::cursor::MoveLeft;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::Print;
use crossterm::terminal::{Clear, ClearType};
use crossterm::{execute, terminal};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;
use std::{io, thread};

pub fn drain_filter_vec<T>(input: &mut Vec<T>, mut filter: impl FnMut(&mut T) -> bool) -> Vec<T> {
    let mut output = vec![];

    let mut i = 0;
    while i < input.len() {
        if filter(&mut input[i]) {
            let val = input.remove(i);
            output.push(val);
        } else {
            i += 1;
        }
    }

    output
}

pub struct CommandPrompt {
    current_command: String,
    rx: Receiver<KeyEvent>,
    stdout: io::Stdout,
}

impl CommandPrompt {
    pub fn new() -> Self {
        terminal::enable_raw_mode().unwrap();
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || loop {
            match event::read().unwrap() {
                Event::Key(key_event) => tx.send(key_event).unwrap(),
                _ => {}
            }
        });

        Self {
            current_command: String::new(),
            rx,
            stdout: io::stdout(),
        }
    }

    pub fn command(&mut self, timeout: Duration) -> Command {
        match self.rx.recv_timeout(timeout) {
            Ok(key_event) => match key_event {
                KeyEvent {
                    code: KeyCode::Char(':'),
                    ..
                } => {
                    execute!(&self.stdout, Print(':'),).unwrap();

                    self.current_command.clear();
                    loop {
                        match self.rx.recv().unwrap() {
                            KeyEvent {
                                code: KeyCode::Char('c'),
                                modifiers: KeyModifiers::CONTROL,
                            } => {
                                return Command::Quit;
                            }
                            KeyEvent {
                                code: KeyCode::Char(c),
                                ..
                            } => {
                                self.current_command.push(c);
                                execute!(&self.stdout, Print(c),).unwrap();
                            }
                            KeyEvent {
                                code: KeyCode::Backspace,
                                ..
                            } => {
                                if self.current_command.pop().is_some() {
                                    execute!(
                                        &self.stdout,
                                        MoveLeft(1),
                                        Clear(ClearType::UntilNewLine),
                                    )
                                    .unwrap();
                                }
                            }
                            KeyEvent {
                                code: KeyCode::Esc, ..
                            } => {
                                self.current_command.clear();
                                break;
                            }
                            KeyEvent {
                                code: KeyCode::Enter,
                                ..
                            } => {
                                break;
                            }
                            _ => {}
                        }
                    }
                    execute!(&self.stdout, Print("\r\n"),).unwrap();
                    self.current_command.as_str().into()
                }
                KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers: KeyModifiers::CONTROL,
                } => Command::Quit,
                _ => Command::Continue,
            },
            Err(RecvTimeoutError::Timeout) => Command::Timeout,
            Err(RecvTimeoutError::Disconnected) => panic!("Command prompt is disconnected"),
        }
    }
}

impl Drop for CommandPrompt {
    fn drop(&mut self) {
        terminal::disable_raw_mode().unwrap();
    }
}

pub enum Command {
    Timeout,
    Continue,
    Quit,
    Write,
    WriteThenQuit,
}

impl From<&str> for Command {
    fn from(s: &str) -> Self {
        match s {
            "q" => Self::Quit,
            "w" => Self::Write,
            "wq" => Self::WriteThenQuit,
            _ => Self::Continue,
        }
    }
}
