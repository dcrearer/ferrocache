pub mod handlers;

use crate::protocol::{RespCommand, RespValue};
use bytes::Bytes;
use thiserror::Error;

/// Errors that can occur during command execution
#[derive(Debug, Error)]
pub enum CommandError {
    #[error("wrong number of arguments for command")]
    WrongArgCount,

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("unknown command: {0}")]
    UnknownCommand(String),
}

/// Convert RespValue (parsed from wire) into RespCommand
///
/// RESP commands come as arrays of bulk strings:
/// *2\r\n$3\r\nGET\r\n$3\r\nkey\r\n -> ["GET", "key"]
pub fn parse_command(value: RespValue) -> Result<RespCommand, CommandError> {
    let parts = match value {
        RespValue::Array(parts) => parts,
        _ => return Err(CommandError::InvalidArgument("expected array".to_string())),
    };

    if parts.is_empty() {
        return Err(CommandError::InvalidArgument("empty command".to_string()));
    }

    // Extract command name
    let cmd_name = match &parts[0] {
        RespValue::BulkString(Some(bytes)) => {
            String::from_utf8(bytes.to_vec())
                .map_err(|_| CommandError::InvalidArgument("invalid UTF-8".to_string()))?
        }
        _ => {
            return Err(CommandError::InvalidArgument(
                "command must be bulk string".to_string(),
            ))
        }
    };

    // Parse by command type
    match cmd_name.to_uppercase().as_str() {
        "PING" => {
            if parts.len() != 1 {
                return Err(CommandError::WrongArgCount);
            }
            Ok(RespCommand::Ping)
        }

        "GET" => {
            if parts.len() != 2 {
                return Err(CommandError::WrongArgCount);
            }
            let key = extract_string(&parts[1])?;
            Ok(RespCommand::Get(key))
        }

        "SET" => {
            // SET key value [EX seconds]
            if parts.len() < 3 {
                return Err(CommandError::WrongArgCount);
            }

            let key = extract_string(&parts[1])?;
            let value = extract_bytes(&parts[2])?;

            let ttl = if parts.len() >= 5 {
                let option = extract_string(&parts[3])?;
                if option.to_uppercase() == "EX" {
                    let seconds = extract_integer(&parts[4])?;
                    Some(seconds as u64)
                } else {
                    return Err(CommandError::InvalidArgument(format!(
                        "unknown option: {}",
                        option
                    )));
                }
            } else if parts.len() == 3 {
                None
            } else {
                return Err(CommandError::WrongArgCount);
            };

            Ok(RespCommand::Set(key, value, ttl))
        }

        "DEL" => {
            if parts.len() != 2 {
                return Err(CommandError::WrongArgCount);
            }
            let key = extract_string(&parts[1])?;
            Ok(RespCommand::Del(key))
        }

        "EXPIRE" => {
            if parts.len() != 3 {
                return Err(CommandError::WrongArgCount);
            }
            let key = extract_string(&parts[1])?;
            let seconds = extract_integer(&parts[2])?;
            if seconds < 0 {
                return Err(CommandError::InvalidArgument(
                    "TTL must be non-negative".to_string(),
                ));
            }
            Ok(RespCommand::Expire(key, seconds as u64))
        }

        "TTL" => {
            if parts.len() != 2 {
                return Err(CommandError::WrongArgCount);
            }
            let key = extract_string(&parts[1])?;
            Ok(RespCommand::Ttl(key))
        }

        _ => Err(CommandError::UnknownCommand(cmd_name)),
    }
}

/// Extract a string from a RespValue
fn extract_string(value: &RespValue) -> Result<String, CommandError> {
    match value {
        RespValue::BulkString(Some(bytes)) => String::from_utf8(bytes.to_vec())
            .map_err(|_| CommandError::InvalidArgument("invalid UTF-8".to_string())),
        _ => Err(CommandError::InvalidArgument(
            "expected bulk string".to_string(),
        )),
    }
}

