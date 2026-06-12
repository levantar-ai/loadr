use std::collections::{BTreeMap, HashMap};
use std::io::Write as _;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Json, Response};
use axum::routing::{any, get, post};
use axum::Router;
use base64::Engine as _;
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::TestServerError;

/// In-process HTTP(S) test server backed by axum.
///
/// Spawn with [`HttpTestServer::spawn`] (plain HTTP) or
/// [`HttpTestServer::spawn_tls`] (HTTPS with a freshly generated self-signed
/// certificate, ALPN `h2` + `http/1.1`). The server shuts down when the
/// handle is dropped.
pub struct HttpTestServer {
    /// Bound address (always `127.0.0.1` with an ephemeral port).
    pub addr: SocketAddr,
    scheme: &'static str,
    cert_pem: Option<String>,
    cert_der: Option<Vec<u8>>,
    shutdown: Option<oneshot::Sender<()>>,
}

impl HttpTestServer {
    /// Spawns a plain-HTTP server on `127.0.0.1` with an ephemeral port.
    pub async fn spawn() -> Result<Self, TestServerError> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let (tx, rx) = oneshot::channel::<()>();
        let app = router();
        tokio::spawn(async move {
            let serve = axum::serve(listener, app).with_graceful_shutdown(async move {
                let _ = rx.await;
            });
            if let Err(e) = serve.await {
                tracing::warn!(error = %e, "http test server exited with error");
            }
        });
        tracing::debug!(%addr, "http test server listening");
        Ok(Self {
            addr,
            scheme: "http",
            cert_pem: None,
            cert_der: None,
            shutdown: Some(tx),
        })
    }

    /// Spawns an HTTPS server using a self-signed certificate (valid for
    /// `localhost` and `127.0.0.1`). Serves both HTTP/1.1 and h2, advertising
    /// both via ALPN. Use [`cert_pem`](Self::cert_pem) /
    /// [`cert_der`](Self::cert_der) to trust the certificate client-side.
    pub async fn spawn_tls() -> Result<Self, TestServerError> {
        let certified = rcgen::generate_simple_self_signed(vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
        ])
        .map_err(|e| TestServerError::Tls(e.to_string()))?;
        let cert_pem = certified.cert.pem();
        let cert_der = certified.cert.der().to_vec();
        let key_der = certified.signing_key.serialize_der();

        let certs = vec![rustls::pki_types::CertificateDer::from(cert_der.clone())];
        let key = rustls::pki_types::PrivateKeyDer::Pkcs8(
            rustls::pki_types::PrivatePkcs8KeyDer::from(key_der),
        );
        let mut config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| TestServerError::Tls(e.to_string()))?;
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let (tx, mut rx) = oneshot::channel::<()>();
        let app = router();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut rx => break,
                    accepted = listener.accept() => {
                        let (stream, peer) = match accepted {
                            Ok(pair) => pair,
                            Err(e) => {
                                tracing::warn!(error = %e, "tls test server accept failed");
                                continue;
                            }
                        };
                        let acceptor = acceptor.clone();
                        let service = hyper_util::service::TowerToHyperService::new(app.clone());
                        tokio::spawn(async move {
                            let tls = match acceptor.accept(stream).await {
                                Ok(tls) => tls,
                                Err(e) => {
                                    tracing::debug!(error = %e, %peer, "tls handshake failed");
                                    return;
                                }
                            };
                            let io = hyper_util::rt::TokioIo::new(tls);
                            let builder = hyper_util::server::conn::auto::Builder::new(
                                hyper_util::rt::TokioExecutor::new(),
                            );
                            if let Err(e) =
                                builder.serve_connection_with_upgrades(io, service).await
                            {
                                tracing::debug!(error = %e, %peer, "tls connection error");
                            }
                        });
                    }
                }
            }
            tracing::debug!("tls test server stopped");
        });
        tracing::debug!(%addr, "tls test server listening");
        Ok(Self {
            addr,
            scheme: "https",
            cert_pem: Some(cert_pem),
            cert_der: Some(cert_der),
            shutdown: Some(tx),
        })
    }

    /// Base URL without a trailing slash, e.g. `http://127.0.0.1:54321`.
    pub fn base_url(&self) -> String {
        format!("{}://{}", self.scheme, self.addr)
    }

    /// Full URL for `path`, e.g. `server.url("/echo")`.
    pub fn url(&self, path: &str) -> String {
        let sep = if path.starts_with('/') { "" } else { "/" };
        format!("{}://{}{}{}", self.scheme, self.addr, sep, path)
    }

    /// PEM-encoded self-signed certificate (TLS servers only).
    pub fn cert_pem(&self) -> Option<&str> {
        self.cert_pem.as_deref()
    }

    /// DER-encoded self-signed certificate (TLS servers only).
    pub fn cert_der(&self) -> Option<&[u8]> {
        self.cert_der.as_deref()
    }

    /// Stops the server. Also happens automatically on drop.
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for HttpTestServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Clone, Default)]
struct AppState {
    counter: Arc<AtomicU64>,
}

