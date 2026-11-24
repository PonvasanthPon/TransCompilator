//! Minimal Linux termios raw mode with RAII and panic-hook restoration.

use std::io::{self, Write as _};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

const TCGETS: u64 = 0x5401;
const TCSETS: u64 = 0x5402;
const TIOCGWINSZ: u64 = 0x5413;

/// Start of a synchronized-output frame.
pub const SYNC_BEGIN: &str = "\x1b[?2026h";
/// End of a synchronized-output frame.
pub const SYNC_END: &str = "\x1b[?2026l";

const ICANON: u32 = 0x0000_0002;
const ECHO: u32 = 0x0000_0008;
const ISIG: u32 = 0x0000_0001;
const IEXTEN: u32 = 0x0000_8000;
const IXON: u32 = 0x0000_0400;
const ICRNL: u32 = 0x0000_0100;
const VTIME: usize = 5;
const VMIN: usize = 6;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[allow(clippy::struct_field_names)] // ABI field names mirror Linux termios.
struct Termios {
    c_iflag: u32,
    c_oflag: u32,
    c_cflag: u32,
    c_lflag: u32,
    c_line: u8,
    c_cc: [u8; 32],
    c_ispeed: u32,
    c_ospeed: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
#[allow(clippy::struct_field_names)] // ABI field names mirror Linux winsize.
struct Winsize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

// SAFETY: this declaration matches the Linux x86_64 `ioctl` ABI. Every call
// below passes a request constant plus a pointer to a live `Termios` or
// `Winsize`, or omits the optional third argument for the same ABI.
unsafe extern "C" {
    fn ioctl(fd: i32, request: u64, ...) -> i32;
}

static ORIGINAL: Mutex<Option<Termios>> = Mutex::new(None);
static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

fn apply(termios: &Termios) -> io::Result<()> {
    // SAFETY: stdin is process-owned, `termios` is a live reference, and
    // TCSETS reads one `struct termios` through the supplied pointer.
    let result = unsafe { ioctl(0, TCSETS, std::ptr::from_ref(termios)) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn restore_from_static() {
    let Ok(guard) = ORIGINAL.lock() else {
        return;
    };
    let Some(termios) = *guard else {
        return;
    };
    let _ = apply(&termios);
    let mut stderr = io::stderr();
    let _ = stderr.write_all(b"\x1b[?1049l\x1b[?25h");
    let _ = stderr.flush();
}

fn install_panic_hook() {
    if HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_from_static();
        previous(info);
    }));
}

/// A terminal held in raw mode. Dropping it always restores the original
/// termios state, leaves the alternate screen, and shows the cursor.
#[derive(Debug)]
pub struct RawTerminal {
    original: Termios,
    restored: bool,
}

impl RawTerminal {
    /// Enters raw mode on stdin.
    ///
    /// # Errors
    ///
    /// Returns the OS error when stdin is not a terminal or termios cannot be
    /// queried or changed.
    pub fn enter() -> io::Result<Self> {
        let mut original = Termios {
            c_iflag: 0,
            c_oflag: 0,
            c_cflag: 0,
            c_lflag: 0,
            c_line: 0,
            c_cc: [0; 32],
            c_ispeed: 0,
            c_ospeed: 0,
        };
        // SAFETY: `original` is a live, correctly sized `Termios`; TCGETS
        // writes the current terminal state into that pointer.
        let result = unsafe { ioctl(0, TCGETS, &raw mut original) };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        if let Ok(mut guard) = ORIGINAL.lock() {
            *guard = Some(original);
        }
        install_panic_hook();
        let mut raw = original;
        raw.c_lflag &= !(ICANON | ECHO | ISIG | IEXTEN);
        raw.c_iflag &= !(IXON | ICRNL);
        raw.c_cc[VMIN] = 1;
        raw.c_cc[VTIME] = 0;
        if let Err(error) = apply(&raw) {
            if let Ok(mut guard) = ORIGINAL.lock() {
                *guard = None;
            }
            return Err(error);
        }
        let terminal = Self {
            original,
            restored: false,
        };
        let mut stdout = io::stdout();
        if let Err(error) = stdout
            .write_all(b"\x1b[?1049h\x1b[2J\x1b[H\x1b[?25l")
            .and_then(|()| stdout.flush())
        {
            drop(terminal);
            return Err(error);
        }
        Ok(terminal)
    }

    /// Returns the terminal size as `(columns, rows)`, falling back to
    /// `(80, 24)` when the ioctl is unavailable.
    #[must_use]
    pub fn size(&self) -> (usize, usize) {
        terminal_size()
    }

    fn restore(&mut self) {
        if self.restored {
            return;
        }
        self.restored = true;
        let _ = apply(&self.original);
        if let Ok(mut guard) = ORIGINAL.lock() {
            *guard = None;
        }
        let mut stdout = io::stdout();
        let _ = stdout.write_all(b"\x1b[?1049l\x1b[?25h");
        let _ = stdout.flush();
    }
}

impl Drop for RawTerminal {
    fn drop(&mut self) {
        self.restore();
    }
}

/// Returns the current terminal size without requiring a raw terminal.
#[must_use]
pub fn terminal_size() -> (usize, usize) {
    let mut size = Winsize::default();
    // SAFETY: `size` is a live, correctly sized `Winsize`; TIOCGWINSZ writes
    // the terminal geometry into that pointer.
    let result = unsafe { ioctl(0, TIOCGWINSZ, &raw mut size) };
    if result == 0 && size.ws_col > 0 && size.ws_row > 0 {
        (usize::from(size.ws_col), usize::from(size.ws_row))
    } else {
        (80, 24)
    }
}
