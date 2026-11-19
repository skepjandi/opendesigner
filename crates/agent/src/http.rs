//! A small HTTP/1.1 client for the few places the app reaches the network: one `GET`, one
//! answer, no cookies, no cache, no keep-alive.
//!
//! The rules live here so they can be read in one place: a connect and a read timeout, a cap on
//! the answer's size, and a redirect that may only lead to the same site as the request that
//! started it. The secure channel is `rustls`; the request and the answer are ours.
//!
//! Nothing in an error message carries the query string, because a query string is where an API
//! token goes.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use thiserror::Error;

/// How long a connection may take to open.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a single read may wait.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// How many redirects may be followed.
const MAX_REDIRECTS: usize = 3;
/// The most bytes a header block may take.
const MAX_HEADER_BYTES: usize = 16 * 1024;

#[derive(Debug, Error)]
pub enum HttpError {
    #[error("{0}")]
    Url(String),
    #[error("cannot reach {host}: {source}")]
    Connect {
        host: String,
        #[source]
        source: std::io::Error,
    },
    #[error("the secure connection to {host} failed: {source}")]
    Tls {
        host: String,
        #[source]
        source: std::io::Error,
    },
    #[error("the answer from {host} is not HTTP: {detail}")]
    Protocol { host: String, detail: String },
    #[error("the answer from {host} is larger than {limit} bytes")]
    TooLarge { host: String, limit: u64 },
    #[error("{host} answered {status}")]
    Status { host: String, status: u16 },
    #[error("{host} points to {to}, which is not the same site")]
    Redirect { host: String, to: String },
    #[error("{host} points to itself too many times")]
    TooManyRedirects { host: String },
}

/// The status, the headers and the body of an answer.
type Answer = (u16, Vec<(String, String)>, Vec<u8>);

/// What a `GET` brought back.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

/// A URL split into the parts a request needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub secure: bool,
    pub host: String,
    pub port: u16,
    /// The path and the query, exactly as they go on the request line.
    pub path: String,
}

impl Url {
    /// Whether `other` is the same site: the same host, or a host under it.
    pub fn same_site(&self, other: &Url) -> bool {
        self.host == other.host
            || other.host.ends_with(&format!(".{}", self.host))
            || self.host.ends_with(&format!(".{}", other.host))
    }
}

/// Splits a URL into its parts, refusing anything that is not `http` or `https` with a plain
/// host name.
pub fn parse_url(url: &str) -> Result<Url, HttpError> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| HttpError::Url("the address must start with http:// or https://".into()))?;
    let secure = match scheme {
        "http" => false,
        "https" => true,
        _ => return Err(HttpError::Url("only http and https are used".into())),
    };
    let (authority, path) = match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, "/"),
    };
    if authority.contains('@') {
        return Err(HttpError::Url(
            "the address must not carry a user name or password".into(),
        ));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => {
            let port = p
                .parse::<u16>()
                .map_err(|_| HttpError::Url("the port is not a number".into()))?;
            (h, port)
        }
        None => (authority, if secure { 443 } else { 80 }),
    };
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        || host.starts_with('.')
        || host.ends_with('.')
        || host.contains("..")
    {
        return Err(HttpError::Url("the host name is not plain".into()));
    }
    if !path.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        return Err(HttpError::Url(
            "the path must be printable ASCII without spaces".into(),
        ));
    }
    Ok(Url {
        secure,
        host: host.to_string(),
        port,
        path: path.to_string(),
    })
}

/// Percent-encodes a value for a query string: everything but the unreserved characters of
/// RFC 3986 is escaped.
pub fn encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Asks for `url` and returns the answer. Redirects are followed while they stay on the same
/// site; an answer larger than `max_bytes` is refused.
pub fn get(url: &str, max_bytes: u64) -> Result<Response, HttpError> {
    let mut target = parse_url(url)?;
    for _ in 0..=MAX_REDIRECTS {
        let (status, headers, body) = request(&target, max_bytes)?;
        if is_redirect(status) {
            let location = headers
                .iter()
                .find(|(name, _)| name == "location")
                .map(|(_, value)| value.as_str())
                .ok_or_else(|| HttpError::Protocol {
                    host: target.host.clone(),
                    detail: "it points somewhere without saying where".into(),
                })?;
            let next = resolve(&target, location)?;
            if !target.same_site(&next) || (target.secure && !next.secure) {
                return Err(HttpError::Redirect {
                    host: target.host.clone(),
                    to: next.host.clone(),
                });
            }
            target = next;
            continue;
        }
        let content_type = headers
            .iter()
            .find(|(name, _)| name == "content-type")
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        return Ok(Response {
            status,
            content_type,
            body,
        });
    }
    Err(HttpError::TooManyRedirects { host: target.host })
}

fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// Resolves a `Location` against the address it came from.
fn resolve(from: &Url, location: &str) -> Result<Url, HttpError> {
    if location.contains("://") {
        return parse_url(location);
    }
    let (path, query) = match location.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (location, None),
    };
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let path = match query {
        Some(q) => format!("{path}?{q}"),
        None => path,
    };
    Ok(Url {
        secure: from.secure,
        host: from.host.clone(),
        port: from.port,
        path,
    })
}

/// One request and its answer.
fn request(url: &Url, max_bytes: u64) -> Result<Answer, HttpError> {
    let address = (url.host.as_str(), url.port)
        .to_socket_addrs()
        .map_err(|e| HttpError::Connect {
            host: url.host.clone(),
            source: e,
        })?
        .next()
        .ok_or_else(|| HttpError::Connect {
            host: url.host.clone(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "no address"),
        })?;
    let stream =
        TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).map_err(|e| HttpError::Connect {
            host: url.host.clone(),
            source: e,
        })?;
    stream
        .set_read_timeout(Some(READ_TIMEOUT))
        .map_err(|e| HttpError::Connect {
            host: url.host.clone(),
            source: e,
        })?;
    stream
        .set_write_timeout(Some(READ_TIMEOUT))
        .map_err(|e| HttpError::Connect {
            host: url.host.clone(),
            source: e,
        })?;
    let host = if (url.secure && url.port == 443) || (!url.secure && url.port == 80) {
        url.host.clone()
    } else {
        format!("{}:{}", url.host, url.port)
    };
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: OpenDesigner/{version}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        path = url.path,
        host = host,
        version = env!("CARGO_PKG_VERSION"),
    );
    if url.secure {
        let mut tls = secure_stream(stream, &url.host)?;
        tls.write_all(request.as_bytes())
            .and_then(|()| tls.flush())
            .map_err(|e| HttpError::Tls {
                host: url.host.clone(),
                source: e,
            })?;
        let mut reader = Buffered::new(tls, &url.host);
        read_answer(&mut reader, &url.host, max_bytes)
    } else {
        let mut stream = stream;
        stream
            .write_all(request.as_bytes())
            .and_then(|()| stream.flush())
            .map_err(|e| HttpError::Connect {
                host: url.host.clone(),
                source: e,
            })?;
        let mut reader = Buffered::new(stream, &url.host);
        read_answer(&mut reader, &url.host, max_bytes)
    }
}

/// The secure channel: `rustls` with the roots the operating system trusts.
fn secure_stream(
    stream: TcpStream,
    host: &str,
) -> Result<StreamOwned<ClientConnection, TcpStream>, HttpError> {
    let mut roots = RootCertStore::empty();
    let native = rustls_native_certs::load_native_certs();
    let (added, ignored) = roots.add_parsable_certificates(native.certs);
    if added == 0 && ignored > 0 {
        return Err(HttpError::Tls {
            host: host.to_string(),
            source: std::io::Error::other("no root certificate could be read"),
        });
    }
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let name = ServerName::try_from(host.to_string()).map_err(|_| HttpError::Tls {
        host: host.to_string(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "bad host name"),
    })?;
    let connection = ClientConnection::new(Arc::new(config), name).map_err(|e| HttpError::Tls {
        host: host.to_string(),
        source: std::io::Error::other(e),
    })?;
    Ok(StreamOwned::new(connection, stream))
}

/// Reads the status line, the headers and the body.
fn read_answer<R: Read>(
    reader: &mut Buffered<R>,
    host: &str,
    max_bytes: u64,
) -> Result<Answer, HttpError> {
    let status_line = String::from_utf8_lossy(&reader.line(MAX_HEADER_BYTES)?).into_owned();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| HttpError::Protocol {
            host: host.to_string(),
            detail: "the status line is not readable".into(),
        })?;
    let mut headers = Vec::new();
    let mut header_bytes = status_line.len();
    loop {
        let line = reader.line(MAX_HEADER_BYTES)?;
        header_bytes += line.len();
        if header_bytes > MAX_HEADER_BYTES {
            return Err(HttpError::TooLarge {
                host: host.to_string(),
                limit: MAX_HEADER_BYTES as u64,
            });
        }
        if line.is_empty() {
            break;
        }
        let line = String::from_utf8_lossy(&line).into_owned();
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    let chunked = headers
        .iter()
        .any(|(name, value)| name == "transfer-encoding" && value.contains("chunked"));
    let length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse::<u64>().ok());
    let body = if chunked {
        reader.chunked(max_bytes, host)?
    } else if let Some(len) = length {
        if len > max_bytes {
            return Err(HttpError::TooLarge {
                host: host.to_string(),
                limit: max_bytes,
            });
        }
        reader.exact(len as usize, max_bytes, host)?
    } else {
        reader.rest(max_bytes, host)?
    };
    Ok((status, headers, body))
}

/// A reader with a buffer, so the head of the answer and its body can share one stream.
struct Buffered<R> {
    inner: R,
    buf: Vec<u8>,
    start: usize,
    host: String,
}

impl<R: Read> Buffered<R> {
    fn new(inner: R, host: &str) -> Buffered<R> {
        Buffered {
            inner,
            buf: Vec::new(),
            start: 0,
            host: host.to_string(),
        }
    }

