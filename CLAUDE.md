# FerroCache - Distributed Cache System

## Project Overview
FerroCache is a distributed, in-memory key-value cache (Redis-like) built in Rust to demonstrate concurrency, networking, and distributed systems thinking.

## Architecture & Goals

### Layer 1: Single-Node Cache Engine (Foundation)
- Concurrent hash map (`DashMap` or sharded `RwLock<HashMap>`) for `String → Bytes` storage
- TTL/expiration with background reaper task + lazy expiration on access
- LRU eviction policy with access order tracking
- Memory budgeting by bytes (not just key count) with actual memory accounting

### Layer 2: Protocol & API
- Wire protocol: Subset of Redis RESP protocol for client compatibility
- Commands: `GET`, `SET`, `DEL`, `EXPIRE`, `TTL`, `PING`
- Async TCP server with Tokio (one task per connection)
- Pipelining support for multiple commands without round-tripping

### Layer 3: Concurrency Model
- Per-shard locks vs global lock (ties to *Rust Atomics and Locks*)
- Read-heavy optimization (caches are read-dominant)
- Background tasks: expiration reaper, stats collection, health checks

### Layer 4: Distribution (Months 2-3 Stretch)
- Consistent hashing for key-to-node mapping
- Node discovery (static config initially, gossip protocol later)
- Start with partitioning only (replication is future enhancement)
- Graceful failure handling (cache misses acceptable, cluster stays healthy)

### Layer 5: Observability (Critical for Production)
- Prometheus metrics: hit/miss rate, latency percentiles, evictions, memory, connections
- Structured logging with `tracing` crate and span context
- HTTP `/health` and `/metrics` endpoints alongside cache protocol

### Layer 6: Deployment (Leverages CKAD Background)
- Multi-stage Dockerfile with minimal final image
- Kubernetes manifests: StatefulSet, headless Service, HPA
- Helm chart or Kustomize for environment templating

## Development Guidelines

### Code Style
- Follow Rust idioms and conventions
- Prefer explicit error handling over `.unwrap()`
- Use `Result<T, E>` for operations that can fail
- Document public APIs with doc comments

### Testing Strategy
- Write unit tests for core cache operations
- Include integration tests for distributed scenarios
- Benchmark critical paths (get, set, eviction)
- Test network partition scenarios

### Performance Considerations
- This is a cache - performance is critical
- Profile before optimizing
- Consider lock-free data structures where appropriate
- Monitor memory usage patterns

## Scope Control - Deferred Features
Keep on "future work" list in README, but don't build:
- Persistence/snapshots (AOF, RDB)
- Pub/sub
- Lua scripting
- Full cluster consensus (Raft)
- Authentication/TLS

Mentioning these shows understanding of full problem space without over-scoping.

## Key Technical Decisions
[Document architectural decisions as they're made]

## Development Timeline
- **Month 1**: Architecture design while reading GPU/DDIA chapters
- **Month 2**: Build Layers 1-3 (single-node working cache with async TCP)
- **Month 3**: Add Layer 5 observability, Layer 6 deployment, optionally start Layer 4

## Build & Test Commands
```bash
# Build
cargo build --release

# Run tests
cargo test

# Run benchmarks
cargo bench

# Check code
cargo clippy -- -D warnings
cargo fmt --check
```

## Dependencies & Rationale
Anticipated core dependencies:
- `tokio` - Async runtime for TCP server and background tasks
- `dashmap` or `parking_lot` - Concurrent data structures
- `bytes` - Efficient byte buffer management
- `tracing` - Structured logging with span context
- `prometheus` - Metrics collection
- `serde` - Serialization for RESP protocol

[Document final selections and rationale as they're added]

## Resources
- Redis protocol specification (if implementing Redis compatibility)
- Consistent hashing algorithms
- Raft/Paxos for consensus (if needed)
