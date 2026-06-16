# FerroCache

A distributed, in-memory key-value cache built in Rust. Redis-like functionality with focus on concurrency, networking, and systems design.

## Project Status

**Current State:** ~85% complete — production-ready single-node cache with full observability  
**Goal:** Production-grade distributed cache suitable for portfolio and technical interviews

| Layer | Status |
|-------|--------|
| 1. Single-Node Cache Engine | ✅ Complete |
| 2. Protocol & API (RESP) | ✅ Complete |
| 3. Concurrency Model | ✅ Complete |
| 4. Distribution | ⏸️ Deferred (optional) |
| 5. Observability (logs + traces + metrics over OTLP) | ✅ Complete |
| 6. Deployment (Kubernetes) | ⏭️ Next |

## Quick Start

```bash
# Build
cargo build

# Run tests
cargo test

# Run with clippy
cargo clippy

# Format code
cargo fmt
```

## Architecture

FerroCache is built in six layers:

1. **Single-Node Cache Engine** - Concurrent hashmap with LRU eviction and TTL
2. **Protocol & API** - Redis RESP protocol over async TCP (Tokio)
3. **Concurrency Model** - Optimized for read-heavy workloads
4. **Distribution** - Consistent hashing and node discovery (stretch goal)
5. **Observability** - Prometheus metrics, structured logging, health endpoints
6. **Deployment** - Kubernetes manifests (StatefulSet, Service, HPA)

See [ARCHITECTURE.md](docs/ARCHITECTURE.md) for detailed design.

## Commands (Implemented)

- `GET key` - Retrieve value
- `SET key value [EX seconds]` - Set value with optional TTL
- `DEL key` - Delete key
- `EXPIRE key seconds` - Set TTL
- `TTL key` - Get remaining TTL
- `PING` - Health check

Connect with any Redis client: `redis-cli -p 6379 PING`.

## Observability

All three pillars export over OpenTelemetry (OTLP) to a collector. A ready-to-run
Elasticsearch + Kibana + APM Server stack lives in [`deploy/`](deploy/).

```bash
# point FerroCache at a collector (config file or env var)
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 cargo run
```

See [deploy/README.md](deploy/README.md) for the full stack and verification steps.

## Technology Stack

- **Language:** Rust (2021 edition)
- **Async Runtime:** Tokio
- **Concurrency:** DashMap (sharded lock-free map)
- **Protocol:** Redis RESP
- **Observability:** tracing + OpenTelemetry (OTLP) → Elasticsearch/Kibana
- **Deployment:** Kubernetes (planned)

## Project Goals

This project demonstrates:
- ✅ Rust concurrency patterns and atomics
- ✅ Async networking with Tokio
- ✅ Distributed systems design
- ✅ Production observability practices
- ✅ Kubernetes deployment patterns

## Future Enhancements (Explicitly Out of Scope)

- Persistence / snapshots (AOF, RDB)
- Pub/sub messaging
- Lua scripting
- Full cluster consensus (Raft)
- Authentication / TLS

These are documented to show understanding of the full problem space.

## Documentation

- [CLAUDE.md](CLAUDE.md) - Project context and layer roadmap
- [ARCHITECTURE.md](docs/ARCHITECTURE.md) - Detailed technical design
- [STUDY_PLAN.md](STUDY_PLAN.md) - Interactive learning guide
- [deploy/README.md](deploy/README.md) - Observability stack (Elasticsearch + Kibana + OTLP)
- [CLAUDE_WORKFLOW.md](docs/CLAUDE_WORKFLOW.md) - Working with Claude Code

## License

[TBD]

## Author

Built as a learning project to demonstrate Rust systems programming, distributed systems design, and production engineering practices.