fn router() -> Router {
    Router::new()
        .route("/", get(root))
        .route("/echo", any(echo))
        .route("/status/{code}", get(status_code))
        .route("/delay/{ms}", get(delay))
        .route("/json", get(json_doc))
        .route("/xml", get(xml_doc))
        .route("/html", get(html_doc))
        .route("/cookies/set", get(cookies_set))
        .route("/cookies", get(cookies_get))
        .route("/gzip", get(gzip))
        .route("/redirect/{n}", get(redirect_n))
        .route("/login", post(login))
        .route("/large/{kb}", get(large))
        .route("/headers", get(headers_echo))
        .route("/counter", get(counter))
        .with_state(AppState::default())
}

async fn root() -> &'static str {
    "Welcome to loadr-testserver"
}

#[derive(Serialize)]
struct EchoView {
    method: String,
    path: String,
    query: HashMap<String, String>,
    headers: BTreeMap<String, String>,
    body: String,
    body_base64: String,
}

async fn echo(
    method: Method,
    uri: Uri,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> Json<EchoView> {
    Json(EchoView {
        method: method.to_string(),
        path: uri.path().to_string(),
        query,
        headers: header_map(&headers),
        body: String::from_utf8_lossy(&body).into_owned(),
        body_base64: base64::engine::general_purpose::STANDARD.encode(&body),
    })
}

async fn status_code(Path(code): Path<u16>) -> StatusCode {
    StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST)
}

async fn delay(Path(ms): Path<u64>) -> String {
    tokio::time::sleep(Duration::from_millis(ms)).await;
    format!("delayed {ms}ms")
}

async fn json_doc() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "items": [
            {"id": 1, "name": "alpha"},
            {"id": 2, "name": "beta"}
        ],
        "token": "tok-123",
        "nested": {"deep": {"value": 42}}
    }))
}

const XML_DOC: &str = r#"<catalog><item id="1"><name>alpha</name></item><item id="2"><name>beta</name></item></catalog>"#;

async fn xml_doc() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/xml")], XML_DOC)
}

async fn html_doc() -> Html<&'static str> {
    Html(
        r#"<!DOCTYPE html>
<html>
<head><title>loadr-testserver</title></head>
<body>
<h1>loadr-testserver</h1>
<form action="/login" method="post">
<input name="csrf" value="csrf-token-xyz">
<input name="username">
<input name="password" type="password">
<button type="submit">Login</button>
</form>
<a href="/json">json</a>
<a href="/xml">xml</a>
</body>
</html>
"#,
    )
}

fn request_cookies(headers: &HeaderMap) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for value in headers.get_all(header::COOKIE) {
        if let Ok(raw) = value.to_str() {
            for c in cookie::Cookie::split_parse(raw.to_owned()).flatten() {
                map.insert(c.name().to_string(), c.value().to_string());
            }
        }
    }
    map
}

async fn cookies_get(headers: HeaderMap) -> Json<BTreeMap<String, String>> {
    Json(request_cookies(&headers))
}

