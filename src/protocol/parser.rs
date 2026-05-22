use crate::protocol::{RespCommand, RespValue};
use bytes::{Buf, Bytes, BytesMut};
use std::io::Cursor;
use thiserror::Error;

/// RESP parser errors
#[derive(Debug, Error, PartialEq)]
pub enum RespError {
    #[error("incomplete data - need more bytes")]
    Incomplete,

    #[error("invalid protocol format: {0}")]
    InvalidFormat(String),

    #[error("invalid integer: {0}")]
    InvalidInteger(String),

    #[error("unsupported command: {0}")]
    UnsupportedCommand(String),
}

/// Streaming RESP parser that handles partial reads
///
/// ## Design:
/// - Buffers incoming bytes until a complete message is available
/// - Returns `Incomplete` when more data is needed
/// - Maintains parse state across multiple `feed()` calls
///
/// ## Usage:
/// ```rust,no_run
/// let mut parser = RespParser::new();
/// parser.feed(b"*2\r\n$3\r\n");  // Partial data
/// assert!(parser.parse_value().is_err()); // Incomplete
///
/// parser.feed(b"GET\r\n$3\r\nkey\r\n"); // Rest of data
/// let value = parser.parse_value().unwrap(); // Complete!
/// ```
pub struct RespParser {
    /// Buffer holding unparsed bytes
    buffer: BytesMut,
}

impl RespParser {
    /// Create a new parser with empty buffer
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::new(),
        }
    }

    /// Feed more bytes into the parser
    pub fn feed(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Try to parse the next RESP value from the buffer
    ///
    /// Returns:
    /// - `Ok(value)` if a complete value was parsed (buffer is advanced)
    /// - `Err(Incomplete)` if more data is needed (buffer unchanged)
    /// - `Err(...)` for protocol errors
    pub fn parse_value(&mut self) -> Result<RespValue, RespError> {
        let mut cursor = Cursor::new(&self.buffer[..]);

        // Try to parse
        match parse_value_internal(&mut cursor) {
            Ok(value) => {
                // Success - advance buffer past parsed data
                let parsed_len = cursor.position() as usize;
                self.buffer.advance(parsed_len);
                Ok(value)
            }
            Err(RespError::Incomplete) => {
                // Need more data - don't modify buffer
                Err(RespError::Incomplete)
            }
            Err(e) => Err(e),
        }
    }

    /// Parse a command from RESP array
    ///
    /// Converts a RespValue::Array into a RespCommand enum
    pub fn parse_command(&mut self) -> Result<RespCommand, RespError> {
        let value = self.parse_value()?;
        value_to_command(value)
    }
}

impl Default for RespParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal recursive parser
///
/// ## Why Cursor?
/// - Tracks position without consuming the underlying buffer
/// - If parsing fails, we haven't modified the original buffer
/// - Can calculate how many bytes were successfully parsed
fn parse_value_internal(cursor: &mut Cursor<&[u8]>) -> Result<RespValue, RespError> {
    if !cursor.has_remaining() {
        return Err(RespError::Incomplete);
    }

    let type_byte = cursor.get_u8();

    match type_byte {
        b'+' => parse_simple_string(cursor),
        b'-' => parse_error(cursor),
        b':' => parse_integer(cursor),
        b'$' => parse_bulk_string(cursor),
        b'*' => parse_array(cursor),
        other => Err(RespError::InvalidFormat(format!(
            "unknown type byte: {}",
            other
        ))),
    }
}

/// Parse a simple string: +OK\r\n
fn parse_simple_string(cursor: &mut Cursor<&[u8]>) -> Result<RespValue, RespError> {
    let line = read_line(cursor)?;
    Ok(RespValue::SimpleString(line))
}

/// Parse an error: -ERR message\r\n
fn parse_error(cursor: &mut Cursor<&[u8]>) -> Result<RespValue, RespError> {
    let line = read_line(cursor)?;
    Ok(RespValue::Error(line))
}

/// Parse an integer: :42\r\n
fn parse_integer(cursor: &mut Cursor<&[u8]>) -> Result<RespValue, RespError> {
    let line = read_line(cursor)?;
    let num = line
        .parse::<i64>()
        .map_err(|_| RespError::InvalidInteger(line.clone()))?;
    Ok(RespValue::Integer(num))
}

