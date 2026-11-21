//! Minimal zero-dependency HTTP/1.1 request parser and response writer.

use std::collections::HashMap;
use std::io::{self, Read as _, Write as _};
use std::net::TcpStream;

use crate::json::{Json, stringify};

/// Parsed HTTP request.
#[derive(Debug, Clone)]
pub struct Request {
    /// Uppercase method.
    pub method: String,
    /// Path without query string.
    pub path: String,
    /// Lowercase header names.
    pub headers: HashMap<String, String>,
    /// Request body.
    pub body: Vec<u8>,
}

impl Request {
    /// Returns the body as UTF-8 text.
    #[must_use]
    pub fn body_text(&self) -> Option<&str> {
        std::str::from_utf8(&self.body).ok()
    }

    /// Parses the body as JSON.
    ///
    /// # Errors
    ///
    /// Returns a parse error for non-UTF-8 or malformed JSON.
    pub fn json(&self) -> Result<Json, String> {
        let text = self
            .body_text()
            .ok_or_else(|| "request body is not UTF-8".to_string())?;
        crate::json::parse(text)
    }
}

/// HTTP response.
#[derive(Debug, Clone)]
pub struct Response {
    /// Status code.
    pub status: u16,
    /// Content type.
    pub content_type: String,
    /// Raw body.
    pub body: Vec<u8>,
}

impl Response {
    /// Builds a JSON response.
    #[must_use]
    pub fn json(status: u16, value: &Json) -> Self {
        Self {
            status,
            content_type: "application/json; charset=utf-8".to_string(),
            body: stringify(value).into_bytes(),
        }
    }

    /// Builds an HTML response.
    #[must_use]
    pub fn html(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            content_type: "text/html; charset=utf-8".to_string(),
            body: body.into().into_bytes(),
        }
    }

    /// Builds a plain-text error response.
    #[must_use]
    pub fn text(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8".to_string(),
            body: body.into().into_bytes(),
        }
    }

    /// Serializes the response headers and body.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let reason = match self.status {
            200 => "OK",
            400 => "Bad Request",
            404 => "Not Found",
            405 => "Method Not Allowed",
            413 => "Payload Too Large",
            500 => "Internal Server Error",
            _ => "Response",
        };
        let headers = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
            self.status,
            reason,
            self.content_type,
            self.body.len()
        );
        let mut bytes = headers.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }
}

const MAX_HEADER: usize = 64 * 1024;
const MAX_BODY: usize = 8 * 1024 * 1024;

/// Reads exactly one HTTP request from a stream.
///
/// # Errors
///
/// Returns the underlying I/O error or a parser error.
pub fn read_request(stream: &mut TcpStream) -> io::Result<Option<Request>> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    let header_end;
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(None);
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_HEADER + MAX_BODY {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request too large",
            ));
        }
        if let Some(position) = find_header_end(&buffer) {
            header_end = position;
            break;
        }
        if buffer.len() > MAX_HEADER {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "headers too large",
            ));
        }
    }
    let header_text = std::str::from_utf8(&buffer[..header_end])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "headers are not UTF-8"))?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing method"))?
        .to_ascii_uppercase();
    let target = parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing path"))?;
    let path = target.split('?').next().unwrap_or(target).to_string();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > MAX_BODY {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "body too large"));
    }
    let body_start = header_end + 4;
    while buffer.len() < body_start + content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body = buffer[body_start..buffer.len().min(body_start + content_length)].to_vec();
    Ok(Some(Request {
        method,
        path,
        headers,
        body,
    }))
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

/// Writes a response and closes the stream.
///
/// # Errors
///
/// Returns the underlying I/O error.
pub fn write_response(stream: &mut TcpStream, response: &Response) -> io::Result<()> {
    stream.write_all(&response.to_bytes())?;
    stream.flush()
}
