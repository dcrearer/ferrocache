use crate::cache::storage::CacheStorage;
use crate::protocol::{RespCommand, RespValue};
use std::sync::Arc;
use std::time::Duration;

/// Execute a command against the cache and return a response
///
/// This is the bridge between the RESP protocol and cache operations.
/// It takes a parsed command, executes it, and returns a RESP response.
pub fn execute_command(cache: &Arc<CacheStorage>, command: RespCommand) -> RespValue {
    match command {
        RespCommand::Ping => {
            // PING -> +PONG
            RespValue::SimpleString("PONG".to_string())
        }

        RespCommand::Get(key) => {
            // GET key -> bulk string or null
            match cache.get(&key) {
                Some(value) => RespValue::BulkString(Some(value)),
                None => RespValue::BulkString(None), // Redis returns null for missing keys
            }
        }

        RespCommand::Set(key, value, ttl_seconds) => {
            // SET key value [EX seconds] -> +OK
            let ttl = ttl_seconds.map(|s| Duration::from_secs(s));
            cache.set(key, value, ttl);
            RespValue::SimpleString("OK".to_string())
        }

        RespCommand::Del(key) => {
            // DEL key -> :1 (deleted) or :0 (not found)
            let existed = cache.remove(&key);
            RespValue::Integer(if existed { 1 } else { 0 })
        }

        RespCommand::Expire(key, seconds) => {
            // EXPIRE key seconds -> :1 (success) or :0 (key not found)
            let success = cache.set_expiration(&key, Duration::from_secs(seconds));
            RespValue::Integer(if success { 1 } else { 0 })
        }

        RespCommand::Ttl(key) => {
            // TTL key -> seconds remaining, -2 (not found), or -1 (no expiration)
            match cache.get_ttl(&key) {
                Some(Some(ttl)) => RespValue::Integer(ttl.as_secs() as i64),
                Some(None) => RespValue::Integer(-1), // Key exists but no TTL
                None => RespValue::Integer(-2),       // Key doesn't exist
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn test_ping() {
        let cache = Arc::new(CacheStorage::new(1024 * 1024));
        let response = execute_command(&cache, RespCommand::Ping);
        assert_eq!(response, RespValue::SimpleString("PONG".to_string()));
    }

    #[test]
    fn test_set_and_get() {
        let cache = Arc::new(CacheStorage::new(1024 * 1024));

        // SET
        let response = execute_command(
            &cache,
            RespCommand::Set("mykey".to_string(), Bytes::from("myvalue"), None),
        );
        assert_eq!(response, RespValue::SimpleString("OK".to_string()));

        // GET
        let response = execute_command(&cache, RespCommand::Get("mykey".to_string()));
        assert_eq!(
            response,
            RespValue::BulkString(Some(Bytes::from("myvalue")))
        );
    }

    #[test]
    fn test_get_missing_key() {
        let cache = Arc::new(CacheStorage::new(1024 * 1024));
        let response = execute_command(&cache, RespCommand::Get("missing".to_string()));
        assert_eq!(response, RespValue::BulkString(None));
    }

    #[test]
    fn test_del() {
        let cache = Arc::new(CacheStorage::new(1024 * 1024));

        // Set a key
        cache.set("mykey".to_string(), Bytes::from("value"), None);

        // Delete it
        let response = execute_command(&cache, RespCommand::Del("mykey".to_string()));
        assert_eq!(response, RespValue::Integer(1));

        // Delete again (should return 0)
        let response = execute_command(&cache, RespCommand::Del("mykey".to_string()));
        assert_eq!(response, RespValue::Integer(0));
    }

    #[test]
    fn test_set_with_ttl() {
        let cache = Arc::new(CacheStorage::new(1024 * 1024));

        // SET with TTL
        let response = execute_command(
            &cache,
            RespCommand::Set("mykey".to_string(), Bytes::from("value"), Some(60)),
        );
        assert_eq!(response, RespValue::SimpleString("OK".to_string()));

        // Should be retrievable immediately
        let response = execute_command(&cache, RespCommand::Get("mykey".to_string()));
        assert!(matches!(response, RespValue::BulkString(Some(_))));
    }
}
