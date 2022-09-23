use super::ProcessCommandResult;
use crate::Pid;
use std::io::{self, BufRead};
use std::process;
use std::sync::mpsc::Sender;

pub struct TopRunner {
    tx: Sender<ProcessCommandResult>,
}

impl TopRunner {
    pub fn new(tx: Sender<ProcessCommandResult>) -> Self {
        Self { tx }
    }

    pub fn run<T: IntoIterator<Item = Pid>>(self, pids: T) {
        let mut command = process::Command::new("script");
        command.args([
            "-q",
            "/dev/null",
            "top",
            "-stats",
            "pid,ports",
            "-a",
            "-l",
            "0",
        ]);

        for p in pids {
            command.args(["-pid", p.to_string().as_str()]);
        }

        let mut child = command
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .spawn()
            .unwrap();

        let mut buf = io::BufReader::new(child.stdout.as_mut().unwrap());
        let mut line = String::new();
        while let Ok(read) = buf.read_line(&mut line) {
            if read == 0 {
                break;
            }

            let p = if let Some(p) = TopProcess::parse(&line) {
                p
            } else {
                line.clear();
                continue;
            };

            if let Err(_) = self.tx.send(ProcessCommandResult {
                pid: p.pid,
                mach_ports: p.mach_ports,
                ..Default::default()
            }) {
                break;
            }

            line.clear();
        }

        let _ = child.kill();
    }
}

#[derive(Debug)]
struct TopProcess {
    pid: u32,
    mach_ports: u32,
}

impl TopProcess {
    fn parse(s: &str) -> Option<Self> {
        let mut iter = s.split_whitespace();
        let pid: u32 = iter.next()?.parse().ok()?;
        let mach_ports: u32 = iter
            .next()?
            .trim_end_matches(|c: char| !c.is_numeric())
            .parse()
            .ok()?;
        Some(Self { pid, mach_ports })
    }
}
