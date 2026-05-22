use crate::protocol::RespValue;
use bytes::{BufMut, Bytes, BytesMut};

/// Serialize a RespValue to RESP protocol bytes
///
/// ## Usage:
/// ```rust,no_run
/// let value = RespValue::SimpleString("OK".to_string());
/// let bytes = serialize(&value);
/// assert_eq!(bytes, b"+OK\r\n");
/// ```
pub fn serialize(value: &RespValue) -> Bytes {
    let mut buf = BytesMut::new();
    serialize_into(&mut buf, value);
    buf.freeze()
}

/// Serialize into an existing buffer (for efficiency)
fn serialize_into(buf: &mut BytesMut, value: &RespValue) {
    match value {
        RespValue::SimpleString(s) => {
            buf.put_u8(b'+');
            buf.put_slice(s.as_bytes());
            buf.put_slice(b"\r\n");
        }

        RespValue::Error(s) => {
            buf.put_u8(b'-');
            buf.put_slice(s.as_bytes());
            buf.put_slice(b"\r\n");
        }

        RespValue::Integer(n) => {
            buf.put_u8(b':');
            buf.put_slice(n.to_string().as_bytes());
            buf.put_slice(b"\r\n");
        }

        RespValue::BulkString(Some(data)) => {
            buf.put_u8(b'$');
            buf.put_slice(data.len().to_string().as_bytes());
            buf.put_slice(b"\r\n");
            buf.put_slice(data);
            buf.put_slice(b"\r\n");
        }

        RespValue::BulkString(None) => {
            buf.put_slice(b"$-1\r\n");
        }

        RespValue::Array(elements) => {
            buf.put_u8(b'*');
            buf.put_slice(elements.len().to_string().as_bytes());
            buf.put_slice(b"\r\n");
            for element in elements {
                serialize_into(buf, element);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_simple_string() {
        let value = RespValue::SimpleString("OK".to_string());
        assert_eq!(serialize(&value), Bytes::from("+OK\r\n"));
    }

    #[test]
    fn test_serialize_error() {
        let value = RespValue::Error("ERR unknown".to_string());
        assert_eq!(serialize(&value), Bytes::from("-ERR unknown\r\n"));
    }

    #[test]
    fn test_serialize_integer() {
        let value = RespValue::Integer(42);
        assert_eq!(serialize(&value), Bytes::from(":42\r\n"));
    }

    #[test]
    fn test_serialize_bulk_string() {
        let value = RespValue::BulkString(Some(Bytes::from("hello")));
        assert_eq!(serialize(&value), Bytes::from("$5\r\nhello\r\n"));
    }

    #[test]
    fn test_serialize_null_bulk_string() {
        let value = RespValue::BulkString(None);
        assert_eq!(serialize(&value), Bytes::from("$-1\r\n"));
    }

    #[test]
    fn test_serialize_array() {
        let value = RespValue::Array(vec![
            RespValue::BulkString(Some(Bytes::from("GET"))),
            RespValue::BulkString(Some(Bytes::from("key"))),
        ]);
        assert_eq!(
            serialize(&value),
            Bytes::from("*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n")
        );
    }
}
