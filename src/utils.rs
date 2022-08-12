use crossterm::cursor::MoveLeft;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use crossterm::style::Print;
use crossterm::terminal::{Clear, ClearType};
use crossterm::{execute, terminal};
use regex::Regex;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
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
    pub fn new() -> Self {
        terminal::enable_raw_mode().unwrap();
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || loop {
            match event::read().unwrap() {
                Event::Key(key_event) => {
                    if tx.send(key_event).is_err() {
                        break;
                    }
                }
                _ => {}
            }
        });

        Self {
            current_command: String::new(),
            rx,
            stdout: io::stdout(),
        }
    }

    pub fn command(&mut self, timeout: Option<Duration>) -> Command {
        let key_event = if let Some(timeout) = timeout {
            match self.rx.recv_timeout(timeout) {
                Ok(key_event) => key_event,
                Err(RecvTimeoutError::Timeout) => return Command::Timeout,
                Err(RecvTimeoutError::Disconnected) => panic!("Command prompt is disconnected"),
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
        terminal::disable_raw_mode().unwrap();
    }
}

pub enum Command {
    Timeout,
    Continue,
    Quit,
    Write(Vec<PathBuf>),
    WriteThenQuit(Vec<PathBuf>),
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

pub fn adjust_privileges() {
    #[cfg(target_os = "windows")]
    platform_windows::adjust_privileges();
}

#[cfg(target_os = "windows")]
mod platform_windows {
    use std::ffi::OsStr;
    use std::mem::MaybeUninit;
    use std::os::windows::ffi::OsStrExt;
    use std::{mem, ptr};
    use winapi::shared::minwindef::{FALSE, TRUE};
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};
    use winapi::um::securitybaseapi::AdjustTokenPrivileges;
    use winapi::um::winbase::LookupPrivilegeValueW;
    use winapi::um::winnt::{
        HANDLE, LUID, SE_DEBUG_NAME, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES,
        TOKEN_PRIVILEGES,
    };
    use windows::Win32::Foundation::{GetLastError, ERROR_NOT_ALL_ASSIGNED};

    struct HandleWrapper(HANDLE);

    impl Drop for HandleWrapper {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    pub fn adjust_privileges() {
        unsafe {
            let current_handle = HandleWrapper(GetCurrentProcess());
            let mut token_handle: HANDLE = MaybeUninit::uninit().assume_init();
            let r = OpenProcessToken(current_handle.0, TOKEN_ADJUST_PRIVILEGES, &mut token_handle);
            if r != TRUE {
                eprintln!("OpenProcessToken failed");
                return;
            }
            let token_handle = HandleWrapper(token_handle);

            let mut luid: LUID = MaybeUninit::uninit().assume_init();
            if LookupPrivilegeValueW(ptr::null(), to_wchar(SE_DEBUG_NAME).as_ptr(), &mut luid)
                != TRUE
            {
                eprintln!("LookupPrivilegeValueW failed");
                return;
            }

            let mut new_state: TOKEN_PRIVILEGES = MaybeUninit::uninit().assume_init();
            new_state.PrivilegeCount = 1;
            new_state.Privileges[0].Luid = luid;
            new_state.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;

            if AdjustTokenPrivileges(
                token_handle.0,
                FALSE,
                &mut new_state,
                mem::size_of::<TOKEN_PRIVILEGES>() as _,
                ptr::null_mut(),
                ptr::null_mut(),
            ) != TRUE
                || GetLastError() == ERROR_NOT_ALL_ASSIGNED
            {
                eprintln!("AdjustTokenPrivileges failed");
            }
        }
    }

    fn to_wchar(str: &str) -> Vec<u16> {
        OsStr::new(str)
            .encode_wide()
            .chain(Some(0).into_iter())
            .collect()
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
