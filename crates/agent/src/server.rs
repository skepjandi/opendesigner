//! The loopback HTTP transport for MCP. Security is in this file: loopback bind, `Host` and
//! `Origin` checks, a bearer token on every request (`docs/wiki/security.md#mcp-endpoint`).

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::error::AgentError;
use crate::mcp::{Handler, SUPPORTED_VERSIONS};
use crate::token;

/// Default port; the server falls forward when it is taken.
pub const DEFAULT_PORT: u16 = 45790;
const PORT_ATTEMPTS: u16 = 20;
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

struct Shared {
    handler: Arc<Handler>,
    token: String,
    allowed_hosts: Vec<String>,
}

pub struct RunningServer {
    addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    join: JoinHandle<()>,
}

impl RunningServer {
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/mcp", self.addr.port())
    }

    pub async fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = self.join.await;
    }
}

/// Starts the server on `127.0.0.1`, trying `port`, `port + 1`, … (port 0 picks any free port).
pub async fn start(
    handler: Arc<Handler>,
    token: String,
    port: u16,
) -> Result<RunningServer, AgentError> {
    let listener = bind(port).await?;
    let addr = listener
        .local_addr()
        .map_err(|e| AgentError::Server(e.to_string()))?;
    let shared = Arc::new(Shared {
        handler,
        token,
        allowed_hosts: vec![
            format!("127.0.0.1:{}", addr.port()),
            format!("localhost:{}", addr.port()),
        ],
    });
    let app = Router::new()
        .route("/mcp", post(post_mcp).get(no_stream).delete(delete_session))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(middleware::from_fn_with_state(shared.clone(), guard))
        .with_state(shared);
    let (tx, rx) = oneshot::channel::<()>();
    let join = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
    });
    Ok(RunningServer {
        addr,
        shutdown: Some(tx),
        join,
    })
}

async fn bind(port: u16) -> Result<tokio::net::TcpListener, AgentError> {
    let attempts = if port == 0 { 1 } else { PORT_ATTEMPTS };
    let mut last = None;
    for i in 0..attempts {
        let p = port.saturating_add(i);
        match tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, p)).await {
            Ok(l) => return Ok(l),
            Err(e) => last = Some(e),
        }
    }
    Err(AgentError::Server(last.map_or_else(
        || "no port available".into(),
        |e| e.to_string(),
    )))
}

fn reject(status: StatusCode, why: &str) -> Response {
    (status, why.to_string()).into_response()
}

/// Every request passes here before any handler runs.
async fn guard(State(shared): State<Arc<Shared>>, req: Request<Body>, next: Next) -> Response {
    let headers = req.headers();
    // A browser always sends Origin on cross-site requests; no legitimate client of ours is a
    // web page, so any Origin is refused (DNS rebinding and CSRF).
    if headers.contains_key(header::ORIGIN) {
        return reject(StatusCode::FORBIDDEN, "browser origins are not allowed");
    }
    let host_ok = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| {
            shared
                .allowed_hosts
                .iter()
                .any(|a| a.eq_ignore_ascii_case(h))
        });
    if !host_ok {
        return reject(StatusCode::FORBIDDEN, "unexpected Host header");
    }
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));
    let authorized =
        bearer.is_some_and(|t| token::constant_time_eq(t.as_bytes(), shared.token.as_bytes()));
    if !authorized {
        let mut resp = reject(StatusCode::UNAUTHORIZED, "a bearer token is required");
        resp.headers_mut()
            .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return resp;
    }
    if let Some(v) = headers.get("mcp-protocol-version") {
        if !v.to_str().is_ok_and(|v| SUPPORTED_VERSIONS.contains(&v)) {
            return reject(StatusCode::BAD_REQUEST, "unsupported MCP-Protocol-Version");
        }
    }
    next.run(req).await
}

fn session_header(headers: &HeaderMap) -> Option<String> {
    headers
        .get("mcp-session-id")
        .and_then(|h| h.to_str().ok())
        .filter(|s| {
            !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| (0x21..=0x7e).contains(&b))
        })
        .map(str::to_string)
}

async fn post_mcp(State(shared): State<Arc<Shared>>, headers: HeaderMap, body: Bytes) -> Response {
    let Ok(msg) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"parse error"}}"#,
        )
            .into_response();
    };
    let session = session_header(&headers);
    let handler = shared.handler.clone();
    // tools render and write files; keep that off the async workers
    let Ok(outcome) =
        tokio::task::spawn_blocking(move || handler.handle(session.as_deref(), msg)).await
    else {
        return reject(StatusCode::INTERNAL_SERVER_ERROR, "the handler failed");
    };
    let status = StatusCode::from_u16(outcome.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut resp = match outcome.body {
        Some(body) => (
            status,
            [(header::CONTENT_TYPE, "application/json")],
            body.to_string(),
        )
            .into_response(),
        None => status.into_response(),
    };
    if let Some(sid) = outcome.session_id {
        if let Ok(v) = HeaderValue::from_str(&sid) {
            resp.headers_mut().insert("mcp-session-id", v);
        }
    }
    resp
}

/// We offer no server-to-client stream.
async fn no_stream(method: Method) -> Response {
    let mut resp = reject(
        StatusCode::METHOD_NOT_ALLOWED,
        &format!("{method} is not supported; POST only"),
    );
    resp.headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static("POST, DELETE"));
    resp
}

async fn delete_session(State(shared): State<Arc<Shared>>, headers: HeaderMap) -> Response {
    match session_header(&headers) {
        Some(id) if shared.handler.end_session(&id) => StatusCode::OK.into_response(),
        Some(_) => StatusCode::NOT_FOUND.into_response(),
        None => StatusCode::BAD_REQUEST.into_response(),
    }
}
