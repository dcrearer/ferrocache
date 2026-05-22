// Property-based tests for RESP protocol
//
// These tests generate thousands of random inputs to verify properties that should
// hold for ALL inputs, not just specific test cases.

#![cfg(test)]

use crate::protocol::{parser::RespParser, serializer::serialize, RespCommand, RespValue};
use proptest::prelude::*;

// Import Arbitrary implementations
#[allow(unused_imports)]
use super::proptest_helpers;

// PROPERTY 1: Round-trip identity
//
// ## What it tests:
// For any RespValue, parse(serialize(value)) should equal value
//
// ## Why it matters:
// - Ensures serializer and parser are inverses
// - Catches bugs where data gets corrupted in round-trip
// - Tests all edge cases automatically (empty strings, negative numbers, nested arrays)
//
// ## How proptest works:
// 1. Generates random RespValue (using Arbitrary impl)
// 2. Serializes it to bytes
// 3. Parses bytes back to RespValue
// 4. Asserts they're equal
// 5. Repeats 256 times (default) with different random values
// 6. If failure found, "shrinks" to minimal failing case
proptest! {
    #[test]
    fn roundtrip_resp_value(value in any::<RespValue>()) {
        // Serialize the value
        let serialized = serialize(&value);

        // Parse it back
        let mut parser = RespParser::new();
        parser.feed(&serialized);
        let parsed = parser.parse_value()
            .expect("round-trip parse should succeed");

        // Should be identical
        prop_assert_eq!(parsed, value);
    }
}

// PROPERTY 2: Round-trip commands
//
// ## What it tests:
// Commands can be serialized as RESP arrays and parsed back
//
// ## Why separate from values:
// - Commands have specific structure (array of bulk strings)
// - Tests the command_to_value and value_to_command conversion
proptest! {
    #[test]
    fn roundtrip_command(cmd in any::<RespCommand>()) {
        // Convert command to RESP array
        let value = command_to_value(&cmd);

        // Serialize
        let serialized = serialize(&value);

        // Parse back
        let mut parser = RespParser::new();
        parser.feed(&serialized);
        let parsed_value = parser.parse_value()
            .expect("command parse should succeed");

        // Convert back to command
        let parsed_cmd = crate::protocol::parser::value_to_command(parsed_value)
            .expect("value_to_command should succeed");

        // Should match original
        prop_assert_eq!(parsed_cmd, cmd);
    }
}

// PROPERTY 3: Partial reads don't affect result
//
// ## What it tests:
// Splitting serialized data at ANY point shouldn't change the parsed result
//
// ## Why it matters:
// - Real TCP connections deliver data in chunks
// - Parser must handle incomplete data correctly
// - Tests the streaming parser's state management
//
// ## Example:
// Data: "*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n"
// Could arrive as:
// - "*2\r\n" then "$3\r\nGET\r\n$3\r\nkey\r\n"
// - "*2\r\n$3\r\nG" then "ET\r\n$3\r\nkey\r\n"
// - Any other split point
// All should produce same result!
proptest! {
    #[test]
    fn partial_reads_equivalent(
        value in any::<RespValue>(),
        split_point in 0..100usize,
    ) {
        let serialized = serialize(&value);
        let len = serialized.len();

        // Clamp split point to valid range
        let split = std::cmp::min(split_point, len);

        // Parse in one chunk
        let mut parser_full = RespParser::new();
        parser_full.feed(&serialized);
        let parsed_full = parser_full.parse_value()
            .expect("full parse should succeed");

        // Parse in two chunks
        let mut parser_split = RespParser::new();
        parser_split.feed(&serialized[..split]);
        parser_split.feed(&serialized[split..]);

        // Now parse (all data is in buffer)
        let parsed_split = parser_split.parse_value()
            .expect("split parse should succeed after feeding all data");

        // Results should match
        prop_assert_eq!(parsed_split, parsed_full);
    }
}