/// Extract bytes from a RespValue
fn extract_bytes(value: &RespValue) -> Result<Bytes, CommandError> {
    match value {
        RespValue::BulkString(Some(bytes)) => Ok(bytes.clone()),
        _ => Err(CommandError::InvalidArgument(
            "expected bulk string".to_string(),
        )),
    }
}

/// Extract an integer from a RespValue
fn extract_integer(value: &RespValue) -> Result<i64, CommandError> {
    match value {
        RespValue::BulkString(Some(bytes)) => {
            let s = String::from_utf8(bytes.to_vec())
                .map_err(|_| CommandError::InvalidArgument("invalid UTF-8".to_string()))?;
            s.parse::<i64>()
                .map_err(|_| CommandError::InvalidArgument("invalid integer".to_string()))
        }
        RespValue::Integer(n) => Ok(*n),
        _ => Err(CommandError::InvalidArgument(
            "expected integer".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bulk_string(s: &str) -> RespValue {
        RespValue::BulkString(Some(Bytes::from(s.to_string())))
    }

    #[test]
    fn test_parse_ping() {
        let cmd = RespValue::Array(vec![make_bulk_string("PING")]);
        assert_eq!(parse_command(cmd).unwrap(), RespCommand::Ping);
    }

    #[test]
    fn test_parse_get() {
        let cmd = RespValue::Array(vec![make_bulk_string("GET"), make_bulk_string("mykey")]);
        assert_eq!(
            parse_command(cmd).unwrap(),
            RespCommand::Get("mykey".to_string())
        );
    }

    #[test]
    fn test_parse_set_without_ttl() {
        let cmd = RespValue::Array(vec![
            make_bulk_string("SET"),
            make_bulk_string("key1"),
            make_bulk_string("value1"),
        ]);
        assert_eq!(
            parse_command(cmd).unwrap(),
            RespCommand::Set("key1".to_string(), Bytes::from("value1"), None)
        );
    }

    #[test]
    fn test_parse_set_with_ttl() {
        let cmd = RespValue::Array(vec![
            make_bulk_string("SET"),
            make_bulk_string("key1"),
            make_bulk_string("value1"),
            make_bulk_string("EX"),
            make_bulk_string("60"),
        ]);
        assert_eq!(
            parse_command(cmd).unwrap(),
            RespCommand::Set("key1".to_string(), Bytes::from("value1"), Some(60))
        );
    }

    #[test]
    fn test_parse_del() {
        let cmd = RespValue::Array(vec![make_bulk_string("DEL"), make_bulk_string("mykey")]);
        assert_eq!(
            parse_command(cmd).unwrap(),
            RespCommand::Del("mykey".to_string())
        );
    }

    #[test]
    fn test_parse_expire() {
        let cmd = RespValue::Array(vec![
            make_bulk_string("EXPIRE"),
            make_bulk_string("mykey"),
            make_bulk_string("120"),
        ]);
        assert_eq!(
            parse_command(cmd).unwrap(),
            RespCommand::Expire("mykey".to_string(), 120)
        );
    }

    #[test]
    fn test_parse_ttl() {
        let cmd = RespValue::Array(vec![make_bulk_string("TTL"), make_bulk_string("mykey")]);
        assert_eq!(
            parse_command(cmd).unwrap(),
            RespCommand::Ttl("mykey".to_string())
        );
    }

    #[test]
    fn test_wrong_arg_count() {
        let cmd = RespValue::Array(vec![make_bulk_string("GET")]);
        assert!(matches!(
            parse_command(cmd),
            Err(CommandError::WrongArgCount)
        ));
    }

    #[test]
    fn test_unknown_command() {
        let cmd = RespValue::Array(vec![make_bulk_string("UNKNOWN")]);
        assert!(matches!(
            parse_command(cmd),
            Err(CommandError::UnknownCommand(_))
        ));
    }
}
