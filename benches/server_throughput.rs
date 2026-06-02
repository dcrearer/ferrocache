use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Connect to FerroCache server and send a command
async fn send_command(addr: &str, command: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(command).await.unwrap();

    let mut response = vec![0u8; 1024];
    let n = stream.read(&mut response).await.unwrap();
    response.truncate(n);
    response
}

/// Benchmark SET operations
fn bench_set(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("server_set");
    group.throughput(Throughput::Elements(1));

    group.bench_function("set_operation", |b| {
        b.to_async(&rt).iter(|| async {
            let cmd = b"*3\r\n$3\r\nSET\r\n$8\r\nbenchkey\r\n$10\r\nbenchvalue\r\n";
            black_box(send_command("127.0.0.1:6379", cmd).await);
        });
    });

    group.finish();
}

/// Benchmark GET operations
fn bench_get(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Setup: insert a key first
    rt.block_on(async {
        let cmd = b"*3\r\n$3\r\nSET\r\n$8\r\nbenchkey\r\n$10\r\nbenchvalue\r\n";
        send_command("127.0.0.1:6379", cmd).await;
    });

    let mut group = c.benchmark_group("server_get");
    group.throughput(Throughput::Elements(1));

    group.bench_function("get_operation", |b| {
        b.to_async(&rt).iter(|| async {
            let cmd = b"*2\r\n$3\r\nGET\r\n$8\r\nbenchkey\r\n";
            black_box(send_command("127.0.0.1:6379", cmd).await);
        });
    });

    group.finish();
}

/// Benchmark pipelined operations
fn bench_pipeline(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("server_pipeline");
    group.throughput(Throughput::Elements(10));

    group.bench_function("pipeline_10_sets", |b| {
        b.to_async(&rt).iter(|| async {
            let mut stream = TcpStream::connect("127.0.0.1:6379").await.unwrap();

            // Send 10 SET commands without waiting
            for i in 0..10 {
                let cmd = format!(
                    "*3\r\n$3\r\nSET\r\n$5\r\nkey{}\r\n$5\r\nval{}\r\n",
                    i, i
                );
                stream.write_all(cmd.as_bytes()).await.unwrap();
            }

            // Read all 10 responses
            let mut buf = vec![0u8; 4096];
            let n = stream.read(&mut buf).await.unwrap();
            black_box(n);
        });
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(10))
        .sample_size(100);
    targets = bench_set, bench_get, bench_pipeline
}
criterion_main!(benches);
