use crate::cache::storage::CacheStorage;
use crate::commands::{parse_command, CommandError};
use crate::commands::handlers::execute_command;
use crate::protocol::parser::{RespError, RespParser};
use crate::protocol::serializer::serialize;
use crate::protocol::{RespCommand, RespValue};
use opentelemetry::metrics::Histogram;
use opentelemetry::trace::Status;
use opentelemetry::KeyValue;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, info_span, warn, Instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Command-execution latency histogram, in milliseconds, tagged by `command`.
///
/// Created once on first use (a histogram is a handle into the global meter;
/// recreating it per command would be wasteful). If OTLP export is disabled the
/// global meter is a no-op, so this is cheap either way.
fn command_latency() -> &'static Histogram<f64> {
    static HIST: OnceLock<Histogram<f64>> = OnceLock::new();
    HIST.get_or_init(|| {
        opentelemetry::global::meter("ferrocache")
            .f64_histogram("ferrocache.command.duration")
            .with_description("Command execution latency in milliseconds")
            .with_unit("ms")
            .build()
    })
}

/// Create a span named after the command type (GET/SET/DEL/...).
///
/// `tracing` requires span names to be compile-time string literals, so each
/// command variant gets its own `info_span!` with a literal name. Naming the
/// span per command (rather than a single "command" span with a `cmd` field)
/// lets APM/tracing backends aggregate latency and throughput *per operation*.
///
/// Names are the command *type* only — never the key or value — to keep span
/// cardinality low (per-key names would overwhelm the backend's index).
fn command_span(command: &RespCommand) -> tracing::Span {
    match command {
        RespCommand::Get(_) => info_span!("GET"),
        RespCommand::Set(_, _, _) => info_span!("SET"),
        RespCommand::Del(_) => info_span!("DEL"),
        RespCommand::Expire(_, _) => info_span!("EXPIRE"),
        RespCommand::Ttl(_) => info_span!("TTL"),
        RespCommand::Ping => info_span!("PING"),
    }
}

/// Low-cardinality command-type label (e.g. "GET") for metric attributes.
/// Type only — never the key — to keep metric cardinality bounded.
fn command_label(command: &RespCommand) -> &'static str {
    match command {
        RespCommand::Get(_) => "GET",
        RespCommand::Set(_, _, _) => "SET",
        RespCommand::Del(_) => "DEL",
        RespCommand::Expire(_, _) => "EXPIRE",
        RespCommand::Ttl(_) => "TTL",
        RespCommand::Ping => "PING",
    }
}

/// Connection handler for a single client
///
/// Manages the lifetime of one TCP connection, including:
/// - Reading from socket
/// - Parsing RESP protocol
/// - Executing commands
/// - Writing responses
/// - Error handling
pub struct Connection {
    stream: TcpStream,
    parser: RespParser,
    cache: Arc<CacheStorage>,
    /// Client address, used as context in log output
    peer: String,
}

impl Connection {
    /// Handle a new connection (entry point)
    pub async fn handle(stream: TcpStream, cache: Arc<CacheStorage>) -> anyhow::Result<()> {
        // Resolve the client address for logging; fall back to "unknown" if
        // the peer has already gone away.
        let peer = stream
            .peer_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        // One span per connection groups all of this client's commands into a
        // single trace. `peer` is recorded once and inherited by child spans.
        let span = info_span!("connection", %peer);

        debug!(parent: &span, %peer, "client connected");

        let mut conn = Connection {
            stream,
            parser: RespParser::new(),
            cache,
            peer,
        };

        let result = conn.run().instrument(span).await;
        debug!(peer = %conn.peer, "client disconnected");
        result
    }