/// Parse a bulk string: $6\r\nfoobar\r\n or $-1\r\n (null)
///
/// ## Format:
/// ```text
/// $<length>\r\n
/// <data>\r\n
/// ```
///
/// Special case: $-1\r\n means null
fn parse_bulk_string(cursor: &mut Cursor<&[u8]>) -> Result<RespValue, RespError> {
    let len_str = read_line(cursor)?;
    let len = len_str
        .parse::<i64>()
        .map_err(|_| RespError::InvalidInteger(len_str.clone()))?;

    // Handle null bulk string
    if len == -1 {
        return Ok(RespValue::BulkString(None));
    }

    if len < 0 {
        return Err(RespError::InvalidFormat(format!(
            "invalid bulk string length: {}",
            len
        )));
    }

    let len = len as usize;

    // Check if we have enough data
    if cursor.remaining() < len + 2 {
        return Err(RespError::Incomplete);
    }

    // Read the data
    let start = cursor.position() as usize;
    let data = cursor.get_ref()[start..start + len].to_vec();
    cursor.advance(len);

    // Verify \r\n terminator
    if cursor.remaining() < 2 {
        return Err(RespError::Incomplete);
    }

    let cr = cursor.get_u8();
    let lf = cursor.get_u8();
    if cr != b'\r' || lf != b'\n' {
        return Err(RespError::InvalidFormat(
            "bulk string not terminated with \\r\\n".to_string(),
        ));
    }

    Ok(RespValue::BulkString(Some(Bytes::from(data))))
}

/// Parse an array: *2\r\n<element1><element2>
///
/// Recursively parses each element
fn parse_array(cursor: &mut Cursor<&[u8]>) -> Result<RespValue, RespError> {
    let len_str = read_line(cursor)?;
    let len = len_str
        .parse::<i64>()
        .map_err(|_| RespError::InvalidInteger(len_str.clone()))?;

    if len < 0 {
        return Err(RespError::InvalidFormat(format!(
            "invalid array length: {}",
            len
        )));
    }

    let len = len as usize;
    let mut elements = Vec::with_capacity(len);

    for _ in 0..len {
        let element = parse_value_internal(cursor)?;
        elements.push(element);
    }

    Ok(RespValue::Array(elements))
}

/// Read a line (up to \r\n) and return as String
///
/// ## Key Function:
/// Most RESP elements end with \r\n, so this is used everywhere
fn read_line(cursor: &mut Cursor<&[u8]>) -> Result<String, RespError> {
    let start = cursor.position() as usize;
    let slice = cursor.get_ref();

    // Find \r\n
    for i in start..slice.len() - 1 {
        if slice[i] == b'\r' && slice[i + 1] == b'\n' {
            let line = &slice[start..i];
            cursor.set_position((i + 2) as u64); // Skip \r\n
            return String::from_utf8(line.to_vec())
                .map_err(|_| RespError::InvalidFormat("invalid UTF-8".to_string()));
        }
    }

    Err(RespError::Incomplete)
}

