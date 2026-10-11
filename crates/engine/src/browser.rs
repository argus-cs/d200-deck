//! Talks to the Edge extension over a WebSocket on 127.0.0.1. The extension
//! reports its tabs; the engine asks it to select a tab before a key that
//! targets a site. Only our extension is accepted, by its exact Origin: a web
//! page cannot set an extension Origin header, and another extension has
//! another id, so neither can feed or drive this. Outside a browser the
//! Origin is just text, and 127.0.0.1 is shared by every user signed in to the
//! PC, so the connection must also come from Edge running as this user.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use log::{debug, info, warn};
use serde::Deserialize;
use serde_json::json;
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::StatusCode;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Error as WsError, Message};

use crate::rules::{Tab, Tabs};

/// Must match `PORT` in extension-edge/background.js.
pub const PORT: u16 = 47820;
/// Comes from the `key` in extension-edge/manifest.json: a new key is a new id
/// (a test checks they match).
pub const EXTENSION_ID: &str = "jfahhgamoiaijnagckhknnanoobeaiko";
const POLL: Duration = Duration::from_millis(50);
/// A connection that doesn't finish the handshake by then is dropped.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// The tab list of hundreds of tabs is a few tens of KB.
const MAX_MESSAGE: usize = 1 << 20;
/// The extension holds one; a few more cover reconnects overlapping.
const MAX_CONNECTIONS: usize = 4;
const ACK_TIMEOUT: Duration = Duration::from_secs(1);

