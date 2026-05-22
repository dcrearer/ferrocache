pub mod parser;
pub mod serializer;

#[cfg(test)]
mod proptest_helpers;

#[cfg(test)]
mod proptests;

use bytes::Bytes;

#[derive(Debug, Clone, PartialEq)]
pub enum RespValue {
    SimpleString(String),
    Error(String),
    Integer(i64),
    BulkString(Option<Bytes>), // None = null
    Array(Vec<RespValue>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum RespCommand {
    Get(String),
    Set(String, Bytes, Option<u64>), // key, value, ttl_seconds
    Del(String),
    Expire(String, u64),
    Ttl(String),
    Ping,
}
