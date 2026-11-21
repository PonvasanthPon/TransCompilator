//! Zero-dependency ANSI/VT100 byte-stream key decoder.

/// A decoded terminal key event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyEvent {
    /// Printable character, including multi-byte UTF-8.
    Char(char),
    /// Enter / carriage return.
    Enter,
    /// Tab.
    Tab,
    /// Backspace / DEL.
    Backspace,
    /// Delete.
    Delete,
    /// Escape key.
    Esc,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Function key `F1..=F12`.
    F(u8),
    /// Control key with the lowercase letter.
    Ctrl(char),
    /// Unrecognized sequence.
    Unknown,
}

fn utf8_width(byte: u8) -> usize {
    if byte < 0x80 {
        1
    } else if byte & 0xE0 == 0xC0 {
        2
    } else if byte & 0xF0 == 0xE0 {
        3
    } else if byte & 0xF8 == 0xF0 {
        4
    } else {
        0
    }
}

fn csi_key(sequence: &[u8]) -> Option<KeyEvent> {
    // `sequence` starts after ESC [.
    if sequence.is_empty() {
        return Some(KeyEvent::Unknown);
    }
    match sequence {
        b"A" => Some(KeyEvent::Up),
        b"B" => Some(KeyEvent::Down),
        b"C" => Some(KeyEvent::Right),
        b"D" => Some(KeyEvent::Left),
        b"H" => Some(KeyEvent::Home),
        b"F" => Some(KeyEvent::End),
        b"1~" => Some(KeyEvent::Home),
        b"2~" => Some(KeyEvent::Unknown),
        b"3~" => Some(KeyEvent::Delete),
        b"4~" => Some(KeyEvent::End),
        b"5~" => Some(KeyEvent::PageUp),
        b"6~" => Some(KeyEvent::PageDown),
        b"11~" => Some(KeyEvent::F(1)),
        b"12~" => Some(KeyEvent::F(2)),
        b"13~" => Some(KeyEvent::F(3)),
        b"14~" => Some(KeyEvent::F(4)),
        b"15~" => Some(KeyEvent::F(5)),
        b"17~" => Some(KeyEvent::F(6)),
        b"18~" => Some(KeyEvent::F(7)),
        b"19~" => Some(KeyEvent::F(8)),
        b"20~" => Some(KeyEvent::F(9)),
        b"21~" => Some(KeyEvent::F(10)),
        b"23~" => Some(KeyEvent::F(11)),
        b"24~" => Some(KeyEvent::F(12)),
        _ => Some(KeyEvent::Unknown),
    }
}

fn ss3_key(byte: u8) -> KeyEvent {
    match byte {
        b'P' => KeyEvent::F(1),
        b'Q' => KeyEvent::F(2),
        b'R' => KeyEvent::F(3),
        b'S' => KeyEvent::F(4),
        b'A' => KeyEvent::Up,
        b'B' => KeyEvent::Down,
        b'C' => KeyEvent::Right,
        b'D' => KeyEvent::Left,
        b'H' => KeyEvent::Home,
        b'F' => KeyEvent::End,
        _ => KeyEvent::Unknown,
    }
}

/// Decodes a byte slice into key events. Incomplete trailing UTF-8 produces
/// one `Unknown` event; callers normally feed complete stdin reads.
#[must_use]
pub fn decode_keys(bytes: &[u8]) -> Vec<KeyEvent> {
    let mut events = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == 0x1b {
            if index + 1 >= bytes.len() {
                events.push(KeyEvent::Esc);
                index += 1;
                continue;
            }
            let next = bytes[index + 1];
            if next == b'[' {
                let mut end = index + 2;
                while end < bytes.len() && !(0x40..=0x7e).contains(&bytes[end]) {
                    end += 1;
                }
                if end < bytes.len() {
                    let sequence = &bytes[index + 2..=end];
                    events.push(csi_key(sequence).unwrap_or(KeyEvent::Unknown));
                    index = end + 1;
                    continue;
                }
                events.push(KeyEvent::Unknown);
                break;
            }
            if next == b'O' {
                if index + 2 < bytes.len() {
                    events.push(ss3_key(bytes[index + 2]));
                    index += 3;
                    continue;
                }
                events.push(KeyEvent::Unknown);
                break;
            }
            events.push(KeyEvent::Esc);
            index += 1;
            continue;
        }
        match byte {
            b'\r' | b'\n' => {
                events.push(KeyEvent::Enter);
                index += 1;
            }
            b'\t' => {
                events.push(KeyEvent::Tab);
                index += 1;
            }
            0x7f | 0x08 => {
                events.push(KeyEvent::Backspace);
                index += 1;
            }
            0x01..=0x1a => {
                let letter = char::from(b'a' + byte - 1);
                events.push(KeyEvent::Ctrl(letter));
                index += 1;
            }
            0x20..=0x7e => {
                events.push(KeyEvent::Char(char::from(byte)));
                index += 1;
            }
            0x80..=0xff => {
                let width = utf8_width(byte);
                if width == 0 || index + width > bytes.len() {
                    events.push(KeyEvent::Unknown);
                    index += 1;
                    continue;
                }
                if let Ok(text) = std::str::from_utf8(&bytes[index..index + width]) {
                    if let Some(character) = text.chars().next() {
                        events.push(KeyEvent::Char(character));
                    } else {
                        events.push(KeyEvent::Unknown);
                    }
                    index += width;
                } else {
                    events.push(KeyEvent::Unknown);
                    index += 1;
                }
            }
            _ => {
                events.push(KeyEvent::Unknown);
                index += 1;
            }
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::{KeyEvent, decode_keys};

    #[test]
    fn decoder_handles_plain_ascii() {
        assert_eq!(
            decode_keys(b"a\r\t\x7f"),
            vec![
                KeyEvent::Char('a'),
                KeyEvent::Enter,
                KeyEvent::Tab,
                KeyEvent::Backspace
            ]
        );
    }
}
