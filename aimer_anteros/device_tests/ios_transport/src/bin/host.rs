use std::env;
use std::io::{BufRead, BufReader};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use aimer_reload_protocol::{ProtocolLimits, SessionCredentials, send_module};

const BUNDLE_ID: &str = "dev.aimers.reload-transport-proof";
const SERVICE_TYPE: &str = "_aimer-reload._tcp";
const PROOF_MODULE: &[u8] = b"\0asm\x01\0\0\0";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device_id = env::args().nth(1).ok_or("expected physical device identifier")?;
    let credentials = SessionCredentials::generate()?;
    let service_name = format!("Aimer Reload Proof {}", std::process::id());
    let mut device_console = launch_device_app(&device_id, &credentials, &service_name)?;
    thread::sleep(Duration::from_secs(1));
    if let Some(status) = device_console.try_wait()? {
        return Err(format!("device app exited before discovery with {status}").into());
    }
    let addresses = resolve_bonjour(&service_name, Duration::from_secs(30))?;
    let limits = ProtocolLimits::new(1024, Duration::from_secs(15));
    let mut last_error = None;
    let mut stream = addresses
        .into_iter()
        .find_map(|address| match TcpStream::connect_timeout(&address, limits.io_timeout()) {
            Ok(stream) => Some(stream),
            Err(error) => {
                last_error = Some(error);
                None
            }
        })
        .ok_or_else(|| {
            last_error.unwrap_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::AddrNotAvailable,
                    "Bonjour host resolved no socket addresses",
                )
            })
        })?;
    stream.set_read_timeout(Some(limits.io_timeout()))?;
    stream.set_write_timeout(Some(limits.io_timeout()))?;
    let acknowledgement = send_module(&mut stream, &credentials, limits, 1, PROOF_MODULE)?;
    if acknowledgement.module_len != PROOF_MODULE.len() {
        return Err("device acknowledged an unexpected module length".into());
    }
    println!("AIMER_RELOAD_TRANSPORT_HOST_PROOF_RESULT=0");
    let _ = device_console.wait();
    Ok(())
}

fn launch_device_app(
    device_id: &str,
    credentials: &SessionCredentials,
    service_name: &str,
) -> Result<Child, Box<dyn std::error::Error>> {
    let (session_id, token) = credentials.launch_environment_hex();
    let child = Command::new("xcrun")
        .args([
            "devicectl",
            "device",
            "process",
            "launch",
            "--device",
            device_id,
            "--terminate-existing",
            "--console",
            BUNDLE_ID,
        ])
        .env("DEVICECTL_CHILD_AIMER_RELOAD_SESSION_ID", session_id.as_str())
        .env("DEVICECTL_CHILD_AIMER_RELOAD_SESSION_TOKEN", token.as_str())
        .env("DEVICECTL_CHILD_AIMER_RELOAD_SERVICE_NAME", service_name)
        .spawn()?;
    Ok(child)
}

fn resolve_bonjour(
    service_name: &str,
    timeout: Duration,
) -> Result<Vec<std::net::SocketAddr>, Box<dyn std::error::Error>> {
    let mut child = Command::new("dns-sd")
        .args(["-L", service_name, SERVICE_TYPE, "local."])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdout = child.stdout.take().ok_or("dns-sd stdout was unavailable")?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(address) = parse_resolved_address(&line) {
                let _ = sender.send(address);
                return;
            }
        }
    });
    let resolved = receiver.recv_timeout(timeout);
    terminate(&mut child);
    let (host, port) = resolved.map_err(|_| "Bonjour service resolution timed out")?;
    let deadline = Instant::now() + timeout;
    loop {
        let addresses: Vec<_> = (host.as_str(), port).to_socket_addrs()?.collect();
        if !addresses.is_empty() {
            return Ok(addresses);
        }
        if Instant::now() >= deadline {
            return Err("Bonjour host address resolution timed out".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn parse_resolved_address(line: &str) -> Option<(String, u16)> {
    let (_, endpoint) = line.split_once(" can be reached at ")?;
    let endpoint = endpoint.split_once(" (interface")?.0;
    let (host, port) = endpoint.rsplit_once(':')?;
    Some((host.trim_end_matches('.').to_string(), port.parse().ok()?))
}

fn terminate(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}