//! LSP `Content-Length` framing and JSON-RPC 2.0 helpers.

use std::io::{self, BufRead, Write};

use crate::json::{Json, parse, stringify};

/// Reads one framed message body.
///
/// # Errors
///
/// Returns I/O errors when the stream cannot be read.
pub fn read_frame<R: BufRead>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut content_length = None;
    let mut saw_header = false;
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            return if saw_header {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "EOF inside LSP headers",
                ))
            } else {
                Ok(None)
            };
        }
        saw_header = true;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse::<usize>().ok();
            }
        }
    }
    let Some(length) = content_length else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "missing Content-Length header",
        ));
    };
    let mut body = vec![0_u8; length];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}

/// Writes one framed JSON message.
///
/// # Errors
///
/// Returns I/O errors when the stream cannot be written.
pub fn write_frame<W: Write>(writer: &mut W, body: &str) -> io::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    writer.flush()
}

/// Writes one framed JSON value.
///
/// # Errors
///
/// Returns I/O errors when the stream cannot be written.
pub fn write_json<W: Write>(writer: &mut W, value: &Json) -> io::Result<()> {
    write_frame(writer, &stringify(value))
}

/// Reads and parses one JSON message, skipping malformed payloads.
///
/// # Errors
///
/// Returns I/O errors when the stream cannot be read.
pub fn read_json<R: BufRead>(reader: &mut R) -> io::Result<Option<Json>> {
    loop {
        let Some(body) = read_frame(reader)? else {
            return Ok(None);
        };
        let Ok(text) = String::from_utf8(body) else {
            continue;
        };
        if let Ok(value) = parse(&text) {
            return Ok(Some(value));
        }
    }
}

/// Builds a successful JSON-RPC response.
#[must_use]
pub fn response(id: &Json, result: Json) -> Json {
    Json::object([
        ("jsonrpc", Json::string("2.0")),
        ("id", id.clone()),
        ("result", result),
    ])
}

/// Builds a JSON-RPC error response.
#[must_use]
pub fn error_response(id: &Json, code: i64, message: &str) -> Json {
    Json::object([
        ("jsonrpc", Json::string("2.0")),
        ("id", id.clone()),
        (
            "error",
            Json::object([
                ("code", Json::Number(code)),
                ("message", Json::string(message)),
            ]),
        ),
    ])
}

/// Builds a server-to-client notification.
#[must_use]
pub fn notification(method: &str, params: Json) -> Json {
    Json::object([
        ("jsonrpc", Json::string("2.0")),
        ("method", Json::string(method)),
        ("params", params),
    ])
}
