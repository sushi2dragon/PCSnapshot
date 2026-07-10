//! Browser native-messaging host.
//!
//! The browser owns stdin/stdout. This process only frames JSON for that port
//! and relays it to PC Snapshot's current-user named-pipe broker. Never write
//! diagnostics to stdout: one stray byte corrupts the browser protocol.

#[cfg(windows)]
use pc_snapshot::browser_bridge::PIPE_NAME;

#[cfg(windows)]
use serde_json::Value;

#[cfg(windows)]
const MAX_NATIVE_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

#[cfg(windows)]
fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("create native-host runtime");
    runtime.block_on(run());
}

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
async fn run() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::sync::mpsc;

    let pipe = connect_bridge().await;
    let (read, mut write) = tokio::io::split(pipe);
    let (to_pipe_tx, mut to_pipe_rx) = mpsc::unbounded_channel::<Value>();
    let (to_browser_tx, to_browser_rx) = std::sync::mpsc::channel::<Value>();

    std::thread::spawn(move || native_reader(to_pipe_tx));
    std::thread::spawn(move || native_writer(to_browser_rx));

    let writer = tokio::spawn(async move {
        while let Some(message) = to_pipe_rx.recv().await {
            let Ok(bytes) = serde_json::to_vec(&message) else { continue; };
            if write.write_all(&bytes).await.is_err() || write.write_all(b"\n").await.is_err() {
                break;
            }
        }
    });

    let mut lines = BufReader::new(read).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(message) = serde_json::from_str::<Value>(&line) else { continue; };
        if to_browser_tx.send(message).is_err() {
            break;
        }
    }
    writer.abort();
}

#[cfg(windows)]
async fn connect_bridge() -> tokio::net::windows::named_pipe::NamedPipeClient {
    loop {
        if let Ok(pipe) = tokio::net::windows::named_pipe::ClientOptions::new().open(PIPE_NAME) {
            return pipe;
        }
        // The extension may start before PC Snapshot. Keeping this browser-owned
        // host alive avoids polling in a Manifest V3 service worker and lets the
        // next desktop capture use a fresh extension connection immediately.
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

#[cfg(windows)]
fn native_reader(out: tokio::sync::mpsc::UnboundedSender<Value>) {
    use std::io::{self, Read};

    let mut input = io::stdin().lock();
    loop {
        let mut header = [0_u8; 4];
        if input.read_exact(&mut header).is_err() {
            return;
        }
        let size = u32::from_le_bytes(header) as usize;
        if size > MAX_NATIVE_MESSAGE_BYTES {
            eprintln!("native message exceeded product limit");
            return;
        }
        let mut payload = vec![0_u8; size];
        if input.read_exact(&mut payload).is_err() {
            return;
        }
        let Ok(message) = serde_json::from_slice::<Value>(&payload) else {
            eprintln!("native message was not valid JSON");
            continue;
        };
        if out.send(message).is_err() {
            return;
        }
    }
}

#[cfg(windows)]
fn native_writer(input: std::sync::mpsc::Receiver<Value>) {
    use std::io::{self, Write};

    let mut output = io::stdout().lock();
    while let Ok(message) = input.recv() {
        let Ok(payload) = serde_json::to_vec(&message) else { continue; };
        if payload.len() > MAX_NATIVE_MESSAGE_BYTES {
            eprintln!("bridge message exceeded product limit");
            continue;
        }
        let size = payload.len() as u32;
        if output.write_all(&size.to_le_bytes()).is_err()
            || output.write_all(&payload).is_err()
            || output.flush().is_err()
        {
            return;
        }
    }
}