// PROPERTY 4: Parser never panics
//
// ## What it tests:
// Parser handles arbitrary bytes gracefully (no panics, no unwraps)
//
// ## Why it matters:
// - Network data could be malicious
// - Parser should return errors, not crash
// - Security: prevents DoS via malformed input
//
// ## What it accepts:
// Arbitrary bytes (any length, any content)
// Parser should either:
// - Return Ok(value) if valid RESP
// - Return Err(Incomplete) if need more data
// - Return Err(InvalidFormat) if malformed
// - NEVER panic!
proptest! {
    #[test]
    fn arbitrary_bytes_dont_panic(bytes in prop::collection::vec(any::<u8>(), 0..1000)) {
        let mut parser = RespParser::new();
        parser.feed(&bytes);

        // Try to parse - should not panic
        let _ = parser.parse_value();

        // Success! (even if parse failed, we didn't panic)
    }
}

// PROPERTY 5: Multiple commands (pipelining)
//
// ## What it tests:
// Multiple commands can be pipelined and parsed in order
//
// ## Why it matters:
// - Redis clients pipeline commands for performance
// - Parser must maintain correct boundaries between commands
// - Tests buffer management (advancing by correct amount)
proptest! {
    #[test]
    fn pipeline_multiple_commands(commands in prop::collection::vec(any::<RespCommand>(), 1..10)) {
        // Serialize all commands into one buffer
        let mut combined = Vec::new();
        for cmd in &commands {
            let value = command_to_value(cmd);
            let serialized = serialize(&value);
            combined.extend_from_slice(&serialized);
        }

        // Parse them back
        let mut parser = RespParser::new();
        parser.feed(&combined);

        let mut parsed_commands = Vec::new();
        for _ in 0..commands.len() {
            let cmd = parser.parse_command()
                .expect("pipelined command parse should succeed");
            parsed_commands.push(cmd);
        }

        // Should match original sequence
        prop_assert_eq!(parsed_commands, commands);
    }
}

// PROPERTY 6: Empty buffer behavior
//
// ## What it tests:
// Parser handles empty input gracefully
proptest! {
    #[test]
    fn empty_buffer_returns_incomplete(_dummy in 0..1) {
        let mut parser = RespParser::new();
        let result = parser.parse_value();

        // Should return Incomplete, not panic
        prop_assert!(result.is_err());
    }
}

// Helper: Convert command to RESP array value
//
// ## Why needed:
// Commands are transmitted as RESP arrays, so we need to convert
// RespCommand -> RespValue::Array for serialization
fn command_to_value(cmd: &RespCommand) -> RespValue {
    use bytes::Bytes;

    match cmd {
        RespCommand::Ping => RespValue::Array(vec![RespValue::BulkString(Some(
            Bytes::from("PING"),
        ))]),

        RespCommand::Get(key) => RespValue::Array(vec![
            RespValue::BulkString(Some(Bytes::from("GET"))),
            RespValue::BulkString(Some(Bytes::from(key.clone()))),
        ]),

        RespCommand::Set(key, value, ttl) => {
            let mut elements = vec![
                RespValue::BulkString(Some(Bytes::from("SET"))),
                RespValue::BulkString(Some(Bytes::from(key.clone()))),
                RespValue::BulkString(Some(value.clone())),
            ];

            if let Some(seconds) = ttl {
                elements.push(RespValue::BulkString(Some(Bytes::from("EX"))));
                let ttl_str = seconds.to_string();
                elements.push(RespValue::BulkString(Some(Bytes::from(ttl_str))));
            }

            RespValue::Array(elements)
        }

        RespCommand::Del(key) => RespValue::Array(vec![
            RespValue::BulkString(Some(Bytes::from("DEL"))),
            RespValue::BulkString(Some(Bytes::from(key.clone()))),
        ]),

        RespCommand::Expire(key, seconds) => {
            let seconds_str = seconds.to_string();
            RespValue::Array(vec![
                RespValue::BulkString(Some(Bytes::from("EXPIRE"))),
                RespValue::BulkString(Some(Bytes::from(key.clone()))),
                RespValue::BulkString(Some(Bytes::from(seconds_str))),
            ])
        }

        RespCommand::Ttl(key) => RespValue::Array(vec![
            RespValue::BulkString(Some(Bytes::from("TTL"))),
            RespValue::BulkString(Some(Bytes::from(key.clone()))),
        ]),
    }
}
