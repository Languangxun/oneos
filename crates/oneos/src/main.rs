use oneos_proto::{Method, Request, Response, Status, call, connect};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("status");

    let outcome = match command {
        "status" => status(),
        "ping" => ping(),
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
    println!("usage: oneos <status|ping|poweroff|reboot|version|help>");
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

fn action(method: Method) -> Result<(), i32> {
    request(method)?;
    Ok(())
}

fn request(method: Method) -> Result<Response, i32> {
    let mut stream = connect().map_err(|err| {
        eprintln!("oneos: cannot connect to oneosd: {err}");
        3
    })?;

    let response = call(&mut stream, &Request::new(1, method)).map_err(|err| {
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