    /// Main connection loop
    ///
    /// Process:
    /// 1. Try to parse command from buffer (pipelining!)
    /// 2. If incomplete, read more data from socket
    /// 3. Execute command
    /// 4. Write response
    /// 5. Repeat until disconnect or error
    async fn run(&mut self) -> anyhow::Result<()> {
        let mut buf = [0u8; 4096];

        loop {
            // Try to parse a complete value from the buffer
            match self.parser.parse_value() {
                Ok(value) => {
                    // We have a complete command! Execute it.
                    let response = self.handle_value(value);
                    self.write_response(response).await?;
                    // Continue loop - buffer might have more pipelined commands
                }
                Err(RespError::Incomplete) => {
                    // Need more data from socket
                    let n = self.stream.read(&mut buf).await?;

                    if n == 0 {
                        // Client disconnected
                        return Ok(());
                    }

                    // Feed data to parser
                    self.parser.feed(&buf[..n]);
                    // Loop back to try parsing again
                }
                Err(e) => {
                    // Parse error - send error response and close connection
                    warn!(peer = %self.peer, error = ?e, "protocol parse error, closing connection");
                    let error_response =
                        RespValue::Error(format!("ERR Protocol error: {}", e));
                    let _ = self.write_response(error_response).await;
                    return Err(e.into());
                }
            }
        }
    }

    /// Handle a parsed value (convert to command and execute)
    fn handle_value(&self, value: RespValue) -> RespValue {
        match parse_command(value) {
            Ok(command) => {
                // One span per executed command: this is the unit of work whose
                // latency (span duration) answers "why is GET slow?". The
                // storage call nests inside it, so storage latency is captured
                // without instrumenting the hot path per-op.
                //
                // The span is NAMED by the command type (GET/SET/...) so APM
                // groups latency/throughput per command, not as one und
                // ifferentiated "command" bucket. tracing requires span names
                // to be static literals, hence the match in command_span().
                let span = command_span(&command);
                let _guard = span.enter();

                let cmd_label = command_label(&command);
                debug!(peer = %self.peer, ?command, "executing command");

                let start = Instant::now();
                let response = execute_command(&self.cache, command);
                let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

                // Record latency once per command, tagged by command type so
                // percentiles can be sliced per operation (GET vs SET).
                command_latency().record(elapsed_ms, &[KeyValue::new("command", cmd_label)]);

                // A command that executed but returned a protocol error (rare —
                // execute_command mostly returns data) marks the span failed so
                // it's filterable in APM. A missing GET is NOT an error.
                if let RespValue::Error(msg) = &response {
                    Span::current().set_status(Status::error(msg.clone()));
                }

                debug!(peer = %self.peer, ?response, "command response");
                response
            }
            // Command-parse failures (bad args, unknown command) are real
            // request errors: mark the span so APM can surface failed requests.
            Err(e) => {
                let msg = match &e {
                    CommandError::WrongArgCount => "wrong number of arguments".to_string(),
                    CommandError::InvalidArgument(m) => format!("invalid argument: {m}"),
                    CommandError::UnknownCommand(cmd) => format!("unknown command '{cmd}'"),
                };
                Span::current().set_status(Status::error(msg.clone()));
                debug!(peer = %self.peer, error = %msg, "command rejected");
                RespValue::Error(format!("ERR {msg}"))
            }
        }
    }

