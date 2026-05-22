# FerroCache Development Plan

## Month 1: Architecture & Learning (Current)
- [x] Design system architecture
- [x] Set up Claude Code environment
- [ ] Complete *Rust Atomics and Locks* chapters on concurrency
- [ ] Study RESP protocol specification
- [ ] Research concurrent data structures (`DashMap` vs sharded locks)

## Month 2: Core Implementation (Layers 1-3)

### Phase 1: Single-Node Cache Engine
- [ ] Define core `Cache` trait interface
- [ ] Implement `CacheEntry` struct with value, TTL, access metadata
- [ ] Choose and implement storage backend (`DashMap` or `RwLock<HashMap>`)
- [ ] Implement basic `get`, `set`, `delete` operations
- [ ] Add TTL support with lazy expiration
- [ ] Implement background reaper task for expired keys
- [ ] Implement LRU eviction policy with memory budgeting
- [ ] Write unit tests for cache operations
- [ ] Write unit tests for expiration and eviction
- [ ] Benchmark core operations (get/set latency, throughput)

### Phase 2: TCP Server & RESP Protocol
- [ ] Study Redis RESP protocol specification
- [ ] Implement RESP parser (deserialize commands)
- [ ] Implement RESP serializer (serialize responses)
- [ ] Build async TCP server with Tokio
- [ ] Wire up command dispatch to cache engine
- [ ] Implement commands: `PING`, `GET`, `SET`, `DEL`
- [ ] Implement commands: `EXPIRE`, `TTL`
- [ ] Add pipelining support
- [ ] Write integration tests with real TCP connections
- [ ] Test with `redis-cli` for compatibility

### Phase 3: Concurrency Optimization
- [ ] Profile lock contention under load
- [ ] Optimize read-heavy workload (consider lock-free reads)
- [ ] Fine-tune shard count if using sharded locks
- [ ] Add stress tests (multiple concurrent clients)
- [ ] Benchmark throughput under various concurrency levels

## Month 3: Observability & Deployment (Layers 5-6)

### Phase 4: Observability
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

### Phase 6: Documentation & Polish
- [ ] Write README with quickstart guide
- [ ] Document architecture decisions
- [ ] Add API documentation (commands supported)
- [ ] Write deployment guide
- [ ] Create performance benchmarking suite
- [ ] Document limitations and future work

## Stretch Goals: Distribution (Layer 4) - If Time Permits
- [ ] Implement consistent hashing algorithm
- [ ] Add node discovery (static config)
- [ ] Implement client-side routing
- [ ] Test multi-node setup
- [ ] Document distributed operation

## Technical Decisions Log

### Data Structure Choice
**Decision:** [TBD - DashMap vs sharded RwLock]
**Rationale:** Will be determined after benchmarking and profiling

### RESP Protocol Subset
**Decision:** Implement minimal command set: GET, SET, DEL, EXPIRE, TTL, PING
**Rationale:** Sufficient for demo, compatible with redis-cli, manageable scope

### Memory Accounting
**Decision:** [TBD - Exact vs approximate accounting]
**Rationale:** Balance between accuracy and performance overhead
