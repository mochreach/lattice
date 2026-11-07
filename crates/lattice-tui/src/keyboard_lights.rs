//! A qs-rgbd client with no dependencies beyond std.
//!
//! Drop this in your tracker as `keyboard_lights.rs`. It speaks the protocol
//! in PROTOCOL.md: connect once, keep the connection for as long as you want
//! your lights on the keyboard, and let the daemon clean up if you crash.
//!
//! ```no_run
//! let mut kb = Keyboard::connect("tracker")?;
//! kb.set(20, &[("f1", "#ff0000"), ("f2", "#00ff00")])?;
//! kb.patch(&[("f1", "#330000")])?;
//! ```
//!
//! Every method degrades to a no-op when the daemon is not running, so a
//! machine without the hardware needs no feature flag or cfg.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

pub struct Keyboard {
    layer: String,
    conn: Option<Conn>,
}

struct Conn {
    writer: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Keyboard {
    /// Connect to the daemon. `layer` is your name for the layer you own.
    /// A missing daemon is not an error: the returned Keyboard is inert.
    pub fn connect(layer: &str) -> Self {
        let conn = socket_path()
            .and_then(|p| UnixStream::connect(p).ok())
            .and_then(|s| {
                let reader = BufReader::new(s.try_clone().ok()?);
                Some(Conn { writer: s, reader })
            });
        Keyboard {
            layer: layer.to_string(),
            conn,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.conn.is_some()
    }

    /// Replace the layer with these keys. Keys you leave out fall through to
    /// whatever is underneath, normally the battery and notification HUD.
    pub fn set(&mut self, priority: u16, keys: &[(&str, &str)]) -> std::io::Result<()> {
        let req = format!(
            r#"{{"cmd":"set","layer":"{}","priority":{},"keys":{{{}}}}}"#,
            esc(&self.layer),
            priority,
            key_map(keys)
        );
        self.send(&req)
    }

    /// Replace the layer and cover the whole keyboard with `base` first, so
    /// nothing below shows through except where you pass a key of "null".
    pub fn set_opaque(
        &mut self,
        priority: u16,
        base: &str,
        keys: &[(&str, &str)],
    ) -> std::io::Result<()> {
        let req = format!(
            r#"{{"cmd":"set","layer":"{}","priority":{},"base":"{}","keys":{{{}}}}}"#,
            esc(&self.layer),
            priority,
            esc(base),
            key_map(keys)
        );
        self.send(&req)
    }

    /// Change just these keys, leaving the rest of the layer alone.
    pub fn patch(&mut self, keys: &[(&str, &str)]) -> std::io::Result<()> {
        let req = format!(
            r#"{{"cmd":"patch","layer":"{}","keys":{{{}}}}}"#,
            esc(&self.layer),
            key_map(keys)
        );
        self.send(&req)
    }

    /// Make one key breathe, pulse or blink. `effect` is one of
    /// "solid", "breathe", "pulse", "blink".
    pub fn animate(
        &mut self,
        key: &str,
        color: &str,
        effect: &str,
        period: f32,
    ) -> std::io::Result<()> {
        let req = format!(
            r#"{{"cmd":"patch","layer":"{}","keys":{{"{}":{{"color":"{}","effect":"{}","period":{}}}}}}}"#,
            esc(&self.layer),
            esc(key),
            esc(color),
            esc(effect),
            period
        );
        self.send(&req)
    }

    /// Drop the layer now. Not required: closing the connection, or exiting
    /// for any reason at all, does the same thing.
    pub fn clear(&mut self) -> std::io::Result<()> {
        let req = format!(r#"{{"cmd":"clear","layer":"{}"}}"#, esc(&self.layer));
        self.send(&req)
    }

    fn send(&mut self, req: &str) -> std::io::Result<()> {
        let Some(conn) = self.conn.as_mut() else {
            return Ok(());
        };
        if writeln!(conn.writer, "{req}").is_err() {
            self.conn = None;
            return Ok(());
        }
        let mut reply = String::new();
        if conn.reader.read_line(&mut reply).is_err() || reply.is_empty() {
            // The daemon went away; stop trying rather than failing the
            // caller, whose real job is not lighting up a keyboard.
            self.conn = None;
        }
        Ok(())
    }
}

fn socket_path() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")?;
    Some(PathBuf::from(dir).join("qs-rgb.sock"))
}

fn key_map(keys: &[(&str, &str)]) -> String {
    keys.iter()
        .map(|(k, v)| {
            if *v == "null" {
                format!(r#""{}":null"#, esc(k))
            } else {
                format!(r#""{}":"{}""#, esc(k), esc(v))
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Key names and colours are short ASCII tokens, but escape anyway so a
/// stray quote cannot break the frame.
fn esc(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            c if c.is_control() => vec![' '],
            c => vec![c],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_key_map() {
        assert_eq!(key_map(&[("w", "#fff")]), "\"w\":\"#fff\"");
        assert_eq!(key_map(&[("esc", "null")]), "\"esc\":null");
        assert_eq!(
            key_map(&[("w", "#fff"), ("a", "red")]),
            "\"w\":\"#fff\",\"a\":\"red\""
        );
    }

    #[test]
    fn escapes_quotes() {
        assert_eq!(esc(r#"a"b"#), r#"a\"b"#);
    }

    #[test]
    fn no_daemon_is_not_an_error() {
        let mut kb = Keyboard {
            layer: "t".into(),
            conn: None,
        };
        assert!(kb.set(20, &[("w", "#fff")]).is_ok());
        assert!(!kb.is_connected());
    }
}
