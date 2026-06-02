# FerroCache

A distributed, in-memory key-value cache built in Rust. Redis-like functionality with focus on concurrency, networking, and systems design.

## Project Status

**Current Phase:** Month 1 - Architecture & Design  
**Timeline:** 3-month project  
**Goal:** Production-grade distributed cache suitable for portfolio and technical interviews

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

## Commands (Planned)

- `GET key` - Retrieve value
- `SET key value [EX seconds]` - Set value with optional TTL
- `DEL key` - Delete key
- `EXPIRE key seconds` - Set TTL
- `TTL key` - Get remaining TTL
- `PING` - Health check

## Development Timeline

- **Month 1:** Architecture design + learning
- **Month 2:** Core implementation (Layers 1-3)
- **Month 3:** Observability + deployment (Layers 5-6)

See [PROJECT_PLAN.md](PROJECT_PLAN.md) for detailed roadmap.

## Technology Stack

- **Language:** Rust (2024 edition)
- **Async Runtime:** Tokio
- **Concurrency:** DashMap or parking_lot (TBD)
- **Protocol:** Redis RESP
- **Logging:** tracing
- **Metrics:** Prometheus
- **Deployment:** Kubernetes

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

- [CLAUDE.md](CLAUDE.md) - Project context for Claude Code
- [ARCHITECTURE.md](docs/ARCHITECTURE.md) - Detailed technical design
- [PROJECT_PLAN.md](PROJECT_PLAN.md) - Development roadmap
- [STUDY_PLAN.md](STUDY_PLAN.md) - Interactive learning guide (3-4 hours)
- [CLAUDE_WORKFLOW.md](docs/CLAUDE_WORKFLOW.md) - Working with Claude Code

## License

[TBD]

## Author

Built as a learning project to demonstrate Rust systems programming, distributed systems design, and production engineering practices.
