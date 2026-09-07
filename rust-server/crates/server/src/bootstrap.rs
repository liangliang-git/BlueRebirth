use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use super::{current_unix_seconds, ServerError, ServerState};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;

pub(super) struct PrefixedTcpStream {
    prefix: Vec<u8>,
    offset: usize,
    stream: TcpStream,
}

impl PrefixedTcpStream {
    pub(super) fn new(prefix: Vec<u8>, stream: TcpStream) -> Self {
        Self {
            prefix,
            offset: 0,
            stream,
        }
    }
}

impl AsyncRead for PrefixedTcpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.offset < self.prefix.len() {
            let count = (self.prefix.len() - self.offset).min(buf.remaining());
            buf.put_slice(&self.prefix[self.offset..self.offset + count]);
            self.offset += count;
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for PrefixedTcpStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

pub(super) async fn read_connection_prefix(stream: &mut TcpStream) -> io::Result<Option<Vec<u8>>> {
    let mut prefix = [0_u8; 5];
    if stream.read(&mut prefix[..1]).await? == 0 {
        return Ok(None);
    }
    let mut offset = 1;
    while offset < prefix.len() {
        let read = stream.read(&mut prefix[offset..]).await?;
        if read == 0 {
            break;
        }
        offset += read;
    }
    Ok(Some(prefix[..offset].to_vec()))
}

pub(super) fn looks_like_http(prefix: &[u8]) -> bool {
    matches!(prefix.get(..4), Some(b"GET " | b"POST" | b"HEAD"))
}

pub(super) fn looks_like_netsocket(prefix: &[u8]) -> bool {
    if prefix.len() < 5 {
        return false;
    }
    let Some(header) = prefix.get(..4).and_then(|bytes| bytes.try_into().ok()) else {
        return false;
    };
    let length = u32::from_be_bytes(header) as usize;
    length <= blueoath_transport::MAX_FRAME_SIZE && prefix[4] <= 2
}

pub(super) async fn handle_bootstrap_http(
    mut stream: TcpStream,
    prefix: Vec<u8>,
    state: Arc<Mutex<ServerState>>,
    game_login_port: Option<u16>,
) -> Result<(), ServerError> {
    let mut request = prefix;
    let mut chunk = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        if request.len() >= 64 * 1024 {
            break;
        }
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
    }
    let text = String::from_utf8_lossy(&request);
    let mut lines = text.lines();
    let request_line = lines.next().unwrap_or_default();
    let host = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("host").then_some(value.trim())
    });
    let snapshot = state
        .lock()
        .map_err(|_| ServerError::InvalidMessage("state mutex poisoned".to_owned()))?
        .clone();
    let (status, reason, content_type, body) =
        bootstrap_response(request_line, host, &snapshot, game_login_port);
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

