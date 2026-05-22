// Property-based testing helpers
//
// This module provides Arbitrary implementations for generating random RESP values
// and commands for property-based testing with proptest.

#![cfg(test)]

use crate::protocol::{RespCommand, RespValue};
use bytes::Bytes;
use proptest::prelude::*;

/// Generate arbitrary RespValue for testing
///
/// ## Strategy:
/// - Generates all RESP types (SimpleString, Error, Integer, BulkString, Array)
/// - Limits recursion depth for arrays (to avoid stack overflow)
/// - Generates reasonable sizes (strings up to 100 chars, arrays up to 10 elements)
///
/// ## Why Limits:
/// - Without limits, proptest could generate huge nested structures
/// - Tests would be slow and hit stack limits
/// - Real-world RESP is rarely deeply nested
impl Arbitrary for RespValue {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        // Use prop_oneof! to choose between different RESP types
        let leaf = prop_oneof![
            // Simple strings: printable ASCII, no \r\n
            "[a-zA-Z0-9 ]{0,100}".prop_map(RespValue::SimpleString),

            // Errors: same format as simple strings
            "[a-zA-Z0-9 ]{0,100}".prop_map(RespValue::Error),

            // Integers: full i64 range
            any::<i64>().prop_map(RespValue::Integer),

            // Bulk strings: Some or None
            prop_oneof![
                // Some: arbitrary bytes
                prop::collection::vec(any::<u8>(), 0..100)
                    .prop_map(|v| RespValue::BulkString(Some(Bytes::from(v)))),
                // None: null bulk string
                Just(RespValue::BulkString(None)),
            ],
        ];

        // Recursive strategy for arrays (with depth limit)
        leaf.prop_recursive(
            3,  // Max depth
            256, // Max total nodes
            10, // Max items per collection
            |inner| {
                // Generate arrays of inner values
                prop::collection::vec(inner, 0..10).prop_map(RespValue::Array)
            },
        )
        .boxed()
    }
}

/// Generate arbitrary RespCommand for testing
///
/// ## Coverage:
/// - All commands: GET, SET, DEL, EXPIRE, TTL, PING
/// - SET with and without TTL
/// - Various key/value combinations
impl Arbitrary for RespCommand {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        prop_oneof![
            // PING: no arguments
            Just(RespCommand::Ping),

            // GET: arbitrary key (printable ASCII for simplicity)
            "[a-zA-Z0-9_]{1,50}".prop_map(RespCommand::Get),

            // SET: key, value, optional TTL
            (
                "[a-zA-Z0-9_]{1,50}",          // key
                prop::collection::vec(any::<u8>(), 0..100), // value
                any::<Option<u64>>(),          // ttl
            )
                .prop_map(|(k, v, ttl)| RespCommand::Set(k, Bytes::from(v), ttl)),

            // DEL: arbitrary key
            "[a-zA-Z0-9_]{1,50}".prop_map(RespCommand::Del),

            // EXPIRE: key and seconds
            ("[a-zA-Z0-9_]{1,50}", 0u64..86400u64) // up to 1 day
                .prop_map(|(k, s)| RespCommand::Expire(k, s)),

            // TTL: arbitrary key
            "[a-zA-Z0-9_]{1,50}".prop_map(RespCommand::Ttl),
        ]
        .boxed()
    }
}
