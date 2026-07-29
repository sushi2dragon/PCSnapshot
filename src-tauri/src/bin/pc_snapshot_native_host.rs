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
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::sync::mpsc;

    let (to_pipe_tx, mut to_pipe_rx) = mpsc::unbounded_channel::<Value>();
    let (to_browser_tx, to_browser_rx) = std::sync::mpsc::channel::<Value>();

    std::thread::spawn(move || native_reader(to_pipe_tx));
    std::thread::spawn(move || native_writer(to_browser_rx));

    // Keep the browser's MV3 service worker alive. A native-messaging port that
    // goes idle for ~30s is torn down along with the worker, after which nothing
    // wakes it until the user manually reloads the extension. A periodic message
    // resets that idle timer, so the companion stays connected and every capture
    // finds a live session. The extension ignores this message type.
    //
    // This starts before the bridge pipe is connected on purpose: the desktop app
    // may be closed for hours, and the worker must survive that so the very next
    // capture finds a live session instead of a torn-down companion.
    let heartbeat_tx = to_browser_tx.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(20));
        if heartbeat_tx
            .send(serde_json::json!({ "protocol_version": 1, "type": "heartbeat" }))
            .is_err()
        {
            return;
        }
    });

    // One browser-owned host process outlives many desktop-app runs. Reconnecting
    // (instead of exiting when the pipe drops) means closing and reopening PC
    // Snapshot no longer leaves the extension silently unregistered until the
    // browser happens to restart the worker. The extension's `hello` is replayed
    // on each new pipe so the fresh bridge re-registers this profile immediately.
    let mut hello: Option<Value> = None;
    loop {
        let pipe = connect_bridge().await;
        let (read, mut write) = tokio::io::split(pipe);
        let mut lines = BufReader::new(read).lines();

        if let Some(greeting) = hello.clone() {
            if write_line(&mut write, &greeting).await.is_err() {
                continue;
            }
        }

        loop {
            tokio::select! {
                outgoing = to_pipe_rx.recv() => {
                    // `None` means the browser closed the port; the reader thread
                    // has already exited the process in that case.
                    let Some(message) = outgoing else { return };
                    if message.get("type").and_then(Value::as_str) == Some("hello") {
                        hello = Some(message.clone());
                    }
                    if write_line(&mut write, &message).await.is_err() {
                        break;
                    }
                }
                incoming = lines.next_line() => {
                    let Ok(Some(line)) = incoming else { break };
                    let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
                    if to_browser_tx.send(message).is_err() {
                        return;
                    }
                }
            }
        }
    }
}

#[cfg(windows)]
async fn write_line<W>(write: &mut W, message: &Value) -> std::io::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;

    let Ok(bytes) = serde_json::to_vec(message) else {
        return Ok(());
    };
    write.write_all(&bytes).await?;
    write.write_all(b"\n").await
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
            // Browser closed the native-messaging port (window/profile closed).
            // The main task is blocked reading the *pipe*, not stdin, so it would
            // otherwise keep this process — and its bridge pipe — alive. That leaves
            // a zombie "connected profile" the desktop app still asks to capture and
            // that never answers. Exit so the pipe drops and the bridge prunes it.
            std::process::exit(0);
        }
        let size = u32::from_le_bytes(header) as usize;
        if size > MAX_NATIVE_MESSAGE_BYTES {
            eprintln!("native message exceeded product limit");
            std::process::exit(0);
        }
        let mut payload = vec![0_u8; size];
        if input.read_exact(&mut payload).is_err() {
            std::process::exit(0);
        }
        let Ok(message) = serde_json::from_slice::<Value>(&payload) else {
            eprintln!("native message was not valid JSON");
            continue;
        };
        if out.send(message).is_err() {
            // Bridge side is gone; nothing left to relay.
            std::process::exit(0);
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
