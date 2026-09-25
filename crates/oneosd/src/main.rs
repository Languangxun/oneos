use oneos_proto::{
    LogsParam, Method, Request, Response, ServiceInfo, SessionState, SettingsInfo, Status,
    UnitParam, ValueParam,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::fd::FromRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
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
    let id = request.id;
    let params = request.params;

    match request.method {
        Method::Ping => Response::ok(id, json!({ "pong": true })),
        Method::Status => Response::ok(id, status()),
        Method::Poweroff => power_action(id, "poweroff"),
        Method::Reboot => power_action(id, "reboot"),
        Method::SessionStatus => Response::ok(id, session_state()),
        Method::SessionStart => session_action(id, "start"),
        Method::SessionStop => session_action(id, "stop"),
        Method::ServiceList => service_list(id),
        Method::ServiceStatus => match parse_unit(id, params) {
            Ok(unit) => service_status(id, &unit),
            Err(response) => response,
        },
        Method::ServiceStart => service_action(id, "start", params),
        Method::ServiceStop => service_action(id, "stop", params),
        Method::ServiceRestart => service_action(id, "restart", params),
        Method::Logs => logs(id, params),
        Method::SettingsShow => Response::ok(id, settings_show()),
        Method::SettingsSetHostname => settings_set(id, params, "hostname"),
        Method::SettingsSetTimezone => settings_set(id, params, "timezone"),
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

const SESSION_UNIT: &str = "oneos-session.service";

fn session_action(id: u64, action: &str) -> Response {
    if std::env::var_os("ONEO_DEV").is_some() {
        return Response::error(
            id,
            "dev_mode",
            "session operations are disabled in dev mode",
        );
    }

    match Command::new("systemctl")
        .arg(action)
        .arg(SESSION_UNIT)
        .output()
    {
        Ok(output) if output.status.success() => Response::ok(id, session_state()),
        Ok(output) => Response::error(
            id,
            "systemctl_failed",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ),
        Err(err) => Response::error(id, "spawn_failed", err.to_string()),
    }
}

fn session_state() -> SessionState {
    let state = Command::new("systemctl")
        .args(["is-active", SESSION_UNIT])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|state| !state.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    SessionState {
        active: state == "active",
        state,
    }
}

fn parse_unit(id: u64, params: Value) -> Result<String, Response> {
    let param: UnitParam = serde_json::from_value(params)
        .map_err(|err| Response::error(id, "bad_params", err.to_string()))?;
    validate_unit(&param.unit).map_err(|message| Response::error(id, "bad_params", message))?;
    Ok(param.unit)
}

fn validate_unit(unit: &str) -> Result<(), String> {
    if unit.is_empty() {
        return Err("unit name must not be empty".to_string());
    }
    if unit.starts_with('-')
        || unit
            .chars()
            .any(|c| c.is_whitespace() || c == '/' || c == '\\' || c == '\0')
    {
        return Err(format!("invalid unit name: {unit}"));
    }
    Ok(())
}

fn service_list(id: u64) -> Response {
    let output = Command::new("systemctl")
        .args([
            "list-units",
            "--type=service",
            "--all",
            "--output=json",
            "--no-pager",
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let units: Vec<SystemdUnit> =
                serde_json::from_slice(&output.stdout).unwrap_or_default();
            let services: Vec<ServiceInfo> = units.into_iter().map(Into::into).collect();
            Response::ok(id, services)
        }
        Ok(output) => Response::error(id, "systemctl_failed", command_error(&output)),
        Err(err) => Response::error(id, "spawn_failed", err.to_string()),
    }
}

fn service_status(id: u64, unit: &str) -> Response {
    match unit_properties(unit) {
        Ok(properties) => Response::ok(id, properties.into_service_info()),
        Err(err) => Response::error(id, "systemctl_failed", err.to_string()),
    }
}

fn service_action(id: u64, action: &str, params: Value) -> Response {
    if std::env::var_os("ONEO_DEV").is_some() {
        return Response::error(
            id,
            "dev_mode",
            "service operations are disabled in dev mode",
        );
    }

    let unit = match parse_unit(id, params) {
        Ok(unit) => unit,
        Err(response) => return response,
    };

    match unit_properties(&unit) {
        Ok(properties) if properties.load_state == "not-found" => {
            Response::error(id, "unit_not_found", format!("{unit}: unit not found"))
        }
        Ok(_) => match Command::new("systemctl").arg(action).arg(&unit).output() {
            Ok(output) if output.status.success() => service_status(id, &unit),
            Ok(output) => Response::error(id, "systemctl_failed", command_error(&output)),
            Err(err) => Response::error(id, "spawn_failed", err.to_string()),
        },
        Err(err) => Response::error(id, "systemctl_failed", err.to_string()),
    }
}

fn logs(id: u64, params: Value) -> Response {
    let param: LogsParam = serde_json::from_value(params).unwrap_or_default();
    let lines = param.lines.unwrap_or(50).clamp(1, 1000);

    let mut command = Command::new("journalctl");
    command
        .args(["--no-pager", "--output=short-iso", "-n"])
        .arg(lines.to_string());
    if let Some(unit) = &param.unit {
        command.arg("-u").arg(unit);
    }

    match command.output() {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            Response::ok(id, json!({ "text": text }))
        }
        Ok(output) => Response::error(id, "journalctl_failed", command_error(&output)),
        Err(err) => Response::error(id, "spawn_failed", err.to_string()),
    }
}

fn settings_show() -> SettingsInfo {
    SettingsInfo {
        hostname: read_trimmed("/etc/hostname").unwrap_or_default(),
        timezone: timezone_name(),
        locale: locale_name(),
    }
}

fn timezone_name() -> String {
    fs::read_link("/etc/localtime")
        .ok()
        .and_then(|path| {
            path.strip_prefix("/usr/share/zoneinfo")
                .ok()
                .map(|path| path.to_string_lossy().trim_start_matches('/').to_string())
        })
        .filter(|timezone| !timezone.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn locale_name() -> String {
    fs::read_to_string("/etc/locale.conf")
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                line.strip_prefix("LANG=")
                    .map(|value| value.trim().to_string())
            })
        })
        .unwrap_or_else(|| "C".to_string())
}

