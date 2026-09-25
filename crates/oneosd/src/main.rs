use oneos_proto::{Method, Request, Response, Status};
use serde_json::json;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::fd::FromRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Command;

fn main() -> io::Result<()> {
    let listener = listener()?;
    eprintln!(
        "oneosd {} listening on {}",
        env!("CARGO_PKG_VERSION"),
        oneos_proto::socket_path().display()
    );

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                std::thread::spawn(move || {
                    if let Err(err) = serve(stream) {
                        eprintln!("oneosd: connection error: {err}");
                    }
                });
            }
            Err(err) => eprintln!("oneosd: accept failed: {err}"),
        }
    }

    Ok(())
}

fn listener() -> io::Result<UnixListener> {
    if let Some(listener) = systemd_listener() {
        return Ok(listener);
    }

    let path = oneos_proto::socket_path();
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o755))?;
    }
    if UnixStream::connect(&path).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            format!("{} is already in use", path.display()),
        ));
    }
    if path.exists() {
        fs::remove_file(&path)?;
    }

    let listener = UnixListener::bind(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o660))?;
    Ok(listener)
}

fn systemd_listener() -> Option<UnixListener> {
    let pid = std::env::var("LISTEN_PID").ok()?.parse::<u32>().ok()?;
    if pid != std::process::id() {
        return None;
    }
    let fds = std::env::var("LISTEN_FDS").ok()?.parse::<u32>().ok()?;
    if fds == 0 {
        return None;
    }
    Some(unsafe { UnixListener::from_raw_fd(3) })
}

fn serve(stream: UnixStream) -> io::Result<()> {
    let mut writer = stream.try_clone()?;
    let reader = BufReader::new(stream);

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => dispatch(request),
            Err(err) => Response::error(0, "bad_request", err.to_string()),
        };

        let mut payload = serde_json::to_vec(&response)?;
        payload.push(b'\n');
        writer.write_all(&payload)?;
        writer.flush()?;
    }

    Ok(())
}

fn dispatch(request: Request) -> Response {
    match request.method {
        Method::Ping => Response::ok(request.id, json!({ "pong": true })),
        Method::Status => Response::ok(request.id, status()),
        Method::Poweroff => power_action(request.id, "poweroff"),
        Method::Reboot => power_action(request.id, "reboot"),
    }
}

fn power_action(id: u64, action: &str) -> Response {
    if std::env::var_os("ONEO_DEV").is_some() {
        return Response::error(id, "dev_mode", "power operations are disabled in dev mode");
    }

    match Command::new("systemctl").arg(action).spawn() {
        Ok(_) => Response::ok(id, json!({ "accepted": action })),
        Err(err) => Response::error(id, "spawn_failed", err.to_string()),
    }
}

fn status() -> Status {
    Status {
        version: env!("CARGO_PKG_VERSION").to_string(),
        os: os_pretty_name(),
        hostname: read_trimmed("/proc/sys/kernel/hostname").unwrap_or_else(|| "oneos".to_string()),
        uptime_secs: read_trimmed("/proc/uptime")
            .and_then(|value| value.split_whitespace().next()?.parse::<f64>().ok())
            .map(|secs| secs as u64)
            .unwrap_or(0),
        boot_id: read_trimmed("/proc/sys/kernel/random/boot_id").unwrap_or_default(),
    }
}

fn read_trimmed(path: &str) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|contents| contents.trim().to_string())
}

fn os_pretty_name() -> String {
    fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                line.strip_prefix("PRETTY_NAME=")
                    .map(|value| value.trim_matches('"').to_string())
            })
        })
        .unwrap_or_else(|| "Linux".to_string())
}
