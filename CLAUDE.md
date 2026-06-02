# FerroCache - Distributed Cache System

## Project Overview
FerroCache is a distributed, in-memory key-value cache (Redis-like) built in Rust to demonstrate concurrency, networking, and distributed systems thinking.

## Architecture & Goals

### ✅ Layer 1: Single-Node Cache Engine (Foundation) - COMPLETE
- ✅ Concurrent hash map (`DashMap`) for `String → Bytes` storage
- ✅ TTL/expiration with background reaper task + lazy expiration on access
- ✅ LRU eviction policy with generation-counter tracking (lock-free)
- ✅ Memory budgeting by bytes with atomic tracking

**Implemented:**
- `CacheStorage` with DashMap for concurrent access
- `LruTracker` with lock-free generation counters
- `CacheEntry` with Bytes for zero-copy value cloning
- `ExpirationReaper` background task (Tokio)
- Memory tracking with eviction on limit

### ✅ Layer 2: Protocol & API - COMPLETE
- ✅ Wire protocol: Redis RESP protocol for client compatibility
- ✅ Commands: `GET`, `SET`, `DEL`, `EXPIRE`, `TTL`, `PING`
- ✅ Async TCP server with Tokio (one task per connection)
- ✅ Pipelining support for multiple commands without round-tripping

**Implemented:**
- `RespParser` with streaming support and buffering
- `RespSerializer` for response encoding
- Command parsing (RESP → typed enums)
- Command execution against cache
- Connection handler with full lifecycle management
- Integration tests with real TCP connections

### ✅ Layer 3: Concurrency Model - COMPLETE
- ✅ Per-shard locks with DashMap (lock-free reads)
- ✅ Read-heavy optimization (generation counters, cheap clones)
- ✅ Background tasks: expiration reaper, graceful shutdown

**Implemented:**
- DashMap automatic sharding (CPU * 4 shards)
- Atomic operations for generation tracking
- Lock-free LRU updates
- Graceful shutdown with broadcast channels
- Connection-level concurrency with Tokio tasks

### ⏸️ Layer 4: Distribution (Future Work)
- Consistent hashing for key-to-node mapping
- Node discovery (static config initially, gossip protocol later)
- Start with partitioning only (replication is future enhancement)
- Graceful failure handling (cache misses acceptable, cluster stays healthy)

**Status:** Deferred - single-node implementation complete

### 🚧 Layer 5: Observability (Next Priority)
- Prometheus metrics: hit/miss rate, latency percentiles, evictions, memory, connections
- Structured logging with `tracing` crate and span context
- HTTP `/health` and `/metrics` endpoints alongside cache protocol

**Status:** Not started - ready to implement

### ⏸️ Layer 6: Deployment (Future Work)
- Multi-stage Dockerfile with minimal final image
- Kubernetes manifests: StatefulSet, headless Service, HPA
- Helm chart or Kustomize for environment templating

**Status:** Not started - waiting for observability layer

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

### ✅ Generation-Counter LRU (Not Linked List)
**Decision:** Use atomic counters instead of traditional linked-list LRU
**Rationale:** 
- Linked lists require global lock for reordering
- Generation counters are lock-free (atomic increment)
- Sampling gives ~95% LRU accuracy with O(1) operations
- Massive concurrency win for read-heavy workloads

### ✅ DashMap Over RwLock<HashMap>
**Decision:** Use DashMap for automatic sharding
**Rationale:**
- Automatic sharding reduces lock contention
- Lock-free reads in most cases
- Scales to multiple cores naturally
- Less code than manual sharding

### ✅ Bytes for Value Storage
**Decision:** Use `bytes::Bytes` instead of `Vec<u8>`
**Rationale:**
- Reference-counted (like Arc) - cheap clones
- Zero-copy when returning values from cache
- Immutable by design (thread-safe)

### ✅ One Task Per Connection
**Decision:** Spawn one Tokio task per client connection
**Rationale:**
- Simple mental model (stateful connections)
- Natural backpressure (OS limits connections)
- Tokio tasks are cheap (~2KB each)
- Good enough for 1000s of connections (cache use case)
- Over-engineering: Task pool only needed for 100k+ connections

### ✅ Close Connection on Parse Errors
**Decision:** Protocol errors close the connection, command errors don't
**Rationale:**
- Malformed RESP = corrupted parser state
- Can't reliably recover from protocol errors
- Command errors (wrong args, unknown cmd) are user errors - recoverable

### ✅ Two-Step Command Parsing
**Decision:** Parse bytes → RespValue → RespCommand (not direct)
**Rationale:**
- Protocol parser decoupled from command set
- Easy to add new commands
- Can inspect raw protocol for debugging
- Small allocation overhead acceptable

## Development Timeline
- **Month 1** (Complete): Architecture design and learning
- **Month 2-3** (In Progress): 
  - ✅ Layers 1-3 complete (cache engine, protocol, TCP server)
  - 🚧 Layer 5 next (observability)
  - ⏸️ Layer 6 after (deployment)
  - ⏸️ Layer 4 optional (distribution)

**Current Status (2026-05-30):** ~70% complete, production-ready single-node cache

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
