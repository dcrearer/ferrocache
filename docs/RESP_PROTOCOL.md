# Redis RESP Protocol Reference

Quick reference for implementing the Redis Serialization Protocol (RESP) subset in FerroCache.

## Overview

RESP is the protocol used by Redis clients to communicate with the server. It's human-readable, simple to implement, and efficient to parse.

## Data Types

RESP has 5 data types, each starting with a type prefix:

| Type | Prefix | Format | Example |
|------|--------|--------|---------|
| Simple String | `+` | `+OK\r\n` | `+OK\r\n` |
| Error | `-` | `-Error message\r\n` | `-ERR unknown command\r\n` |
| Integer | `:` | `:1000\r\n` | `:42\r\n` |
| Bulk String | `$` | `$6\r\nfoobar\r\n` | `$5\r\nhello\r\n` |
| Array | `*` | `*2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n` | See below |

**Note:** All RESP types end with `\r\n` (CRLF)

## Examples

### Simple String
```
+OK\r\n
```
Used for simple responses like "OK", "PONG"

### Error
```
-ERR unknown command 'foobar'\r\n
```
Used to return error messages

### Integer
```
:1000\r\n
```
Used for numeric responses (e.g., TTL value)

### Bulk String (Null)
```
$-1\r\n
```
Represents a null value (e.g., key doesn't exist)

### Bulk String (Value)
```
$6\r\n
foobar\r\n
```
- `$6` = length is 6 bytes
- `foobar` = the actual data
- `\r\n` = terminator

### Array
```
*2\r\n
$3\r\n
foo\r\n
$3\r\n
bar\r\n
```
Represents: `["foo", "bar"]`

## Command Examples

### GET Command
**Client sends:**
```
*2\r\n
$3\r\n
GET\r\n
$3\r\n
key\r\n
```
This is an array of 2 bulk strings: `["GET", "key"]`

**Server responds (key exists):**
```
$5\r\n
value\r\n
```

**Server responds (key doesn't exist):**
```
$-1\r\n
```

### SET Command
**Client sends:**
```
*3\r\n
$3\r\n
SET\r\n
$3\r\n
key\r\n
$5\r\n
value\r\n
```
Array of 3 bulk strings: `["SET", "key", "value"]`

**Server responds:**
```
+OK\r\n
```

### SET with TTL (EX)
**Client sends:**
```
*5\r\n
$3\r\n
SET\r\n
$3\r\n
key\r\n
$5\r\n
value\r\n
$2\r\n
EX\r\n
$2\r\n
60\r\n
```
Array of 5: `["SET", "key", "value", "EX", "60"]`

### DEL Command
**Client sends:**
```
*2\r\n
$3\r\n
DEL\r\n
$3\r\n
key\r\n
```

**Server responds (number of keys deleted):**
```
:1\r\n
```

### EXPIRE Command
**Client sends:**
```
*3\r\n
$6\r\n
EXPIRE\r\n
$3\r\n
key\r\n
$2\r\n
60\r\n
```
Array: `["EXPIRE", "key", "60"]`

**Server responds (1 if set, 0 if key doesn't exist):**
```
:1\r\n
```

### TTL Command
**Client sends:**
```
*2\r\n
$3\r\n
TTL\r\n
$3\r\n
key\r\n
```

**Server responds (seconds remaining):**
```
:42\r\n
```

**Key doesn't exist:**
```
:-2\r\n
```

**Key exists but has no TTL:**
```
:-1\r\n
```

### PING Command
**Client sends:**
```
*1\r\n
$4\r\n
PING\r\n
```

**Server responds:**
```
+PONG\r\n
```

## Implementation Notes

### Parser Requirements
1. **Streaming:** Handle partial reads (commands may arrive in chunks)
2. **Buffering:** Buffer incomplete commands until full command received
3. **Error Handling:** Validate lengths, detect malformed commands
4. **Performance:** Minimize allocations, avoid copying data when possible

### Serializer Requirements
1. **Efficiency:** Pre-calculate lengths for bulk strings
2. **Correctness:** Always use `\r\n`, not just `\n`
3. **Null handling:** Use `$-1\r\n` for null bulk strings

### Testing Strategy
```rust
// Test cases to implement:
#[test]
fn parse_simple_get() { }

#[test]
fn parse_set_with_ttl() { }

#[test]
fn parse_partial_command() { } // Streaming test

#[test]
fn parse_pipelined_commands() { } // Multiple commands

#[test]
fn parse_invalid_length() { } // Error handling

#[test]
fn serialize_bulk_string() { }

#[test]
fn serialize_null_bulk_string() { }

#[test]
fn serialize_array() { }
```

## FerroCache Command Set

Minimum viable commands:

| Command | Args | Response | Description |
|---------|------|----------|-------------|
| `GET` | key | Bulk string or null | Retrieve value |
| `SET` | key value [EX seconds] | Simple string "+OK" | Set value with optional TTL |
| `DEL` | key | Integer (0 or 1) | Delete key |
| `EXPIRE` | key seconds | Integer (0 or 1) | Set TTL on existing key |
| `TTL` | key | Integer (-2, -1, or seconds) | Get remaining TTL |
| `PING` | - | Simple string "+PONG" | Health check |

## Resources

- [RESP Protocol Specification](https://redis.io/docs/reference/protocol-spec/)
- [Redis Command Reference](https://redis.io/commands/)

## Testing with redis-cli

Once server is running:
```bash
# Connect
redis-cli -h localhost -p 6379

# Test commands
> PING
PONG
> SET mykey myvalue
OK
> GET mykey
"myvalue"
> EXPIRE mykey 60
(integer) 1
> TTL mykey
(integer) 59
```
