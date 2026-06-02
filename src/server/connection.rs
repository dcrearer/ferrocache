use crate::cache::storage::CacheStorage;
use crate::commands::{parse_command, CommandError};
use crate::commands::handlers::execute_command;
use crate::protocol::parser::{RespError, RespParser};
use crate::protocol::serializer::serialize;
use crate::protocol::RespValue;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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
}

impl Connection {
    /// Handle a new connection (entry point)
    pub async fn handle(stream: TcpStream, cache: Arc<CacheStorage>) -> anyhow::Result<()> {
        let mut conn = Connection {
            stream,
            parser: RespParser::new(),
            cache,
        };

        conn.run().await
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
                    eprintln!("Parse error: {:?}", e);
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
            Ok(command) => execute_command(&self.cache, command),
            Err(CommandError::WrongArgCount) => {
                RespValue::Error("ERR wrong number of arguments".to_string())
            }
            Err(CommandError::InvalidArgument(msg)) => {
                RespValue::Error(format!("ERR invalid argument: {}", msg))
            }
            Err(CommandError::UnknownCommand(cmd)) => {
                RespValue::Error(format!("ERR unknown command '{}'", cmd))
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

    #[tokio::test]
    async fn test_ping() {
        let (_cache, addr) = setup_server().await;
        let mut stream = TcpStream::connect(addr).await.unwrap();

        // Send PING
        stream.write_all(b"*1\r\n$4\r\nPING\r\n").await.unwrap();

        // Read response
        let mut buf = [0u8; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        let response = std::str::from_utf8(&buf[..n]).unwrap();

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

        // Read SET response
        let mut buf = [0u8; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"+OK\r\n");

        // Send GET mykey
        stream
            .write_all(b"*2\r\n$3\r\nGET\r\n$5\r\nmykey\r\n")
            .await
            .unwrap();

        // Read GET response
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"$7\r\nmyvalue\r\n");
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

        // Read response (should be null bulk string)
        let mut buf = [0u8; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"$-1\r\n");
    }

    #[tokio::test]
    async fn test_pipelining() {
        let (_cache, addr) = setup_server().await;
        let mut stream = TcpStream::connect(addr).await.unwrap();

        // Send multiple commands without waiting for responses
        let commands = b"*3\r\n$3\r\nSET\r\n$2\r\nk1\r\n$2\r\nv1\r\n*3\r\n$3\r\nSET\r\n$2\r\nk2\r\n$2\r\nv2\r\n*2\r\n$3\r\nGET\r\n$2\r\nk1\r\n";
        stream.write_all(commands).await.unwrap();

        // Read all responses
        let mut buf = [0u8; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        let response = std::str::from_utf8(&buf[..n]).unwrap();

        // Should get +OK, +OK, $2\r\nv1\r\n
        assert!(response.contains("+OK"));
        assert!(response.contains("$2\r\nv1\r\n"));
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

        // Read error response
        let mut buf = [0u8; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        let response = std::str::from_utf8(&buf[..n]).unwrap();

        assert!(response.starts_with("-ERR"));
    }
}
