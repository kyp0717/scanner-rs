/// IB TWS API message encoding/decoding.
///
/// The TWS API uses a binary protocol over TCP where messages are length-prefixed:
///   [4-byte big-endian length][payload]
/// The payload is a sequence of null-terminated fields.

use std::io::{self, Read, Write};

/// Write a length-prefixed message to a writer.
pub fn write_message(writer: &mut impl Write, fields: &[&str]) -> io::Result<()> {
    let mut payload = Vec::new();
    for field in fields {
        payload.extend_from_slice(field.as_bytes());
        payload.push(0);
    }
    let len = payload.len() as u32;
    writer.write_all(&len.to_be_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()
}

/// Read a length-prefixed message from a reader.
/// Returns the parsed fields as strings.
pub fn read_message(reader: &mut impl Read) -> io::Result<Vec<String>> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len == 0 {
        return Ok(vec![]);
    }

    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;

    let mut fields = Vec::new();
    let mut start = 0;
    for (i, &b) in payload.iter().enumerate() {
        if b == 0 {
            let field = String::from_utf8_lossy(&payload[start..i]).to_string();
            fields.push(field);
            start = i + 1;
        }
    }

    Ok(fields)
}

/// IB API message types (outgoing).
pub mod out_msg {
    pub const REQ_SCANNER_SUBSCRIPTION: &str = "22";
    pub const CANCEL_SCANNER_SUBSCRIPTION: &str = "23";
    pub const REQ_SCANNER_PARAMETERS: &str = "24";
    pub const REQ_MKT_DATA: &str = "1";
    pub const CANCEL_MKT_DATA: &str = "2";
    pub const REQ_MKT_DATA_TYPE: &str = "59";
}

/// IB API message types (incoming).
pub mod in_msg {
    pub const TICK_PRICE: &str = "1";
    pub const TICK_SIZE: &str = "2";
    pub const ERR_MSG: &str = "4";
    pub const NEXT_VALID_ID: &str = "9";
    pub const SCANNER_DATA: &str = "20";
    pub const SCANNER_PARAMETERS: &str = "19";
}

/// Tick type IDs.
pub mod tick_type {
    pub const BID: i32 = 1;
    pub const ASK: i32 = 2;
    pub const LAST: i32 = 4;
    pub const VOLUME: i32 = 8;
    pub const CLOSE: i32 = 9;
    pub const DELAYED_BID: i32 = 66;
    pub const DELAYED_ASK: i32 = 67;
    pub const DELAYED_LAST: i32 = 68;
    pub const DELAYED_CLOSE: i32 = 75;
}

/// Non-fatal error codes that are informational.
pub const NONFATAL_ERRORS: &[i32] = &[
    162,   // Scanner cancelled
    354,   // No subscription
    502,   // Cannot connect
    2104, 2106, 2158, 2119,  // Market data farm messages
    10167, 10168,             // Delayed market data
    10197,                    // No data during competing session
];

/// Build the initial handshake message for TWS API v100+.
pub fn build_handshake() -> Vec<u8> {
    // API prefix "API\0" followed by version range "v100..176"
    let version = "v100..176";
    let mut msg = Vec::new();
    msg.extend_from_slice(b"API\0");
    let version_bytes = version.as_bytes();
    let len = version_bytes.len() as u32;
    msg.extend_from_slice(&len.to_be_bytes());
    msg.extend_from_slice(version_bytes);
    msg
}

/// Build the start API message (sent after handshake to specify client ID).
pub fn build_start_api(client_id: i32) -> Vec<u8> {
    let mut payload = Vec::new();
    // Message type: 71 (START_API)
    payload.extend_from_slice(b"71");
    payload.push(0);
    // Version: 2
    payload.extend_from_slice(b"2");
    payload.push(0);
    // Client ID
    payload.extend_from_slice(client_id.to_string().as_bytes());
    payload.push(0);
    // Optional capabilities (empty)
    payload.push(0);

    let mut msg = Vec::new();
    let len = payload.len() as u32;
    msg.extend_from_slice(&len.to_be_bytes());
    msg.extend_from_slice(&payload);
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_and_read_message() {
        let mut buf = Vec::new();
        write_message(&mut buf, &["hello", "world"]).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let fields = read_message(&mut cursor).unwrap();
        assert_eq!(fields, vec!["hello", "world"]);
    }

    #[test]
    fn test_write_empty_message() {
        let mut buf = Vec::new();
        write_message(&mut buf, &[]).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let fields = read_message(&mut cursor).unwrap();
        assert!(fields.is_empty());
    }

    #[test]
    fn test_build_handshake() {
        let msg = build_handshake();
        assert!(msg.starts_with(b"API\0"));
    }

    #[test]
    fn test_build_start_api() {
        let msg = build_start_api(1);
        // Should be a length-prefixed message containing "71"
        assert!(!msg.is_empty());
        // First 4 bytes are the length
        let len = u32::from_be_bytes([msg[0], msg[1], msg[2], msg[3]]) as usize;
        assert_eq!(msg.len(), 4 + len);
    }

    #[test]
    fn test_nonfatal_errors_contains_known() {
        assert!(NONFATAL_ERRORS.contains(&162));
        assert!(NONFATAL_ERRORS.contains(&502));
        assert!(!NONFATAL_ERRORS.contains(&999));
    }
}
