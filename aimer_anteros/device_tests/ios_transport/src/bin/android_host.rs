use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use aimer_cli::hot_reload::route::{AndroidRouteAdapter, SystemCommandExecutor};
use aimer_reload_protocol::{ProtocolLimits, SessionCredentials, send_module};

const LISTENER_PORT: u16 = 37654;
const REMOTE_BINARY: &str = "/data/local/tmp/aimer_reload_transport_proof";
const PROOF_MODULE: &[u8] = b"\0asm\x01\0\0\0";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device_id = env::args().nth(1).ok_or("expected Android device identifier")?;
    let local_binary = android_binary_path();
    run_adb(&device_id, &["push", local_binary.to_str().ok_or("invalid binary path")?, REMOTE_BINARY])?;
    let _remote_binary = RemoteBinary::new(device_id.clone());
    run_adb(&device_id, &["shell", "chmod", "700", REMOTE_BINARY])?;
    let mut device = Command::new("adb")
        .args(["-s", &device_id, "shell", REMOTE_BINARY])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let credentials = SessionCredentials::generate()?;
    let (session_id, token) = credentials.launch_environment_hex();
    let mut stdin = device.stdin.take().ok_or("Android proof stdin unavailable")?;
    writeln!(stdin, "{}", session_id.as_str())?;
    writeln!(stdin, "{}", token.as_str())?;
    stdin.flush()?;
    drop(stdin);
    let stdout = device.stdout.take().ok_or("Android proof stdout unavailable")?;
    let mut output = BufReader::new(stdout);
    let mut ready = String::new();
    output.read_line(&mut ready)?;
    if ready.trim() != format!("AIMER_RELOAD_ANDROID_LISTENER_READY={LISTENER_PORT}") {
        return Err(format!("Android listener did not become ready: {ready:?}").into());
    }

    let adapter = AndroidRouteAdapter::new(Arc::new(SystemCommandExecutor));
    let forward = adapter.prepare(&device_id, LISTENER_PORT)?;
    let limits = ProtocolLimits::new(1024, Duration::from_secs(15));
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], forward.host_port()));
    let mut stream = TcpStream::connect_timeout(&address, limits.io_timeout())?;
    stream.set_read_timeout(Some(limits.io_timeout()))?;
    stream.set_write_timeout(Some(limits.io_timeout()))?;
    let acknowledgement = send_module(&mut stream, &credentials, limits, 1, PROOF_MODULE)?;
    if acknowledgement.module_len != PROOF_MODULE.len() {
        return Err("Android acknowledged an unexpected module length".into());
    }
    let mut result = String::new();
    output.read_line(&mut result)?;
    if result.trim() != "AIMER_RELOAD_TRANSPORT_ANDROID_DEVICE_PROOF_RESULT=0" {
        return Err(format!("Android device proof failed: {result:?}").into());
    }
    let status = device.wait()?;
    if !status.success() {
        return Err(format!("Android listener exited with {status}").into());
    }
    println!("AIMER_RELOAD_TRANSPORT_ANDROID_HOST_PROOF_RESULT=0");
    Ok(())
}

fn android_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../../target/aarch64-linux-android/debug/aimer_reload_transport_proof_android_device",
    )
}

fn run_adb(device_id: &str, arguments: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("adb")
        .args(["-s", device_id])
        .args(arguments)
        .status()?;
    if !status.success() {
        return Err(format!("adb command failed with {status}").into());
    }
    Ok(())
}

struct RemoteBinary {
    device_id: String,
}

impl RemoteBinary {
    fn new(device_id: String) -> Self {
        Self { device_id }
    }
}

impl Drop for RemoteBinary {
    fn drop(&mut self) {
        let _ = Command::new("adb")
            .args(["-s", &self.device_id, "shell", "rm", "-f", REMOTE_BINARY])
            .status();
    }
}