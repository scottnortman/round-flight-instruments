//! Presentation only: input still passes unchanged to the strict command framer.
use std::fmt::Write;
use text_cli::{Response, MAX_LINE};

pub(super) struct Echo {
    pub enabled: bool,
    previous_cr: bool,
    count: usize,
}
impl Echo {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            previous_cr: false,
            count: 0,
        }
    }
    pub fn push(&mut self, byte: u8) -> Option<Response> {
        let crlf = self.previous_cr && byte == b'\n';
        self.previous_cr = byte == b'\r';
        let mut chunk = Response::new();
        if byte == b'\r' || byte == b'\n' {
            self.count = 0;
            if crlf || !self.enabled {
                return None;
            }
            // ESP-IDF VFS maps LF to CRLF; do not add a second CR here.
            chunk.write_char('\n').ok()?;
        } else {
            self.count = self.count.saturating_add(1);
            if !self.enabled || self.count > MAX_LINE + 1 {
                return None;
            }
            if self.count == MAX_LINE + 1 {
                chunk.write_str("[...]").ok()?;
            } else if byte.is_ascii_graphic() || byte == b' ' || byte == b'\t' {
                chunk.write_char(byte as char).ok()?;
            } else {
                // No terminal escape execution or misleading backspace editing.
                write!(chunk, "\\x{byte:02X}").ok()?;
            }
        }
        Some(chunk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rendered(echo: &mut Echo, input: &[u8]) -> String {
        input
            .iter()
            .filter_map(|b| echo.push(*b))
            .map(|r| r.as_str().to_owned())
            .collect()
    }
    #[test]
    fn echoes_before_enter_and_normalizes_all_terminators() {
        let mut echo = Echo::new(true);
        assert_eq!(rendered(&mut echo, b"s"), "s");
        assert_eq!(
            rendered(&mut echo, b"tatus\r\necho status\rhelp\n"),
            "tatus\necho status\nhelp\n"
        );
        echo.enabled = false;
        assert_eq!(rendered(&mut echo, b"status\r\n"), "");
    }
    #[test]
    fn escapes_controls_and_bounds_invalid_line_output() {
        let mut echo = Echo::new(true);
        assert_eq!(
            rendered(&mut echo, b"\x1b[2J\x08\x7f\xff\n"),
            "\\x1B[2J\\x08\\x7F\\xFF\n"
        );
        let long = rendered(&mut echo, &[0xff; 4096]);
        assert_eq!(long.len(), MAX_LINE * 4 + 5);
        assert!(long.ends_with("[...]"));
        assert_eq!(rendered(&mut echo, b"\nhelp\n"), "\nhelp\n");
    }
}
