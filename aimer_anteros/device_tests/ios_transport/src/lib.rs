//! Physical-iOS proof entry point for encrypted Bonjour module transfer.

use std::env;
use std::io::{self, Write};
use std::process;
use std::thread;
use std::time::Duration;

use aimer_reload_protocol::{ProtocolLimits, SessionCredentials};
use aimer_reload_server::ReloadListener;
use zeroize::Zeroizing;

const PROOF_MODULE: &[u8] = b"\0asm\x01\0\0\0";

/// Starts an encrypted app-listening reload proof and returns its TCP port.
///
/// The launch environment is populated through `devicectl`'s private child
/// environment channel. This function copies the values into zeroizing buffers,
/// validates their exact lengths, and never writes them to diagnostics.
#[unsafe(no_mangle)]
pub extern "C" fn aimer_reload_transport_proof_start() -> u16 {
    match start_listener() {
        Ok(port) => port,
        Err(status) => status,
    }
}

fn start_listener() -> Result<u16, u16> {
    let session_id =
        Zeroizing::new(env::var("AIMER_RELOAD_SESSION_ID").map_err(|_| 1_u16)?);
    let token = Zeroizing::new(env::var("AIMER_RELOAD_SESSION_TOKEN").map_err(|_| 2_u16)?);
    let mut session_id_bytes = [0_u8; 16];
    let mut token_bytes = [0_u8; 32];
    hex::decode_to_slice(session_id.as_bytes(), &mut session_id_bytes).map_err(|_| 3_u16)?;
    hex::decode_to_slice(token.as_bytes(), &mut token_bytes).map_err(|_| 4_u16)?;
    let credentials = SessionCredentials::from_parts(session_id_bytes, token_bytes);
    let limits = ProtocolLimits::new(1024, Duration::from_secs(15));
    let listener = ReloadListener::bind("[::]:0", credentials, limits, |module| {
        if module == PROOF_MODULE {
            Ok(())
        } else {
            Err("device proof received unexpected module bytes".into())
        }
    })
    .map_err(|_| 5_u16)?;
    let port = listener.local_addr().map_err(|_| 6_u16)?.port();
    thread::spawn(move || loop {
        match listener.accept_once() {
            Ok(acknowledgement) if acknowledgement.module_len == PROOF_MODULE.len() => {
                println!("AIMER_RELOAD_TRANSPORT_DEVICE_PROOF_RESULT=0");
                let _ = io::stdout().flush();
                process::exit(0);
            }
            Ok(_) | Err(_) => {
                continue;
            }
        }
    });
    Ok(port)
}