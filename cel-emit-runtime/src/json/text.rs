//! JSON object text written as an expression computes it: each member written in place as its
//! value computes, and one that fails, or a nested object left with no member, cut back out. No
//! value forms for the object, and nothing writes twice.

use super::write;
use crate::dynamic::Dyn;
use crate::Result;

/// Compact JSON text being written.
#[derive(Default)]
pub struct Text(Vec<u8>);

impl Text {
    /// Text with room for `n` bytes before it grows.
    pub fn with_capacity(n: usize) -> Text {
        Text(Vec::with_capacity(n))
    }

    /// Where the text ends: what [`Text::cut`] goes back to.
    pub fn mark(&self) -> usize {
        self.0.len()
    }

    /// Back to `mark`, leaving out what was written past it.
    pub fn cut(&mut self, mark: usize) {
        self.0.truncate(mark);
    }

    /// Whether nothing past an object opened at `mark` is written.
    pub fn bare(&self, mark: usize) -> bool {
        self.0.len() == mark.saturating_add(1)
    }

    /// Starts an array.
    pub fn open_list(&mut self) {
        self.0.push(b'[');
    }

    /// Ends an array.
    pub fn close_list(&mut self) {
        self.0.push(b']');
    }

    /// Starts an array's next item.
    pub fn item(&mut self) {
        if self.0.last() != Some(&b'[') {
            self.0.push(b',');
        }
    }

    /// Starts an object.
    pub fn open(&mut self) {
        self.0.push(b'{');
    }

    /// Ends an object.
    pub fn close(&mut self) {
        self.0.push(b'}');
    }

    /// Ends an object, or cuts back to `mark` where it has no member; whether it is kept.
    pub fn close_or_cut(&mut self, mark: usize) -> bool {
        if self.0.last() == Some(&b'{') {
            self.cut(mark);
            return false;
        }
        self.close();
        true
    }

    /// Writes a member's key, `quoted` as JSON with its colon, up to where its value goes; the
    /// mark before it.
    pub fn key(&mut self, quoted: &str) -> usize {
        let mark = self.mark();
        if self.0.last() != Some(&b'{') {
            self.0.push(b',');
        }
        self.0.extend_from_slice(quoted.as_bytes());
        mark
    }

    /// Writes `x`, or `Error::Range` where it holds a NaN or an infinity.
    pub fn value(&mut self, x: Dyn) -> Result<()> {
        write::value(&mut self.0, &x)
    }

    /// Writes the string `x`.
    pub fn str(&mut self, x: &str) -> Result<()> {
        write::string(&mut self.0, x)
    }

    /// Writes the int `x`.
    pub fn int(&mut self, x: i64) -> Result<()> {
        write::value(&mut self.0, &Dyn::Int(x))
    }

    /// Writes the uint `x`.
    pub fn uint(&mut self, x: u64) -> Result<()> {
        write::value(&mut self.0, &Dyn::UInt(x))
    }

    /// Writes the double `x`, or `Error::Range` where it is NaN or infinite.
    pub fn double(&mut self, x: f64) -> Result<()> {
        write::value(&mut self.0, &Dyn::Double(x))
    }

    /// Writes the bool `x`.
    pub fn bool(&mut self, x: bool) -> Result<()> {
        write::value(&mut self.0, &Dyn::Bool(x))
    }

    /// Writes the entries of `text`, JSON object text, each value as the text has it: a key of
    /// `own` the object wrote is left out, and text that is no object adds nothing.
    pub fn rest(&mut self, text: String, own: &[(&str, bool)]) -> Result<()> {
        let Some(raw) = super::shaped::object(text.as_bytes()) else {
            return Ok(());
        };
        for (k, v) in raw.iter() {
            if own.iter().any(|(o, written)| *written && *o == k) {
                continue;
            }
            if self.0.last() != Some(&b'{') {
                self.0.push(b',');
            }
            write::string(&mut self.0, k)?;
            self.0.push(b':');
            self.0.extend_from_slice(v.as_bytes());
        }
        Ok(())
    }

    /// The text written.
    pub fn done(self) -> String {
        String::from_utf8(self.0).unwrap_or_default()
    }
}