pub(super) fn bootstrap_response(
    request_line: &str,
    host: Option<&str>,
    state: &ServerState,
    game_login_port: Option<u16>,
) -> (u16, &'static str, &'static str, String) {
    let profile_id =
        serde_json::to_string(&state.profile_id).unwrap_or_else(|_| "\"local-player\"".to_owned());
    let version = serde_json::to_string(&state.version).unwrap_or_else(|_| "\"1.4.0\"".to_owned());
    let game_port = game_login_port.unwrap_or(7201);
    let commander_level = state.level.max(1);
    let lower = request_line.to_ascii_lowercase();
    if host.is_some_and(|value| {
        ["ifconfig.io", "ipify.org", "ipinfo.io", "3322.net"]
            .iter()
            .any(|suffix| value.to_ascii_lowercase().contains(suffix))
    }) {
        return (
            200,
            "OK",
            "text/plain; charset=utf-8",
            "203.0.113.1".to_owned(),
        );
    }
    if lower.contains("/phone/switch/getstate") {
        return (
            200,
            "OK",
            "application/json; charset=utf-8",
            "{\"errornu\":\"0\",\"errordesc\":\"\",\"DNS_sw\":{\"state\":1}}".to_owned(),
        );
    }
    if lower.contains("/sdk/gettime") {
        return (
            200,
            "OK",
            "application/json; charset=utf-8",
            format!("{{\"time\":{}}}", current_unix_seconds()),
        );
    }
    if lower.contains("/phone/applereview") {
        return (
            200,
            "OK",
            "application/json; charset=utf-8",
            "{\"errornu\":\"0\",\"applereview\":1}".to_owned(),
        );
    }
    if lower.contains("/phone/getversion/") {
        return (200, "OK", "application/json; charset=utf-8", format!(
            "{{\"errornu\":\"0\",\"script\":[{{\"pl\":\"google_windows\",\"os\":\"android\",\"groupbase\":\"\",\"gn\":\"jpshipgirl\",\"path\":\"\",\"src_version\":{version},\"tar_version\":{version},\"updateType\":\"0\",\"file\":\"\",\"total_size\":0,\"sizes\":[],\"forceExit\":0,\"forceUpdate\":0}}],\"static_url\":\"\",\"spare_static_url\":\"\"}}"
        ));
    }
    if lower.contains("/phone/getpldata/getpldata") {
        return (200, "OK", "application/json; charset=utf-8", format!(
            "{{\"errornu\":\"0\",\"errordesc\":\"\",\"data\":{{\"networkCheck\":\"1\",\"uuid\":\"00000000-0000-4000-8000-000000000001\",\"pid\":{profile_id},\"serverId\":\"jp\",\"pl\":\"google_windows\",\"os\":\"android\",\"gn\":\"jpshipgirl\",\"sensorInfo\":\"\",\"localInfo\":\"\",\"timeZoneId\":\"\",\"screenWidth\":\"1920\",\"screenHeight\":\"1080\",\"dangerWidth\":\"0\",\"strDeviceInfo\":\"\",\"noticeBoard\":{{}}}}}}"
        ));
    }
    if lower.contains("/login?") {
        return (200, "OK", "application/json; charset=utf-8", format!(
            "{{\"errornu\":0,\"errordesc\":\"\",\"Pid\":{profile_id},\"UID\":{profile_id},\"uid\":{profile_id},\"uuid\":\"00000000-0000-4000-8000-000000000001\",\"token\":\"local-token\",\"openid\":{profile_id},\"ServerID\":\"jp\",\"serverid\":\"jp\",\"newuser\":\"0\",\"qid\":\"1\",\"id\":\"1\"}}"
        ));
    }
    if lower.contains("/gethash") {
        return (200, "OK", "application/json; charset=utf-8", format!(
            "{{\"errornu\":\"0\",\"errordesc\":\"\",\"pid\":{profile_id},\"serverID\":\"game1\",\"feignRoleId\":\"1\",\"qid\":\"1\",\"uuid\":\"00000000-0000-4000-8000-000000000001\",\"offset\":\"0\",\"host\":\"127.0.0.1\",\"port\":{game_port}}}"
        ));
    }
    if lower.contains("/phone/serverlist/") {
        return (200, "OK", "application/json; charset=utf-8", format!(
            "{{\"errornu\":\"0\",\"errordesc\":\"\",\"root\":{{\"notice\":{{\"open\":0,\"desc\":\"\"}},\"item\":[{{\"name\":\"BlueoathRebirth\",\"serverIndex\":1,\"new\":0,\"groupid\":\"1\",\"openDateTime\":\"20171109140000\",\"status\":1,\"hot\":0,\"level\":{commander_level},\"host\":\"127.0.0.1\",\"port\":{game_port},\"recommend_weight\":1}}]}}}}"
        ));
    }
    if lower.contains("/phone/loginrole/") {
        return (200, "OK", "application/json; charset=utf-8", format!(
            "{{\"errornu\":\"0\",\"errordesc\":\"\",\"root\":{{\"role\":[{{\"name\":\"BlueoathRebirth\",\"serverIndex\":1,\"groupid\":\"1\",\"serverId\":\"1\",\"level\":{commander_level},\"host\":\"127.0.0.1\",\"port\":{game_port},\"status\":1,\"openDateTime\":\"20171109140000\"}}]}}}}"
        ));
    }
    if lower.contains("/phone/platform/getplatformuserinfo") {
        return (200, "OK", "application/json; charset=utf-8", "{\"errornu\":\"0\",\"errordesc\":\"\",\"data\":{\"isFastUser\":1,\"idcardStatus\":1,\"isAdult\":1,\"OnNoRealnameLogin\":0}}".to_owned());
    }
    if lower.contains("/phone/platform/getplatformext") {
        return (
            200,
            "OK",
            "application/json; charset=utf-8",
            "{\"errornu\":\"0\",\"errordesc\":\"\",\"data\":{}}".to_owned(),
        );
    }
    if lower.contains("/phone/platform/getgamemaintainnotice") {
        return (
            200,
            "OK",
            "application/json; charset=utf-8",
            "{\"errornu\":\"0\",\"errordesc\":\"\",\"data\":[]}".to_owned(),
        );
    }
    if lower.contains("/phone/innerbrowse") {
        return (
            200,
            "OK",
            "application/json; charset=utf-8",
            "{\"errornu\":\"0\",\"errordesc\":\"\",\"noticear\":[]}".to_owned(),
        );
    }
    if lower.contains("/phone/getuserextra/") {
        return (200, "OK", "application/json; charset=utf-8", "{\"errornu\":\"0\",\"errordesc\":\"\",\"data\":{\"userInfo\":{\"readQuestion\":0},\"payBack\":{\"returnGold\":0,\"returnMonthCard\":0},\"oldUser\":{\"returnUserReceiveGift\":0}}}".to_owned());
    }
    if lower.contains("/c.gif")
        || host.is_some_and(|value| value.to_ascii_lowercase().starts_with("static"))
    {
        return (200, "OK", "text/plain; charset=utf-8", "ok".to_owned());
    }
    (
        501,
        "Not Implemented",
        "text/plain; charset=utf-8",
        String::new(),
    )
}
