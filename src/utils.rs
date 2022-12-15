use crossterm::cursor::MoveLeft;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use crossterm::style::Print;
use crossterm::terminal::{Clear, ClearType};
use crossterm::{execute, terminal};
use regex::Regex;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;
use std::{io, thread};

#[allow(dead_code)]
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
    pub fn new() -> Option<Self> {
        terminal::enable_raw_mode().ok()?;
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            while let Ok(event) = event::read() {
                match event {
                    Event::Key(key_event) => {
                        if tx.send(key_event).is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        });

        Some(Self {
            current_command: String::new(),
            rx,
            stdout: io::stdout(),
        })
    }

    pub fn command(&mut self, timeout: Option<Duration>) -> Command {
        let key_event = if let Some(timeout) = timeout {
            match self.rx.recv_timeout(timeout) {
                Ok(key_event) => key_event,
                Err(_) => return Command::Timeout,
            }
        } else {
            KeyEvent {
                code: KeyCode::Char(':'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Release,
                state: KeyEventState::NONE,
            }
        };
        match key_event {
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
                            ..
                        } => {
                            break Command::Quit;
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
                            execute!(&self.stdout, Print("\r\n"),).unwrap();
                            break Command::Continue;
                        }
                        KeyEvent {
                            code: KeyCode::Enter,
                            ..
                        } => {
                            execute!(&self.stdout, Print("\r\n"),).unwrap();
                            break self.current_command.as_str().into();
                        }
                        _ => {}
                    }
                }
            }
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => Command::Quit,
            _ => Command::Continue,
        }
    }
}

impl Drop for CommandPrompt {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

pub enum Command {
    Timeout,
    Continue,
    Quit,
    Write(Vec<PathBuf>),
    WriteThenQuit(Vec<PathBuf>),
    Time(chrono::Duration),
    Yes,
    No,
    Empty,
    Unknown,
}

impl From<&str> for Command {
    fn from(s: &str) -> Self {
        let mut tokens = s.split_whitespace();
        let command = if let Some(command) = tokens.next() {
            command.to_lowercase()
        } else {
            return Self::Empty;
        };

        match command.as_str() {
            "q" => Self::Quit,
            "w" => {
                let mut ps = vec![];
                while let Some(p) = tokens.next() {
                    if let Ok(p) = p.parse::<PathBuf>() {
                        ps.push(p);
                    } else {
                        eprintln!("Invalid output path\r");
                        return Self::Unknown;
                    }
                }
                Self::Write(ps)
            }
            "wq" => {
                let mut ps = vec![];
                while let Some(p) = tokens.next() {
                    if let Ok(p) = p.parse::<PathBuf>() {
                        ps.push(p);
                    } else {
                        eprintln!("Invalid output path\r");
                        return Self::Unknown;
                    }
                }
                Self::WriteThenQuit(ps)
            }
            "time" => {
                if let Some(d) = tokens
                    .next()
                    .and_then(|d| d.parse::<humantime::Duration>().ok())
                    .and_then(|d| chrono::Duration::from_std(*d).ok())
                {
                    Self::Time(d)
                } else {
                    eprintln!("Invalid time\r");
                    Self::Unknown
                }
            }
            "y" | "yes" => Self::Yes,
            "n" | "no" => Self::No,
            _ => Self::Unknown,
        }
    }
}

pub fn extend_path(path_re: &Regex, ps: Vec<PathBuf>) -> Vec<PathBuf> {
    ps.into_iter()
        .filter_map(|path| {
            let ext = path.extension()?;
            let mut paths = vec![];

            if let Some(ext) = ext.to_str() {
                if path_re.is_match(ext) {
                    for cap in path_re.captures_iter(ext) {
                        for ext_str in cap[1].split(',') {
                            let mut path_cloned = path.clone();
                            path_cloned.set_extension(ext_str);
                            paths.push(path_cloned);
                        }
                    }
                } else {
                    paths.push(path);
                }
            } else {
                paths.push(path);
            };

            Some(paths)
        })
        .flatten()
        .collect()
}

pub fn check_permission(ps: &[PathBuf]) -> bool {
    let mut opt = OpenOptions::new();
    opt.write(true);
    for p in ps {
        if p.exists() && opt.open(p).is_err() {
            return false;
        }
    }
    true
}

pub fn adjust_privileges() {
    #[cfg(target_os = "windows")]
    platform_windows::adjust_privileges();
}

#[cfg(target_os = "windows")]
mod platform_windows {
    use std::mem;
    use std::os::windows::prelude::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
    use windows::Win32::System::{SystemServices, Threading};
    use windows::Win32::{Foundation, Security};

    pub fn adjust_privileges() {
        unsafe {
            let mut token_handle: Foundation::HANDLE = mem::zeroed();
            let r = Threading::OpenProcessToken(
                Threading::GetCurrentProcess(),
                Security::TOKEN_ADJUST_PRIVILEGES,
                &mut token_handle,
            );
            if !r.as_bool() {
                eprintln!("OpenProcessToken failed");
                return;
            }
            let token_handle = OwnedHandle::from_raw_handle(token_handle.0 as _);

            let mut luid: Foundation::LUID = mem::zeroed();
            if !Security::LookupPrivilegeValueW(None, SystemServices::SE_DEBUG_NAME, &mut luid)
                .as_bool()
            {
                eprintln!("LookupPrivilegeValueW failed");
                return;
            }

            let mut new_state: Security::TOKEN_PRIVILEGES = mem::zeroed();
            new_state.PrivilegeCount = 1;
            new_state.Privileges[0].Luid = luid;
            new_state.Privileges[0].Attributes = Security::SE_PRIVILEGE_ENABLED;

            if !Security::AdjustTokenPrivileges(
                windows_raw_handle(token_handle.as_raw_handle()),
                false,
                Some(&mut new_state),
                mem::size_of::<Security::TOKEN_PRIVILEGES>() as _,
                None,
                None,
            )
            .as_bool()
                || Foundation::GetLastError() == Foundation::ERROR_NOT_ALL_ASSIGNED
            {
                eprintln!("AdjustTokenPrivileges failed");
            }
        }
    }

    unsafe fn windows_raw_handle(handle: RawHandle) -> Foundation::HANDLE {
        mem::transmute(handle)
    }
}

pub fn overwrite_detect(ps: &[PathBuf], prompt: &mut CommandPrompt) -> bool {
    let ps: Vec<_> = ps.into_iter().filter(|p| p.exists()).collect();
    if ps.is_empty() {
        return true;
    }

    println!("Files below already exist:\r");
    for p in ps {
        println!("{}\r", p.display());
    }
    println!("Do you want to overwrite them?[Y/n](Y)\r");

    match prompt.command(None) {
        Command::Yes | Command::Empty => true,
        _ => false,
    }
}
