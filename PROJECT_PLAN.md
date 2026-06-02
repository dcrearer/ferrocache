# FerroCache Development Plan

## Month 1: Architecture & Learning (COMPLETE ✅)
- [x] Design system architecture
- [x] Set up Claude Code environment
- [x] Study concurrency patterns and atomics
- [x] Study RESP protocol specification
- [x] Research concurrent data structures (chose `DashMap`)

## Month 2-3: Core Implementation (Layers 1-3) - IN PROGRESS

### Phase 1: Single-Node Cache Engine (COMPLETE ✅)
- [x] Define core cache interface (`CacheStorage`)
- [x] Implement `CacheEntry` struct with value, TTL, access metadata
- [x] Choose and implement storage backend (chose `DashMap`)
- [x] Implement basic `get`, `set`, `delete` operations
- [x] Add TTL support with lazy expiration
- [x] Implement background reaper task for expired keys (`ExpirationReaper`)
- [x] Implement LRU eviction policy with generation counters
- [x] Add memory budgeting with atomic tracking
- [x] Write unit tests for cache operations
- [x] Write unit tests for expiration and eviction
- [x] Benchmark core operations (get/set latency, throughput)

### Phase 2: TCP Server & RESP Protocol (COMPLETE ✅)
- [x] Study Redis RESP protocol specification
- [x] Implement RESP parser with streaming support (`RespParser`)
- [x] Implement RESP serializer (serialize responses)
- [x] Build async TCP server with Tokio
- [x] Implement connection handler with lifecycle management
- [x] Wire up command dispatch to cache engine
- [x] Implement commands: `PING`, `GET`, `SET`, `DEL`
- [x] Implement commands: `EXPIRE`, `TTL`
- [x] Add `set_expiration()` and `get_ttl()` to cache
- [x] Add pipelining support (parse loop)
- [x] Write integration tests with real TCP connections
- [x] Test with `redis-cli` for compatibility
- [x] Add graceful shutdown with Ctrl+C handling
- [x] Create test scripts (`test_server.sh`, `load_test.sh`)

### Phase 3: Concurrency Optimization (COMPLETE ✅)
- [x] Profile lock contention under load
- [x] Optimize read-heavy workload (generation counters, DashMap)
- [x] Use automatic sharding with DashMap (CPU * 4 shards)
- [x] Add stress tests (multiple concurrent clients)
- [x] Benchmark throughput under various concurrency levels
- [x] Fix critical deadlock bug (EXPIRE command)
- [x] Add concurrent benchmarks (`benches/concurrent_ops.rs`)
- [x] Add server throughput benchmarks (`benches/server_throughput.rs`)
- [x] Create performance testing suite

## Month 3: Observability & Deployment (Layers 5-6) - NOT STARTED

### Phase 4: Observability (NEXT PRIORITY 🚧)
- [ ] Add `tracing` instrumentation throughout
- [ ] Implement Prometheus metrics collection
- [ ] Hit/miss rate counter
- [ ] Request latency histogram
- [ ] Eviction counter
- [ ] Memory usage gauge
- [ ] Active connections gauge
- [ ] Add HTTP server for metrics/health endpoints
- [ ] Implement `/health` endpoint (liveness/readiness)
- [ ] Implement `/metrics` endpoint (Prometheus format)
- [ ] Add structured logging with span context
- [ ] Test metrics collection under load

### Phase 5: Deployment & Containerization
- [ ] Write multi-stage Dockerfile
- [ ] Optimize Docker image size
- [ ] Write Kubernetes StatefulSet manifest
- [ ] Write headless Service manifest for discovery
- [ ] Add HPA manifest based on CPU/memory
- [ ] Create Helm chart or Kustomize templates
- [ ] Test local deployment with Minikube/kind
- [ ] Document deployment procedures

### Phase 6: Documentation & Polish (PARTIALLY COMPLETE)
- [x] Create comprehensive study plan (`STUDY_PLAN.md`)
- [x] Document architecture decisions in `CLAUDE.md`
- [x] Add API documentation (commands supported)
- [x] Create performance benchmarking suite
- [ ] Write README with quickstart guide
- [ ] Write deployment guide
- [ ] Document limitations and future work

## Stretch Goals: Distribution (Layer 4) - DEFERRED ⏸️
- [ ] Implement consistent hashing algorithm
- [ ] Add node discovery (static config)
- [ ] Implement client-side routing
- [ ] Test multi-node setup
- [ ] Document distributed operation

**Status:** Single-node implementation complete and stable. Distribution is optional stretch goal.

## Technical Decisions Log

### Data Structure Choice ✅
**Decision:** DashMap with automatic sharding
**Rationale:** 
- Lock-free reads in most cases
- Automatic sharding reduces contention (CPU * 4 shards)
- Simpler than manual sharded RwLock
- Excellent performance in benchmarks

### LRU Implementation ✅
**Decision:** Generation-counter based (not linked list)
**Rationale:**
- Lock-free updates (atomic increment)
- O(1) operations vs O(n) linked list reordering
- ~95% LRU accuracy with sampling sufficient for cache workload
- Massive concurrency improvement

### Value Storage ✅
**Decision:** Use `bytes::Bytes` for values
**Rationale:**
- Reference-counted (like Arc) - cheap clones
- Zero-copy when returning from cache
- Immutable and thread-safe by design

### RESP Protocol Subset ✅
**Decision:** Implement command set: PING, GET, SET, DEL, EXPIRE, TTL
**Rationale:** Sufficient for demo, compatible with redis-cli, manageable scope

### Memory Accounting ✅
**Decision:** Exact accounting with atomic counter
**Rationale:** 
- Atomic operations are fast enough
- Exact accounting prevents memory leaks
- Pre-calculate entry size (struct + value + key)

### Connection Model ✅
**Decision:** One Tokio task per connection
**Rationale:**
- Simple mental model
- Tokio tasks are cheap (~2KB each)
- Natural backpressure via OS connection limits
- Scales to thousands of connections (sufficient for cache)

### Error Handling ✅
**Decision:** Close connection on protocol errors, keep open on command errors
**Rationale:**
- Protocol errors corrupt parser state - can't recover
- Command errors are user errors - connection still valid
- Matches Redis behavior

## Project Status (2026-05-30)

**Overall Progress:** ~70% complete

**Completed:**
- ✅ Single-node cache engine (Layer 1)
- ✅ RESP protocol implementation (Layer 2)
- ✅ Async TCP server (Layer 3)
- ✅ Concurrency optimization (Layer 3)
- ✅ Performance benchmarking suite
- ✅ Comprehensive testing (82 tests passing)

**Next:**
- 🚧 Observability layer (metrics, tracing, health checks)

**Future:**
- ⏸️ Deployment (Docker, Kubernetes)
- ⏸️ Distribution (optional)
