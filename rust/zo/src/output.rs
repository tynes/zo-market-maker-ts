use std::io::Write;

use crate::error::FeedError;
use crate::types::CombinedStreamMsg;

/// Parse a raw JSON text message and write formatted output to the writer.
///
/// Returns `Ok(())` on success, or a `FeedError` if parsing fails.
pub fn handle_message<W: Write>(
    text: &str,
    json_mode: bool,
    buf: &mut String,
    writer: &mut W,
) -> Result<(), FeedError> {
    let msg: CombinedStreamMsg = serde_json::from_str(text)?;
    let d = &msg.data;

    let bid: f64 = d.b.parse()?;
    let ask: f64 = d.a.parse()?;
    let mid = (bid + ask) * 0.5;

    buf.clear();

    if json_mode {
        // Manual JSON construction to avoid serde_json::to_string allocation overhead.
        buf.push_str("{\"symbol\":\"");
        buf.push_str(&d.s);
        buf.push_str("\",\"bid\":");
        format_f64(buf, bid);
        buf.push_str(",\"ask\":");
        format_f64(buf, ask);
        buf.push_str(",\"mid\":");
        format_f64(buf, mid);
        buf.push_str(",\"bid_qty\":\"");
        buf.push_str(&d.bid_qty);
        buf.push_str("\",\"ask_qty\":\"");
        buf.push_str(&d.ask_qty);
        buf.push_str("\",\"event_time\":");
        itoa_u64(buf, d.event_time);
        buf.push('}');
    } else {
        // TSV: symbol \t bid \t ask \t mid \t bid_qty \t ask_qty \t event_time
        buf.push_str(&d.s);
        buf.push('\t');
        format_f64(buf, bid);
        buf.push('\t');
        format_f64(buf, ask);
        buf.push('\t');
        format_f64(buf, mid);
        buf.push('\t');
        buf.push_str(&d.bid_qty);
        buf.push('\t');
        buf.push_str(&d.ask_qty);
        buf.push('\t');
        itoa_u64(buf, d.event_time);
    }

    buf.push('\n');
    writer.write_all(buf.as_bytes())?;
    writer.flush()?;

    Ok(())
}

/// Fast f64 formatting via `ryu`.
fn format_f64(buf: &mut String, val: f64) {
    let mut b = ryu::Buffer::new();
    buf.push_str(b.format(val));
}

/// Fast u64 formatting (avoids `format!` allocation).
fn itoa_u64(buf: &mut String, val: u64) {
    let mut b = itoa_buf::<20>();
    let s = write_u64(&mut b, val);
    buf.push_str(s);
}

fn itoa_buf<const N: usize>() -> [u8; N] {
    [0u8; N]
}

fn write_u64(buf: &mut [u8; 20], mut val: u64) -> &str {
    if val == 0 {
        return "0";
    }
    let mut i = buf.len();
    while val > 0 {
        i -= 1;
        buf[i] = b'0' + (val % 10) as u8;
        val /= 10;
    }
    // SAFETY: digits 0-9 are valid UTF-8
    unsafe { std::str::from_utf8_unchecked(&buf[i..]) }
}

impl From<std::io::Error> for FeedError {
    fn from(e: std::io::Error) -> Self {
        // Map IO errors (stdout broken pipe, etc.) to ConnectionClosed
        tracing::debug!("IO error: {}", e);
        FeedError::ConnectionClosed
    }
}
