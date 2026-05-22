# FerroCache Architecture

## System Components

### Layer 1: Core Cache Engine
**Data Structure:**
- Concurrent hash map using `DashMap` or sharded `RwLock<HashMap<String, CacheEntry>>`
- `CacheEntry` contains: value (`Bytes`), TTL, access metadata for LRU

**Expiration Strategy:**
- Background reaper task (Tokio task) that periodically scans for expired keys
- Lazy expiration on `GET` operations (check TTL before returning)
- Trade-off: reaper frequency vs. memory pressure

**Eviction Policy:**
- LRU (Least Recently Used) implementation
- Requires access order tracking - consider doubly-linked list or timestamp-based approach
- Data structure implications: O(1) get/set requires careful design

**Memory Budgeting:**
- Track actual bytes used, not just key count
- `size_of::<String>() + value.len()` accounting
- Trigger eviction when approaching memory limit

### Layer 2: Protocol & API
**Wire Protocol:**
- Implement subset of Redis RESP (REdis Serialization Protocol)
- Benefits: Free compatibility with existing Redis clients
- Commands: `GET`, `SET`, `DEL`, `EXPIRE`, `TTL`, `PING`

**TCP Server:**
- Async with Tokio runtime
- One task per connection for isolation
- Parse incoming bytes → dispatch to cache engine → serialize response

**Pipelining:**
- Handle multiple commands per connection without waiting for responses
- Buffer management and backpressure handling

### Layer 3: Concurrency Model
**Lock Granularity:**
- Per-shard locks (better concurrency) vs. global lock (simpler)
- Read-heavy workload optimization (caches are 90%+ reads typically)
- Consider `RwLock` for read-write separation or lock-free structures

**Background Tasks:**
- Expiration reaper (periodic sweep)
- Stats collection (aggregating metrics)
- Health checks (liveness/readiness)

### Layer 4: Distribution (Phase 2)
**Consistent Hashing:**
- Map keys to nodes using hash ring
- Minimize reshuffling when nodes join/leave
- Virtual nodes for better distribution

**Node Discovery:**
- Static configuration (initial implementation)
- Gossip protocol (future: Memberlist-style)

**Partitioning vs. Replication:**
- Start with partitioning only (simpler, scales capacity)
- Replication for availability (future enhancement)

**Failure Handling:**
- Cache misses are acceptable (it's a cache, not a database)
- Cluster health maintained even with node failures
- Client retry logic and timeout handling

### Layer 5: Observability
**Prometheus Metrics:**
- Hit rate / miss rate (cache effectiveness)
- Latency percentiles (p50, p90, p99)
- Eviction count and rate
- Memory usage (current / max)
- Active connections count

**Structured Logging:**
- `tracing` crate with span context
- Follow request through system with trace IDs
- Log levels: ERROR, WARN, INFO, DEBUG, TRACE

**Health Endpoints:**
- HTTP server alongside cache protocol
- `/health` - liveness/readiness checks
- `/metrics` - Prometheus scrape endpoint

### Layer 6: Deployment
**Containerization:**
- Multi-stage Dockerfile:
  - Stage 1: `rust:latest` for building
  - Stage 2: Minimal runtime (distroless or alpine)
- Build optimizations: cargo cache layers

**Kubernetes:**
- StatefulSet for stable network identities
- Headless Service for node discovery
- HPA (Horizontal Pod Autoscaler) based on CPU/memory
- Resource requests/limits tuned to cache size

## Data Flow
1. Client connects via TCP to cache node
2. Client sends RESP-encoded command
3. Server parses command and validates
4. Cache engine executes operation (with locking)
5. Background tasks handle expiration/eviction
6. Response serialized and sent back to client

## Concurrency Patterns
- Read path: Optimized for concurrent reads (RwLock or lock-free)
- Write path: Serialized per-shard or per-key
- Background tasks: Separate Tokio tasks with independent scheduling

## Failure Scenarios
- Node failure: Clients reconnect, keys on failed node result in cache miss
- Network partition: Clients timeout and retry
- Memory pressure: Eviction policy kicks in, oldest entries removed
- Slow client: Backpressure applied, connection may be dropped
