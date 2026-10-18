//! hatcheck serve: run the command on each line of JSON read from stdin, such as
//! {"id": 1, "args": ["usage", "--json"], "env": {"HATCHECK_TERMINAL": "iterm"}}, and answer
//! each with a line of JSON on stdout, {"id": 1, "code": 0, "out": "...", "err": "..."},
//! until stdin ends. env sets environment variables for that command alone (null unsets
//! one). The process stays up between commands, so the desktop app pays for starting it
//! once, and the session cache stays loaded.

use crate::out;
use crate::py::{self, get};
use serde_json::{json, Value};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};

/// The requests and answers get pipes of their own; a program a command starts gets nothing
/// to read and has its output thrown away, so it can't take a request or garble an answer.
#[cfg(unix)]
fn take_pipes() -> (Box<dyn BufRead>, Box<dyn Write>) {
    use std::os::fd::FromRawFd;
    // SAFETY: plain descriptor juggling on 0 and 1, before any other thread runs.
    unsafe {
        let (requests, answers) = (libc::dup(0), libc::dup(1));
        if requests >= 0 && answers >= 0 {
            for (fd, mode) in [(0, libc::O_RDONLY), (1, libc::O_WRONLY)] {
                let null = libc::open(c"/dev/null".as_ptr(), mode);
                if null >= 0 {
                    libc::dup2(null, fd);
                    libc::close(null);
                }
            }
            return (
                Box::new(BufReader::new(File::from_raw_fd(requests))),
                Box::new(File::from_raw_fd(answers)),
            );
        }
    }
    (
        Box::new(BufReader::new(std::io::stdin())),
        Box::new(std::io::stdout()),
    )
}

/// On Windows the commands start nothing that writes to this process's standard handles.
#[cfg(not(unix))]
fn take_pipes() -> (Box<dyn BufRead>, Box<dyn Write>) {
    (
        Box::new(BufReader::new(std::io::stdin())),
        Box::new(std::io::stdout()),
    )
}

/// Run one of serve's commands: its exit code, stdout and stderr.
fn serve_one(args: Vec<String>, env: &serde_json::Map<String, Value>) -> (i32, String, String) {
    let saved: Vec<(String, Option<String>)> = env
        .keys()
        .map(|k| (k.clone(), std::env::var(k).ok()))
        .collect();
    for (k, v) in env {
        // SAFETY: hatcheck serve runs one command at a time, and sets these between them.
        unsafe {
            if v.is_null() {
                std::env::remove_var(k);
            } else {
                std::env::set_var(k, py::str_of(v));
            }
        }
    }
    let (code, out_text, mut err_text) =
        out::captured(|| std::panic::catch_unwind(|| crate::cli::served(args)));
    let code = match code {
        Ok(code) => code,
        Err(panic) => {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            err_text.push_str(&format!("hatcheck: {message}\n"));
            1
        }
    };
    for (k, v) in saved {
        // SAFETY: as above.
        unsafe {
            match v {
                Some(v) => std::env::set_var(&k, v),
                None => std::env::remove_var(&k),
            }
        }
    }
    crate::facts::save_cache(); // now rather than at exit: the app can quit any time
    (code, out_text, err_text)
}

pub fn run() -> i32 {
    std::panic::set_hook(Box::new(|_| {})); // a command's panic becomes its answer, not a message on stderr
    let (requests, mut answers) = take_pipes();
    for line in requests.lines() {
        let Ok(line) = line else { break };
        if py::strip(&line).is_empty() {
            continue;
        }
        let request: Option<Value> = serde_json::from_str(&line).ok();
        let parsed = request.as_ref().filter(|r| r.is_object()).and_then(|r| {
            let args: Vec<String> = get(r, "args").as_array()?.iter().map(py::str_of).collect();
            let env = match get(r, "env") {
                Value::Object(e) => e.clone(),
                Value::Null => serde_json::Map::new(),
                _ => return None,
            };
            Some((get(r, "id").clone(), args, env))
        });
        let (id, code, out_text, err_text) = match parsed {
            None => {
                let shown = py::head(py::strip(&line), 200).to_string();
                (
                    Value::Null,
                    2,
                    String::new(),
                    format!("hatcheck serve: not a request: {shown}\n"),
                )
            }
            Some((id, args, _)) if args.iter().any(|a| a == "--here") => {
                // It would turn this process into the session's terminal.
                (
                    id,
                    2,
                    String::new(),
                    "hatcheck serve can't run a command here (--here); run it in a terminal\n"
                        .into(),
                )
            }
            Some((id, args, env)) => {
                let (code, o, e) = serve_one(args, &env);
                (id, code, o, e)
            }
        };
        let answer = json!({"id": id, "code": code, "out": out_text, "err": err_text});
        if writeln!(answers, "{answer}")
            .and_then(|()| answers.flush())
            .is_err()
        {
            break;
        }
    }
    0
}
