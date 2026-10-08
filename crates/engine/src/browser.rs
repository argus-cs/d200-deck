//! Talks to the Edge extension over a WebSocket on 127.0.0.1. The extension
//! reports its tabs; the engine asks it to select a tab before a key that
//! targets a site. Only browser extensions are accepted: a web page cannot
//! set an extension Origin header, so no site can feed or drive this.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Result};
use log::{debug, info, warn};
use serde::Deserialize;
use serde_json::json;
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::StatusCode;
use tungstenite::{Error as WsError, Message};

use crate::rules::{Tab, Tabs};

/// Must match `PORT` in extension-edge/background.js.
pub const PORT: u16 = 47820;
const POLL: Duration = Duration::from_millis(50);
const ACK_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum FromExtension {
    Tabs { active: Option<Tab>, open: Vec<Tab> },
    Ack { id: u64, ok: bool },
    Keepalive,
}

#[derive(Clone, Default)]
pub struct Bridge {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Messages for the connected extension, if any, with its connection id.
    outgoing: Mutex<Option<(u64, Sender<String>)>>,
    /// Requests waiting for the extension's answer, by id.
    waiting: Mutex<HashMap<u64, Sender<bool>>>,
    next_id: AtomicU64,
}

/// Starts listening; tab updates arrive on the receiver.
pub fn start() -> (Bridge, Receiver<Tabs>) {
    let (tabs_tx, tabs_rx) = channel();
    let bridge = Bridge::default();
    let server = bridge.clone();
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(("127.0.0.1", PORT)) {
            Ok(listener) => listener,
            Err(e) => {
                warn!("regras de site desligadas: a porta {PORT} não abriu ({e})");
                return;
            }
        };
        for stream in listener.incoming().flatten() {
            let (bridge, tabs) = (server.clone(), tabs_tx.clone());
            std::thread::spawn(move || {
                if let Err(e) = bridge.serve(stream, &tabs) {
                    debug!("conexão da extensão encerrada: {e:#}");
                }
            });
        }
    });
    (bridge, tabs_rx)
}

impl Bridge {
    /// Selects a tab and focuses its Edge window; waits for the extension to confirm.
    pub fn activate(&self, tab: i64) -> Result<()> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        self.inner.waiting.lock().unwrap().insert(id, tx);
        let message = json!({ "type": "activate", "id": id, "tab": tab }).to_string();
        let sent = self.inner.outgoing.lock().unwrap().as_ref().is_some_and(|(_, out)| out.send(message).is_ok());
        let answer = if sent { rx.recv_timeout(ACK_TIMEOUT).ok() } else { None };
        self.inner.waiting.lock().unwrap().remove(&id);
        match (sent, answer) {
            (false, _) => bail!("a extensão do Edge não está conectada"),
            (true, None) => bail!("a extensão do Edge não respondeu"),
            (true, Some(false)) => bail!("a aba {tab} não existe mais"),
            (true, Some(true)) => Ok(()),
        }
    }

    fn serve(&self, stream: TcpStream, tabs: &Sender<Tabs>) -> Result<()> {
        let mut socket = tungstenite::accept_hdr(stream, only_extensions)?;
        // Short reads let this thread also deliver outgoing messages.
        socket.get_mut().set_read_timeout(Some(POLL))?;
        let (tx, rx) = channel::<String>();
        let connection = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        *self.inner.outgoing.lock().unwrap() = Some((connection, tx));
        info!("extensão do Edge conectada");
        let result = loop {
            if let Err(e) = rx.try_iter().try_for_each(|text| socket.send(Message::text(text))) {
                break Err(e.into());
            }
            match socket.read() {
                Ok(Message::Text(text)) => self.handle(text.as_str(), tabs),
                Ok(Message::Close(_)) => break Ok(()),
                Ok(_) => {}
                Err(WsError::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(e) => break Err(e.into()),
            }
        };
        let mut outgoing = self.inner.outgoing.lock().unwrap();
        // A newer connection may already have taken over.
        if outgoing.as_ref().is_some_and(|(current, _)| *current == connection) {
            *outgoing = None;
            let _ = tabs.send(Tabs::default());
            info!("extensão do Edge desconectada");
        }
        result
    }

    fn handle(&self, text: &str, tabs: &Sender<Tabs>) {
        match serde_json::from_str::<FromExtension>(text) {
            Ok(FromExtension::Tabs { active, open }) => {
                let _ = tabs.send(Tabs { active, open });
            }
            Ok(FromExtension::Ack { id, ok }) => {
                if let Some(waiter) = self.inner.waiting.lock().unwrap().remove(&id) {
                    let _ = waiter.send(ok);
                }
            }
            Ok(FromExtension::Keepalive) => {}
            Err(e) => warn!("mensagem da extensão ignorada: {e}"),
        }
    }
}

#[allow(clippy::result_large_err)]
fn only_extensions(request: &Request, response: Response) -> Result<Response, ErrorResponse> {
    let origin = request.headers().get("Origin").and_then(|v| v.to_str().ok()).unwrap_or_default();
    if origin.starts_with("chrome-extension://") || origin.starts_with("extension://") {
        return Ok(response);
    }
    warn!("conexão recusada de {origin:?}: só a extensão do Edge pode se conectar");
    let mut refused = ErrorResponse::new(Some("only the D200 Deck extension may connect".into()));
    *refused.status_mut() = StatusCode::FORBIDDEN;
    Err(refused)
}
