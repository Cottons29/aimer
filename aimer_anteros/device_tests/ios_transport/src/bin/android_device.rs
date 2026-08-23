use std::io::{self, BufRead, Write};
use std::time::Duration;

use aimer_reload_protocol::{ProtocolLimits, SessionCredentials};
use aimer_reload_server::ReloadListener;
use zeroize::Zeroizing;

const LISTENER_PORT: u16 = 37654;
const PROOF_MODULE: &[u8] = b"\0asm\x01\0\0\0";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let session_id = Zeroizing::new(lines.next().ok_or("missing session identifier")??);
    let token = Zeroizing::new(lines.next().ok_or("missing session token")??);
    let mut session_id_bytes = [0_u8; 16];
    let mut token_bytes = [0_u8; 32];
    hex::decode_to_slice(session_id.as_bytes(), &mut session_id_bytes)?;
    hex::decode_to_slice(token.as_bytes(), &mut token_bytes)?;
    let credentials = SessionCredentials::from_parts(session_id_bytes, token_bytes);
    let limits = ProtocolLimits::new(1024, Duration::from_secs(15));
    let listener = ReloadListener::bind(
        ("127.0.0.1", LISTENER_PORT),
        credentials,
        limits,
        |module| {
            if module == PROOF_MODULE {
                Ok(())
            } else {
                Err("Android proof received unexpected module bytes".into())
            }
        },
    )?;
    println!("AIMER_RELOAD_ANDROID_LISTENER_READY={LISTENER_PORT}");
    io::stdout().flush()?;
    let acknowledgement = listener.accept_once()?;
    if acknowledgement.module_len != PROOF_MODULE.len() {
        return Err("Android proof acknowledged an unexpected module length".into());
    }
    println!("AIMER_RELOAD_TRANSPORT_ANDROID_DEVICE_PROOF_RESULT=0");
    io::stdout().flush()?;
    Ok(())
}