/// Convert a RespValue (usually an array) into a RespCommand
///
/// ## Command Format:
/// Commands are arrays where the first element is the command name:
/// - ["GET", "key"] -> Get("key")
/// - ["SET", "key", "value"] -> Set("key", "value", None)
/// - ["SET", "key", "value", "EX", "60"] -> Set("key", "value", Some(60))
pub fn value_to_command(value: RespValue) -> Result<RespCommand, RespError> {
    let elements = match value {
        RespValue::Array(elements) => elements,
        _ => {
            return Err(RespError::InvalidFormat(
                "command must be an array".to_string(),
            ))
        }
    };

    if elements.is_empty() {
        return Err(RespError::InvalidFormat("empty command array".to_string()));
    }

    // Extract command name
    let cmd_name = match &elements[0] {
        RespValue::BulkString(Some(bytes)) => {
            String::from_utf8(bytes.to_vec())
                .map_err(|_| RespError::InvalidFormat("invalid UTF-8 in command".to_string()))?
                .to_uppercase()
        }
        _ => {
            return Err(RespError::InvalidFormat(
                "command name must be bulk string".to_string(),
            ))
        }
    };

    // Parse based on command
    match cmd_name.as_str() {
        "PING" => Ok(RespCommand::Ping),

        "GET" => {
            if elements.len() != 2 {
                return Err(RespError::InvalidFormat("GET requires 1 argument".to_string()));
            }
            let key = extract_bulk_string(&elements[1])?;
            Ok(RespCommand::Get(key))
        }

        "SET" => {
            if elements.len() < 3 {
                return Err(RespError::InvalidFormat(
                    "SET requires at least 2 arguments".to_string(),
                ));
            }
            let key = extract_bulk_string(&elements[1])?;
            let value = extract_bulk_bytes(&elements[2])?;

            // Check for EX option
            let ttl = if elements.len() >= 5 {
                if let RespValue::BulkString(Some(option)) = &elements[3] {
                    if option.as_ref() == b"EX" || option.as_ref() == b"ex" {
                        let ttl_str = extract_bulk_string(&elements[4])?;
                        let ttl = ttl_str.parse::<u64>().map_err(|_| {
                            RespError::InvalidInteger(format!("invalid TTL: {}", ttl_str))
                        })?;
                        Some(ttl)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            Ok(RespCommand::Set(key, value, ttl))
        }

        "DEL" => {
            if elements.len() != 2 {
                return Err(RespError::InvalidFormat("DEL requires 1 argument".to_string()));
            }
            let key = extract_bulk_string(&elements[1])?;
            Ok(RespCommand::Del(key))
        }

        "EXPIRE" => {
            if elements.len() != 3 {
                return Err(RespError::InvalidFormat(
                    "EXPIRE requires 2 arguments".to_string(),
                ));
            }
            let key = extract_bulk_string(&elements[1])?;
            let seconds_str = extract_bulk_string(&elements[2])?;
            let seconds = seconds_str
                .parse::<u64>()
                .map_err(|_| RespError::InvalidInteger(format!("invalid seconds: {}", seconds_str)))?;
            Ok(RespCommand::Expire(key, seconds))
        }

        "TTL" => {
            if elements.len() != 2 {
                return Err(RespError::InvalidFormat("TTL requires 1 argument".to_string()));
            }
            let key = extract_bulk_string(&elements[1])?;
            Ok(RespCommand::Ttl(key))
        }

        _ => Err(RespError::UnsupportedCommand(cmd_name)),
    }
}

/// Helper: Extract String from bulk string
fn extract_bulk_string(value: &RespValue) -> Result<String, RespError> {
    match value {
        RespValue::BulkString(Some(bytes)) => String::from_utf8(bytes.to_vec())
            .map_err(|_| RespError::InvalidFormat("invalid UTF-8".to_string())),
        _ => Err(RespError::InvalidFormat(
            "expected bulk string".to_string(),
        )),
    }
}

/// Helper: Extract Bytes from bulk string
fn extract_bulk_bytes(value: &RespValue) -> Result<Bytes, RespError> {
    match value {
        RespValue::BulkString(Some(bytes)) => Ok(bytes.clone()),
        _ => Err(RespError::InvalidFormat(
            "expected bulk string".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_string() {
        let mut parser = RespParser::new();
        parser.feed(b"+OK\r\n");

        let value = parser.parse_value().unwrap();
        assert_eq!(value, RespValue::SimpleString("OK".to_string()));
    }

    #[test]
    fn test_parse_error() {
        let mut parser = RespParser::new();
        parser.feed(b"-ERR unknown command\r\n");

        let value = parser.parse_value().unwrap();
        assert_eq!(value, RespValue::Error("ERR unknown command".to_string()));
    }

    #[test]
    fn test_parse_integer() {
        let mut parser = RespParser::new();
        parser.feed(b":42\r\n");

        let value = parser.parse_value().unwrap();
        assert_eq!(value, RespValue::Integer(42));
    }

    #[test]
    fn test_parse_negative_integer() {
        let mut parser = RespParser::new();
        parser.feed(b":-1\r\n");

        let value = parser.parse_value().unwrap();
        assert_eq!(value, RespValue::Integer(-1));
    }

    #[test]
    fn test_parse_bulk_string() {
        let mut parser = RespParser::new();
        parser.feed(b"$6\r\nfoobar\r\n");

        let value = parser.parse_value().unwrap();
        assert_eq!(
            value,
            RespValue::BulkString(Some(Bytes::from("foobar")))
        );
    }

    #[test]
    fn test_parse_null_bulk_string() {
        let mut parser = RespParser::new();
        parser.feed(b"$-1\r\n");

        let value = parser.parse_value().unwrap();
        assert_eq!(value, RespValue::BulkString(None));
    }

    #[test]
    fn test_parse_empty_bulk_string() {
        let mut parser = RespParser::new();
        parser.feed(b"$0\r\n\r\n");

        let value = parser.parse_value().unwrap();
        assert_eq!(value, RespValue::BulkString(Some(Bytes::from(""))));
    }

    #[test]
    fn test_parse_array() {
        let mut parser = RespParser::new();
        parser.feed(b"*2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n");

        let value = parser.parse_value().unwrap();
        assert_eq!(
            value,
            RespValue::Array(vec![
                RespValue::BulkString(Some(Bytes::from("foo"))),
                RespValue::BulkString(Some(Bytes::from("bar"))),
            ])
        );
    }

    #[test]
    fn test_parse_nested_array() {
        let mut parser = RespParser::new();
        // Array containing an array
        parser.feed(b"*2\r\n*1\r\n:1\r\n:2\r\n");

        let value = parser.parse_value().unwrap();
        assert_eq!(
            value,
            RespValue::Array(vec![
                RespValue::Array(vec![RespValue::Integer(1)]),
                RespValue::Integer(2),
            ])
        );
    }

    #[test]
    fn test_parse_incomplete() {
        let mut parser = RespParser::new();
        parser.feed(b"$6\r\nfoo"); // Incomplete bulk string

        let result = parser.parse_value();
        assert_eq!(result, Err(RespError::Incomplete));

        // Feed rest of data
        parser.feed(b"bar\r\n");
        let value = parser.parse_value().unwrap();
        assert_eq!(
            value,
            RespValue::BulkString(Some(Bytes::from("foobar")))
        );
    }

    #[test]
    fn test_parse_get_command() {
        let mut parser = RespParser::new();
        parser.feed(b"*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n");

        let cmd = parser.parse_command().unwrap();
        assert_eq!(cmd, RespCommand::Get("key".to_string()));
    }

    #[test]
    fn test_parse_set_command() {
        let mut parser = RespParser::new();
        parser.feed(b"*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$5\r\nvalue\r\n");

        let cmd = parser.parse_command().unwrap();
        assert_eq!(
            cmd,
            RespCommand::Set("key".to_string(), Bytes::from("value"), None)
        );
    }

    #[test]
    fn test_parse_set_command_with_ttl() {
        let mut parser = RespParser::new();
        parser.feed(b"*5\r\n$3\r\nSET\r\n$3\r\nkey\r\n$5\r\nvalue\r\n$2\r\nEX\r\n$2\r\n60\r\n");

        let cmd = parser.parse_command().unwrap();
        assert_eq!(
            cmd,
            RespCommand::Set("key".to_string(), Bytes::from("value"), Some(60))
        );
    }

    #[test]
    fn test_parse_del_command() {
        let mut parser = RespParser::new();
        parser.feed(b"*2\r\n$3\r\nDEL\r\n$3\r\nkey\r\n");

        let cmd = parser.parse_command().unwrap();
        assert_eq!(cmd, RespCommand::Del("key".to_string()));
    }

    #[test]
    fn test_parse_ping_command() {
        let mut parser = RespParser::new();
        parser.feed(b"*1\r\n$4\r\nPING\r\n");

        let cmd = parser.parse_command().unwrap();
        assert_eq!(cmd, RespCommand::Ping);
    }

    #[test]
    fn test_multiple_commands_pipelined() {
        let mut parser = RespParser::new();
        // Two commands in one buffer
        parser.feed(b"*1\r\n$4\r\nPING\r\n*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n");

        let cmd1 = parser.parse_command().unwrap();
        assert_eq!(cmd1, RespCommand::Ping);

        let cmd2 = parser.parse_command().unwrap();
        assert_eq!(cmd2, RespCommand::Get("key".to_string()));
    }

    #[test]
    fn test_partial_command_across_feeds() {
        let mut parser = RespParser::new();

        // Feed command in chunks
        parser.feed(b"*2\r\n$3\r\n");
        assert_eq!(parser.parse_command(), Err(RespError::Incomplete));

        parser.feed(b"GET\r\n");
        assert_eq!(parser.parse_command(), Err(RespError::Incomplete));

        parser.feed(b"$3\r\nkey\r\n");
        let cmd = parser.parse_command().unwrap();
        assert_eq!(cmd, RespCommand::Get("key".to_string()));
    }
}