    /// Reads one line without its end (`\r\n`). Fails when the line grows past `limit`.
    fn line(&mut self, limit: usize) -> Result<Vec<u8>, HttpError> {
        loop {
            if let Some(at) = find_crlf(&self.buf[self.start..]) {
                let line = self.buf[self.start..self.start + at].to_vec();
                self.start += at + 2;
                return Ok(line);
            }
            if self.buf.len() - self.start > limit {
                return Err(HttpError::Protocol {
                    host: self.host.clone(),
                    detail: "a header line is too long".into(),
                });
            }
            self.fill()?;
        }
    }

    /// Reads exactly `len` bytes.
    fn exact(&mut self, len: usize, max_bytes: u64, host: &str) -> Result<Vec<u8>, HttpError> {
        if u64::try_from(len).unwrap_or(u64::MAX) > max_bytes {
            return Err(HttpError::TooLarge {
                host: host.to_string(),
                limit: max_bytes,
            });
        }
        while self.buf.len() - self.start < len {
            self.fill()?;
        }
        let out = self.buf[self.start..self.start + len].to_vec();
        self.start += len;
        Ok(out)
    }

    /// Reads what is left, up to `max_bytes`.
    fn rest(&mut self, max_bytes: u64, host: &str) -> Result<Vec<u8>, HttpError> {
        let mut out = self.buf[self.start..].to_vec();
        self.start = self.buf.len();
        let mut chunk = [0u8; 8192];
        loop {
            let read = self
                .inner
                .read(&mut chunk)
                .map_err(|e| HttpError::Connect {
                    host: host.to_string(),
                    source: e,
                })?;
            if read == 0 {
                break;
            }
            if out.len() as u64 + read as u64 > max_bytes {
                return Err(HttpError::TooLarge {
                    host: host.to_string(),
                    limit: max_bytes,
                });
            }
            out.extend_from_slice(&chunk[..read]);
        }
        Ok(out)
    }

    /// Reads a body sent in chunks.
    fn chunked(&mut self, max_bytes: u64, host: &str) -> Result<Vec<u8>, HttpError> {
        let mut out = Vec::new();
        loop {
            let line = self.line(1024)?;
            let text = String::from_utf8_lossy(&line);
            let size = usize::from_str_radix(text.split(';').next().unwrap_or("").trim(), 16)
                .map_err(|_| HttpError::Protocol {
                    host: host.to_string(),
                    detail: "a chunk size is not a number".into(),
                })?;
            if size == 0 {
                break;
            }
            if out.len() as u64 + size as u64 > max_bytes {
                return Err(HttpError::TooLarge {
                    host: host.to_string(),
                    limit: max_bytes,
                });
            }
            out.extend_from_slice(&self.exact(size, max_bytes, host)?);
            self.line(8)?;
        }
        Ok(out)
    }

    /// Reads more of the stream into the buffer.
    fn fill(&mut self) -> Result<(), HttpError> {
        let mut chunk = [0u8; 8192];
        let read = self
            .inner
            .read(&mut chunk)
            .map_err(|e| HttpError::Connect {
                host: self.host.clone(),
                source: e,
            })?;
        if read == 0 {
            return Err(HttpError::Protocol {
                host: self.host.clone(),
                detail: "the answer ended too early".into(),
            });
        }
        if self.start > 0 {
            self.buf.drain(..self.start);
            self.start = 0;
        }
        self.buf.extend_from_slice(&chunk[..read]);
        Ok(())
    }
}

fn find_crlf(bytes: &[u8]) -> Option<usize> {
    bytes.windows(2).position(|w| w == b"\r\n")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn urls_are_split_and_refused() {
        let u = parse_url("https://freesound.org/apiv2/search/?query=bell").unwrap();
        assert!(u.secure && u.host == "freesound.org" && u.port == 443);
        assert_eq!(u.path, "/apiv2/search/?query=bell");
        let u = parse_url("http://127.0.0.1:8080/x").unwrap();
        assert!(!u.secure && u.port == 8080);
        for bad in [
            "freesound.org/x",
            "ftp://freesound.org/x",
            "https://user@freesound.org/x",
            "https://free sound.org/x",
            "https://free..sound.org/x",
            "https://freesound.org/a b",
        ] {
            assert!(parse_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn query_values_are_escaped() {
        assert_eq!(encode_query_value("bell drum"), "bell%20drum");
        assert_eq!(encode_query_value("a&b=c"), "a%26b%3Dc");
        assert_eq!(encode_query_value("café"), "caf%C3%A9");
    }

    #[test]
    fn a_redirect_to_another_site_is_refused() {
        let from = parse_url("https://freesound.org/apiv2/x").unwrap();
        let other = parse_url("https://example.com/x").unwrap();
        assert!(from.same_site(&parse_url("https://cdn.freesound.org/x").unwrap()));
        assert!(!from.same_site(&other));
    }
}