async fn cookies_set(
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let mut response = Json(request_cookies(&headers)).into_response();
    for (name, value) in params {
        let c = cookie::Cookie::build((name, value)).path("/").build();
        match HeaderValue::from_str(&c.to_string()) {
            Ok(hv) => {
                response.headers_mut().append(header::SET_COOKIE, hv);
            }
            Err(e) => {
                return (StatusCode::BAD_REQUEST, format!("invalid cookie: {e}")).into_response();
            }
        }
    }
    response
}

async fn gzip() -> Response {
    let compress = || -> std::io::Result<Vec<u8>> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(br#"{"compressed":true}"#)?;
        encoder.finish()
    };
    match compress() {
        Ok(body) => (
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CONTENT_ENCODING, "gzip"),
            ],
            body,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("gzip encoding failed: {e}"),
        )
            .into_response(),
    }
}

async fn redirect_n(Path(n): Path<u32>) -> Response {
    if n == 0 {
        (StatusCode::OK, "done").into_response()
    } else {
        (
            StatusCode::FOUND,
            [(header::LOCATION, format!("/redirect/{}", n - 1))],
        )
            .into_response()
    }
}

#[derive(serde::Deserialize, Default)]
struct LoginRequest {
    username: Option<String>,
    password: Option<String>,
}

async fn login(headers: HeaderMap, body: Bytes) -> Response {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let (username, password) = if content_type.starts_with("application/json") {
        match serde_json::from_slice::<LoginRequest>(&body) {
            Ok(req) => (req.username, req.password),
            Err(e) => {
                return (StatusCode::BAD_REQUEST, format!("invalid json: {e}")).into_response();
            }
        }
    } else {
        let text = String::from_utf8_lossy(&body);
        let mut username = None;
        let mut password = None;
        for (key, value) in parse_form(&text) {
            match key.as_str() {
                "username" => username = Some(value),
                "password" => password = Some(value),
                _ => {}
            }
        }
        (username, password)
    };
    let (Some(_username), Some(_password)) = (username, password) else {
        return (StatusCode::BAD_REQUEST, "missing username or password").into_response();
    };
    let token = format!("sess-{}", random_hex(16));
    let session = cookie::Cookie::build(("session", token.clone()))
        .path("/")
        .http_only(true)
        .build();
    let mut response = Json(serde_json::json!({ "token": token })).into_response();
    if let Ok(hv) = HeaderValue::from_str(&session.to_string()) {
        response.headers_mut().append(header::SET_COOKIE, hv);
    }
    response
}

async fn large(Path(kb): Path<usize>) -> Response {
    const MAX_KB: usize = 64 * 1024;
    if kb > MAX_KB {
        return (
            StatusCode::BAD_REQUEST,
            format!("kb must be <= {MAX_KB}, got {kb}"),
        )
            .into_response();
    }
    (
        [(header::CONTENT_TYPE, "application/octet-stream")],
        vec![b'x'; kb * 1024],
    )
        .into_response()
}

async fn headers_echo(headers: HeaderMap) -> Json<BTreeMap<String, String>> {
    Json(header_map(&headers))
}

async fn counter(State(state): State<AppState>) -> String {
    let count = state.counter.fetch_add(1, Ordering::SeqCst) + 1;
    count.to_string()
}

fn header_map(headers: &HeaderMap) -> BTreeMap<String, String> {
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for (name, value) in headers {
        let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
        map.entry(name.as_str().to_string())
            .and_modify(|existing| {
                existing.push_str(", ");
                existing.push_str(&value);
            })
            .or_insert(value);
    }
    map
}

fn parse_form(input: &str) -> Vec<(String, String)> {
    input
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (form_decode(key), form_decode(value))
        })
        .collect()
}

fn form_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                    out.push(hi * 16 + lo);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn random_hex(len: usize) -> String {
    use rand::RngExt as _;
    let mut rng = rand::rng();
    (0..len)
        .map(|_| format!("{:02x}", rng.random::<u8>()))
        .collect()
}
