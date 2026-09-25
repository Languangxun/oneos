use oneos_proto::{
    Method, Request, Response, ServiceInfo, SessionState, SettingsInfo, Status, call, connect,
};
use serde_json::{Value, json};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("status");

    let outcome = match command {
        "status" => status(),
        "ping" => ping(),
        "session" => session(args.get(1).map(String::as_str).unwrap_or("status")),
        "service" => service(&args),
        "logs" => logs(&args),
        "settings" => settings(&args),
        "poweroff" => action(Method::Poweroff),
        "reboot" => action(Method::Reboot),
        "version" => {
            println!("oneos {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "help" | "--help" | "-h" => {
            usage();
            Ok(())
        }
        unknown => {
            eprintln!("oneos: unknown command: {unknown}");
            usage();
            Err(2)
        }
    };

    if let Err(code) = outcome {
        std::process::exit(code);
    }
}

fn usage() {
    println!("usage: oneos <status|ping|session|service|logs|poweroff|reboot|version|help>");
    println!("       oneos session <status|start|stop>");
    println!("       oneos service <list|status|start|stop|restart> [unit]");
    println!("       oneos logs [-u unit] [-n lines]");
    println!("       oneos settings [show|hostname <name>|timezone <zone>]");
}

fn status() -> Result<(), i32> {
    let response = request(Method::Status)?;
    let status: Status = response
        .result
        .and_then(|value| serde_json::from_value(value).ok())
        .ok_or_else(|| {
            eprintln!("oneos: malformed status response");
            2
        })?;

    println!("OneOS        {}", status.version);
    println!("Hostname     {}", status.hostname);
    println!("OS           {}", status.os);
    println!("Uptime       {}", format_uptime(status.uptime_secs));
    println!("Boot ID      {}", status.boot_id);
    Ok(())
}

fn ping() -> Result<(), i32> {
    request(Method::Ping)?;
    println!("pong");
    Ok(())
}

fn session(subcommand: &str) -> Result<(), i32> {
    let method = match subcommand {
        "status" => Method::SessionStatus,
        "start" => Method::SessionStart,
        "stop" => Method::SessionStop,
        unknown => {
            eprintln!("oneos: unknown session command: {unknown}");
            return Err(2);
        }
    };

    let response = request(method)?;
    if let Some(value) = response.result
        && let Ok(state) = serde_json::from_value::<SessionState>(value)
    {
        println!("Session      {}", state.state);
    }
    Ok(())
}

fn service(args: &[String]) -> Result<(), i32> {
    let subcommand = args.get(1).map(String::as_str).unwrap_or("list");

    if subcommand == "list" {
        let response = request_params(Method::ServiceList, json!({}))?;
        let services: Vec<ServiceInfo> = response
            .result
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        for service in services {
            println!(
                "{:<40} {:<8} {:<10} {}",
                service.unit, service.active, service.sub, service.description
            );
        }
        return Ok(());
    }

    let method = match subcommand {
        "status" => Method::ServiceStatus,
        "start" => Method::ServiceStart,
        "stop" => Method::ServiceStop,
        "restart" => Method::ServiceRestart,
        unknown => {
            eprintln!("oneos: unknown service command: {unknown}");
            return Err(2);
        }
    };

    let Some(unit) = args.get(2) else {
        eprintln!("oneos: unit name required");
        return Err(2);
    };

    let response = request_params(method, json!({ "unit": unit }))?;
    if let Some(value) = response.result
        && let Ok(info) = serde_json::from_value::<ServiceInfo>(value)
    {
        println!("Unit         {}", info.unit);
        println!("Active       {} ({})", info.active, info.sub);
        println!("Description  {}", info.description);
    }
    Ok(())
}

fn logs(args: &[String]) -> Result<(), i32> {
    let mut unit: Option<String> = None;
    let mut lines: Option<u32> = None;
    let mut iter = args.iter().skip(1);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-u" | "--unit" => unit = iter.next().cloned(),
            "-n" | "--lines" => lines = iter.next().and_then(|value| value.parse().ok()),
            unknown => {
                eprintln!("oneos: unknown logs option: {unknown}");
                return Err(2);
            }
        }
    }

    let response = request_params(Method::Logs, json!({ "unit": unit, "lines": lines }))?;
    if let Some(value) = response.result
        && let Some(text) = value.get("text").and_then(|text| text.as_str())
    {
        print!("{text}");
        if !text.ends_with('\n') {
            println!();
        }
    }
    Ok(())
}

fn action(method: Method) -> Result<(), i32> {
    request(method)?;
    Ok(())
}

fn settings(args: &[String]) -> Result<(), i32> {
    let subcommand = args.get(1).map(String::as_str).unwrap_or("show");

    let method = match subcommand {
        "show" => Method::SettingsShow,
        "hostname" => Method::SettingsSetHostname,
        "timezone" => Method::SettingsSetTimezone,
        unknown => {
            eprintln!("oneos: unknown settings command: {unknown}");
            return Err(2);
        }
    };

    let response = if method == Method::SettingsShow {
        request(method)?
    } else {
        let Some(value) = args.get(2) else {
            eprintln!("oneos: value required");
            return Err(2);
        };
        request_params(method, json!({ "value": value }))?
    };

    if let Some(value) = response.result
        && let Ok(info) = serde_json::from_value::<SettingsInfo>(value)
    {
        println!("Hostname     {}", info.hostname);
        println!("Timezone     {}", info.timezone);
        println!("Locale       {}", info.locale);
    }
    Ok(())
}

fn request(method: Method) -> Result<Response, i32> {
    request_params(method, Value::Null)
}

fn request_params(method: Method, params: Value) -> Result<Response, i32> {
    let mut stream = connect().map_err(|err| {
        eprintln!("oneos: cannot connect to oneosd: {err}");
        3
    })?;

    let request = Request {
        id: 1,
        method,
        params,
    };
    let response = call(&mut stream, &request).map_err(|err| {
        eprintln!("oneos: request failed: {err}");
        3
    })?;

    if !response.ok {
        eprintln!("oneos: {}", response.error_message());
        return Err(4);
    }

    Ok(response)
}

fn format_uptime(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}