fn settings_set(id: u64, params: Value, key: &str) -> Response {
    if std::env::var_os("ONEO_DEV").is_some() {
        return Response::error(id, "dev_mode", "settings changes are disabled in dev mode");
    }

    let param: ValueParam = match serde_json::from_value(params) {
        Ok(param) => param,
        Err(err) => return Response::error(id, "bad_params", err.to_string()),
    };

    let (command, argument) = match key {
        "hostname" => {
            if let Err(message) = validate_hostname(&param.value) {
                return Response::error(id, "bad_params", message);
            }
            ("hostnamectl", "set-hostname")
        }
        "timezone" => {
            if let Err(message) = validate_timezone(&param.value) {
                return Response::error(id, "bad_params", message);
            }
            ("timedatectl", "set-timezone")
        }
        unknown => {
            return Response::error(id, "bad_params", format!("unknown setting: {unknown}"));
        }
    };

    match run_control(command, &[argument, param.value.as_str()]) {
        Ok(()) => Response::ok(id, settings_show()),
        Err(message) => Response::error(id, "settings_failed", message),
    }
}

fn run_control(command: &str, args: &[&str]) -> Result<(), String> {
    match Command::new(command).args(args).output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(command_error(&output)),
        Err(err) => Err(err.to_string()),
    }
}

fn validate_hostname(hostname: &str) -> Result<(), String> {
    if hostname.is_empty() || hostname.len() > 63 {
        return Err("hostname must be 1 to 63 characters long".to_string());
    }
    let valid = hostname
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-');
    if !valid || hostname.starts_with('-') || hostname.ends_with('-') {
        return Err(format!("invalid hostname: {hostname}"));
    }
    Ok(())
}