const FAVICON_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum FromExtension {
    Tabs { active: Option<Tab>, open: Vec<Tab> },
    Ack { id: u64, ok: bool },
    /// A tab's icon as base64 PNG; `None` when Edge has none for it.
    Favicon { id: u64, data: Option<String> },
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
    favicons: Mutex<HashMap<u64, Sender<Option<String>>>>,
    next_id: AtomicU64,
    /// Connections being served now.
    connections: AtomicUsize,
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
            // One thread per connection: refuse more than a few at once.
            if server.inner.connections.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                server.inner.connections.fetch_sub(1, Ordering::SeqCst);
                debug!("conexão recusada: já há {MAX_CONNECTIONS} abertas");
                continue;
            }
            let (bridge, tabs) = (server.clone(), tabs_tx.clone());
            std::thread::spawn(move || {
                if let Err(e) = bridge.serve(stream, &tabs) {
                    debug!("conexão da extensão encerrada: {e:#}");
                }
                bridge.inner.connections.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    (bridge, tabs_rx)
}

impl Bridge {
    pub fn is_connected(&self) -> bool {
        self.inner.outgoing.lock().unwrap().is_some()
    }

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

    /// The icon Edge keeps for a page, as PNG bytes. Edge answers from its
    /// own cache: the site itself is not contacted.
    pub fn favicon(&self, url: &str) -> Result<Vec<u8>> {
        use base64::Engine as _;

        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        self.inner.favicons.lock().unwrap().insert(id, tx);
        let message = json!({ "type": "favicon", "id": id, "url": url }).to_string();
        let sent = self.inner.outgoing.lock().unwrap().as_ref().is_some_and(|(_, out)| out.send(message).is_ok());
        let answer = if sent { rx.recv_timeout(FAVICON_TIMEOUT).ok() } else { None };
        self.inner.favicons.lock().unwrap().remove(&id);
        match (sent, answer) {
            (false, _) => bail!("a extensão do Edge não está conectada"),
            (true, None) => bail!("a extensão do Edge não respondeu (recarregue-a em edge://extensions depois de atualizar)"),
            (true, Some(None)) => bail!("o Edge não tem um ícone guardado para esse site"),
            (true, Some(Some(data))) => Ok(base64::engine::general_purpose::STANDARD.decode(data)?),
        }
    }

    fn serve(&self, stream: TcpStream, tabs: &Sender<Tabs>) -> Result<()> {
        if let Err(e) = check_client(&stream) {
            warn!("conexão recusada: {e:#}");
            return Err(e);
        }
        stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
        let limits = WebSocketConfig::default().max_message_size(Some(MAX_MESSAGE)).max_frame_size(Some(MAX_MESSAGE));
        let mut socket = tungstenite::accept_hdr_with_config(stream, only_extensions, Some(limits))?;
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
            Ok(FromExtension::Favicon { id, data }) => {
                if let Some(waiter) = self.inner.favicons.lock().unwrap().remove(&id) {
                    let _ = waiter.send(data);
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
    if is_our_extension(origin) {
        return Ok(response);
    }
    warn!("conexão recusada de {origin:?}: só a extensão do D200 Deck pode se conectar");
    let mut refused = ErrorResponse::new(Some("only the D200 Deck extension may connect".into()));
    *refused.status_mut() = StatusCode::FORBIDDEN;
    Err(refused)
}

/// Edge shows extension pages as `extension://`; Chromium's own scheme is
/// `chrome-extension://`.
fn is_our_extension(origin: &str) -> bool {
    ["chrome-extension://", "extension://"].iter().any(|scheme| origin.strip_prefix(scheme) == Some(EXTENSION_ID))
}

/// The connection must come from Edge (its network process, msedge.exe)
/// running as the user this app runs as: another program, or another user
/// signed in to the PC, could send our extension's Origin too.
#[cfg(windows)]
fn check_client(stream: &TcpStream) -> Result<()> {
    let pid = win::client_of(stream.peer_addr()?.port(), stream.local_addr()?.port())?;
    let path = crate::context::process_path(pid).ok_or_else(|| anyhow!("o processo {pid} não foi identificado"))?;
    let exe = path.rsplit('\\').next().unwrap_or(&path);
    if !exe.eq_ignore_ascii_case("msedge.exe") {
        bail!("veio de {exe} (processo {pid}), não do Edge");
    }
    if !win::same_user(pid)? {
        bail!("veio do Edge de outro usuário do PC (processo {pid})");
    }
    Ok(())
}

#[cfg(not(windows))]
fn check_client(_: &TcpStream) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
mod win {
    use std::net::Ipv4Addr;

    use anyhow::{bail, Result};
    use windows::Win32::Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, HANDLE};
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPTABLE_OWNER_PID, MIB_TCP_STATE_ESTAB, TCP_TABLE_OWNER_PID_CONNECTIONS,
    };
    use windows::Win32::Networking::WinSock::AF_INET;
    use windows::Win32::Security::{EqualSid, GetTokenInformation, TokenUser, PSID, TOKEN_QUERY, TOKEN_USER};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};

    /// The process at the client end of a connection on 127.0.0.1 from
    /// `client_port` to `server_port`, from Windows' TCP table.
    pub fn client_of(client_port: u16, server_port: u16) -> Result<u32> {
        let mut size = 0u32;
        unsafe { GetExtendedTcpTable(None, &mut size, false, AF_INET.0 as u32, TCP_TABLE_OWNER_PID_CONNECTIONS, 0) };
        // u32s keep the table aligned; it may grow between calls, so some room and a retry.
        for _ in 0..3 {
            let mut buffer = vec![0u32; (size as usize).div_ceil(4) + 64];
            size = (buffer.len() * 4) as u32;
            let result = unsafe {
                GetExtendedTcpTable(Some(buffer.as_mut_ptr().cast()), &mut size, false, AF_INET.0 as u32, TCP_TABLE_OWNER_PID_CONNECTIONS, 0)
            };
            if result == ERROR_INSUFFICIENT_BUFFER.0 {
                continue;
            }
            if result != 0 {
                bail!("a tabela de conexões do Windows não foi lida (erro {result})");
            }
            let rows = unsafe {
                let table = &*(buffer.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
                std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize)
            };
            // Ports are in network byte order, in the low 16 bits.
            let port = |raw: u32| u16::from_be(raw as u16);
            let found = rows.iter().find(|row| {
                row.dwState == MIB_TCP_STATE_ESTAB.0 as u32
                    && port(row.dwLocalPort) == client_port
                    && port(row.dwRemotePort) == server_port
                    && Ipv4Addr::from(row.dwRemoteAddr.to_ne_bytes()).is_loopback()
            });
            return match found {
                Some(row) => Ok(row.dwOwningPid),
                None => bail!("a conexão da porta {client_port} não está na tabela do Windows"),
            };
        }
        bail!("a tabela de conexões do Windows não parou de crescer")
    }

    /// Whether the process runs as the same Windows user as this one.
    pub fn same_user(pid: u32) -> Result<bool> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?;
            let theirs = token_user(process);
            let _ = CloseHandle(process);
            let (theirs, ours) = (theirs?, token_user(GetCurrentProcess())?);
            Ok(EqualSid(sid(&theirs), sid(&ours)).is_ok())
        }
    }

    /// A process's TOKEN_USER, in a buffer that holds its SID too.
    unsafe fn token_user(process: HANDLE) -> Result<Vec<u64>> {
        let mut token = HANDLE::default();
        unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token)? };
        let mut size = 0u32;
        let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut size) };
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        let result = unsafe { GetTokenInformation(token, TokenUser, Some(buffer.as_mut_ptr().cast()), size, &mut size) };
        let _ = unsafe { CloseHandle(token) };
        result?;
        Ok(buffer)
    }

    fn sid(token_user: &[u64]) -> PSID {
        unsafe { (*(token_user.as_ptr() as *const TOKEN_USER)).User.Sid }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_extension_gets_in() {
        assert!(is_our_extension("chrome-extension://jfahhgamoiaijnagckhknnanoobeaiko"));
        assert!(is_our_extension("extension://jfahhgamoiaijnagckhknnanoobeaiko"));
        for refused in [
            "",
            "null",
            "https://example.com",
            "http://127.0.0.1:47820",
            "chrome-extension://aaaabbbbccccddddeeeeffffgggghhhh",
            "chrome-extension://jfahhgamoiaijnagckhknnanoobeaiko.example.com",
            "chrome-extension://jfahhgamoiaijnagckhknnanoobeaiko/",
            "moz-extension://jfahhgamoiaijnagckhknnanoobeaiko",
        ] {
            assert!(!is_our_extension(refused), "{refused}");
        }
    }

    /// Chromium's id: the first 16 bytes of the key's SHA-256, each hex digit
    /// written as a letter from a to p.
    #[test]
    fn the_id_comes_from_the_manifest_key() {
        use base64::Engine as _;
        use sha2::{Digest, Sha256};

        let manifest: serde_json::Value = serde_json::from_str(include_str!("../../../extension-edge/manifest.json")).unwrap();
        let key = base64::engine::general_purpose::STANDARD.decode(manifest["key"].as_str().unwrap()).unwrap();
        let id: String = Sha256::digest(&key)[..16].iter().flat_map(|b| [b >> 4, b & 0xF]).map(|n| (b'a' + n) as char).collect();
        assert_eq!(id, EXTENSION_ID);
    }

    #[test]
    fn odd_messages_are_ignored() {
        let (bridge, (tx, rx)) = (Bridge::default(), channel());
        for text in ["", "not json", r#"{"type":"tabs"}"#, r#"{"type":"unknown"}"#, r#"[1,2,3]"#] {
            bridge.handle(text, &tx);
        }
        bridge.handle(r#"{"type":"tabs","active":null,"open":[{"id":1,"url":"https://youtube.com/","title":"YouTube"}]}"#, &tx);
        let tabs: Vec<Tabs> = rx.try_iter().collect();
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].open.len(), 1);
    }

    /// A connection from this test process: found in Windows' table as ours,
    /// same user, and refused for not being Edge.
    #[cfg(windows)]
    #[test]
    fn only_edge_of_this_user_may_connect() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server_side, _) = listener.accept().unwrap();
        let pid = win::client_of(client.local_addr().unwrap().port(), listener.local_addr().unwrap().port()).unwrap();
        assert_eq!(pid, std::process::id());
        assert!(win::same_user(pid).unwrap());
        let refused = check_client(&server_side).unwrap_err().to_string();
        assert!(refused.contains("não do Edge"), "{refused}");
    }
}