    /// Write a response to the client
    async fn write_response(&mut self, value: RespValue) -> anyhow::Result<()> {
        let bytes = serialize(&value);
        self.stream.write_all(&bytes).await?;
        self.stream.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    async fn setup_server() -> (Arc<CacheStorage>, String) {
        let cache = Arc::new(CacheStorage::new(1024 * 1024));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let cache_clone = cache.clone();
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let cache = cache_clone.clone();
                tokio::spawn(async move {
                    let _ = Connection::handle(stream, cache).await;
                });
            }
        });

        (cache, addr.to_string())
    }

    /// Read exactly `len` bytes from the stream and return them as a String.
    ///
    /// A single `read()` is NOT guaranteed to return all available bytes — TCP
    /// may deliver a response in several segments. Tests that assert on the full
    /// response must read until the expected number of bytes has arrived, or
    /// they flake. `read_exact` loops internally until `len` bytes are read.
    ///
    /// The read is bounded by a timeout: if the server sends fewer bytes than
    /// `len` (e.g. the test's expected length is wrong), this fails fast with a
    /// clear message instead of hanging forever waiting for bytes that will
    /// never come.
    async fn read_response(stream: &mut TcpStream, len: usize) -> String {
        let mut buf = vec![0u8; len];
        tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut buf))
            .await
            .expect("timed out reading response — expected length likely wrong")
            .expect("stream closed before full response was read");
        String::from_utf8(buf).unwrap()
    }

    #[tokio::test]
    async fn test_ping() {
        let (_cache, addr) = setup_server().await;
        let mut stream = TcpStream::connect(addr).await.unwrap();

        // Send PING
        stream.write_all(b"*1\r\n$4\r\nPING\r\n").await.unwrap();

        // Read response ("+PONG\r\n" = 7 bytes)
        let response = read_response(&mut stream, 7).await;

        assert_eq!(response, "+PONG\r\n");
    }

    #[tokio::test]
    async fn test_set_and_get() {
        let (_cache, addr) = setup_server().await;
        let mut stream = TcpStream::connect(addr).await.unwrap();

        // Send SET mykey myvalue
        stream
            .write_all(b"*3\r\n$3\r\nSET\r\n$5\r\nmykey\r\n$7\r\nmyvalue\r\n")
            .await
            .unwrap();

        // Read SET response ("+OK\r\n" = 5 bytes)
        let response = read_response(&mut stream, 5).await;
        assert_eq!(response, "+OK\r\n");

        // Send GET mykey
        stream
            .write_all(b"*2\r\n$3\r\nGET\r\n$5\r\nmykey\r\n")
            .await
            .unwrap();

        // Read GET response ("$7\r\nmyvalue\r\n" = 13 bytes)
        let response = read_response(&mut stream, 13).await;
        assert_eq!(response, "$7\r\nmyvalue\r\n");
    }

    #[tokio::test]
    async fn test_get_missing_key() {
        let (_cache, addr) = setup_server().await;
        let mut stream = TcpStream::connect(addr).await.unwrap();

        // Send GET missing
        stream
            .write_all(b"*2\r\n$3\r\nGET\r\n$7\r\nmissing\r\n")
            .await
            .unwrap();

        // Read response (null bulk string "$-1\r\n" = 5 bytes)
        let response = read_response(&mut stream, 5).await;
        assert_eq!(response, "$-1\r\n");
    }

    #[tokio::test]
    async fn test_pipelining() {
        let (_cache, addr) = setup_server().await;
        let mut stream = TcpStream::connect(addr).await.unwrap();

        // Send multiple commands without waiting for responses
        let commands = b"*3\r\n$3\r\nSET\r\n$2\r\nk1\r\n$2\r\nv1\r\n*3\r\n$3\r\nSET\r\n$2\r\nk2\r\n$2\r\nv2\r\n*2\r\n$3\r\nGET\r\n$2\r\nk1\r\n";
        stream.write_all(commands).await.unwrap();

        // Read all three responses, in order, as one contiguous byte stream.
        // SET -> "+OK\r\n" (5), SET -> "+OK\r\n" (5), GET k1 -> "$2\r\nv1\r\n" (8).
        // read_exact loops until all 18 bytes arrive, so a fragmented TCP read
        // can't make this flake.
        let response = read_response(&mut stream, 18).await;

        // Responses must come back in request order.
        assert_eq!(response, "+OK\r\n+OK\r\n$2\r\nv1\r\n");
    }

    #[tokio::test]
    async fn test_invalid_command() {
        let (_cache, addr) = setup_server().await;
        let mut stream = TcpStream::connect(addr).await.unwrap();

        // Send UNKNOWN command
        stream
            .write_all(b"*1\r\n$7\r\nUNKNOWN\r\n")
            .await
            .unwrap();

        // Read error response ("-ERR unknown command 'UNKNOWN'\r\n").
        // Error text length is deterministic, so read exactly that many bytes.
        let expected = "-ERR unknown command 'UNKNOWN'\r\n";
        let response = read_response(&mut stream, expected.len()).await;

        assert_eq!(response, expected);
    }
}