fn validate_timezone(timezone: &str) -> Result<(), String> {
    if timezone.is_empty()
        || timezone.starts_with('/')
        || timezone.contains("..")
        || timezone
            .chars()
            .any(|c| c.is_whitespace() || c == '\\' || c == '\0')
    {
        return Err(format!("invalid timezone: {timezone}"));
    }
    if !Path::new("/usr/share/zoneinfo").join(timezone).exists() {
        return Err(format!("unknown timezone: {timezone}"));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct SystemdUnit {
    unit: String,
    load: String,
    active: String,
    sub: String,
    description: String,
}

impl From<SystemdUnit> for ServiceInfo {
    fn from(unit: SystemdUnit) -> Self {
        Self {
            unit: unit.unit,
            load: unit.load,
            active: unit.active,
            sub: unit.sub,
            description: unit.description,
        }
    }
}

#[derive(Debug, Default)]
struct UnitProperties {
    id: String,
    load_state: String,
    active_state: String,
    sub_state: String,
    description: String,
}

impl UnitProperties {
    fn into_service_info(self) -> ServiceInfo {
        ServiceInfo {
            unit: self.id,
            load: self.load_state,
            active: self.active_state,
            sub: self.sub_state,
            description: self.description,
        }
    }
}

fn unit_properties(unit: &str) -> io::Result<UnitProperties> {
    let output = Command::new("systemctl")
        .args([
            "show",
            unit,
            "--no-pager",
            "--property=Id,LoadState,ActiveState,SubState,Description",
        ])
        .output()?;

    if !output.status.success() {
        return Err(io::Error::other(command_error(&output)));
    }

    Ok(parse_unit_properties(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

fn parse_unit_properties(text: &str) -> UnitProperties {
    let mut properties = UnitProperties::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "Id" => properties.id = value.to_string(),
            "LoadState" => properties.load_state = value.to_string(),
            "ActiveState" => properties.active_state = value.to_string(),
            "SubState" => properties.sub_state = value.to_string(),
            "Description" => properties.description = value.to_string(),
            _ => {}
        }
    }
    properties
}

fn command_error(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        format!("command exited with {}", output.status)
    } else {
        stderr.to_string()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_unit_accepts_normal_names() {
        assert!(validate_unit("sshd.service").is_ok());
        assert!(validate_unit("oneos-session.service").is_ok());
        assert!(validate_unit("getty@tty1.service").is_ok());
    }

    #[test]
    fn validate_unit_rejects_dangerous_names() {
        assert!(validate_unit("").is_err());
        assert!(validate_unit("--help").is_err());
        assert!(validate_unit("foo bar").is_err());
        assert!(validate_unit("foo/bar").is_err());
    }

    #[test]
    fn command_error_falls_back_to_status() {
        let output = Command::new("sh").args(["-c", "exit 7"]).output().unwrap();
        assert_eq!(command_error(&output), "command exited with exit status: 7");
    }

    #[test]
    fn parse_unit_properties_reads_show_output() {
        let text = "Id=sshd.service\nDescription=OpenBSD Secure Shell server\nLoadState=loaded\nActiveState=active\nSubState=running\n";
        let properties = parse_unit_properties(text);
        assert_eq!(properties.id, "sshd.service");
        assert_eq!(properties.load_state, "loaded");
        assert_eq!(properties.active_state, "active");
        assert_eq!(properties.sub_state, "running");
        assert_eq!(properties.description, "OpenBSD Secure Shell server");
    }

    #[test]
    fn parse_unit_properties_handles_missing_unit() {
        let text = "Id=nope.service\nLoadState=not-found\nActiveState=inactive\nSubState=dead\n";
        let properties = parse_unit_properties(text);
        assert_eq!(properties.load_state, "not-found");
    }

    #[test]
    fn systemd_unit_json_maps_to_service_info() {
        let json = r#"{"unit":"dbus.service","load":"loaded","active":"active","sub":"running","description":"D-Bus System Message Bus"}"#;
        let unit: SystemdUnit = serde_json::from_str(json).unwrap();
        let info: ServiceInfo = unit.into();
        assert_eq!(info.unit, "dbus.service");
        assert_eq!(info.active, "active");
        assert_eq!(info.sub, "running");
    }

    #[test]
    fn validate_hostname_rules() {
        assert!(validate_hostname("oneos").is_ok());
        assert!(validate_hostname("my-host2").is_ok());
        assert!(validate_hostname("").is_err());
        assert!(validate_hostname("-bad").is_err());
        assert!(validate_hostname("bad-").is_err());
        assert!(validate_hostname("bad host").is_err());
        assert!(validate_hostname(&"a".repeat(64)).is_err());
    }

    #[test]
    fn validate_timezone_rules() {
        assert!(validate_timezone("UTC").is_ok());
        assert!(validate_timezone("Asia/Shanghai").is_ok());
        assert!(validate_timezone("").is_err());
        assert!(validate_timezone("/etc/passwd").is_err());
        assert!(validate_timezone("../../etc/passwd").is_err());
        assert!(validate_timezone("Not/AZone").is_err());
    }
}
