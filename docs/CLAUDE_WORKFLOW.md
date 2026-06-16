
# Working with Claude Code on FerroCache

## Quick Reference

### Starting a Session
```bash
cd ~/Dev/rust/ferrocache
claude
```

Claude will automatically:
- Load CLAUDE.md for project context
- Apply permissions from .claude/settings.json
- Have access to your project plan and architecture docs

### Common Commands

**Build & Test:**
- "Build the project" → `cargo build`
- "Run tests" → `cargo test`
- "Run clippy" → `cargo clippy`
- "Format code" → `cargo fmt`

**Code Tasks:**
- "Implement the CacheEntry struct with TTL and LRU metadata"
- "Add unit tests for the expiration logic"
- "Refactor the lock granularity to use per-shard locks"
- "Profile this function and suggest optimizations"

**Documentation:**
- "Document this function with examples"
- "Update ARCHITECTURE.md with the decision about DashMap vs RwLock"

**Git Operations:**
- "/commit" - Create a well-formatted commit
- "Show me the diff" - Review changes before committing

### Effective Prompting

**✅ Good Prompts:**
- "Implement LRU eviction that tracks access order with O(1) operations"
- "Add a background Tokio task that reaps expired keys every 60 seconds"
- "Write property-based tests for the RESP parser using quickcheck"
- "Profile the concurrent get/set operations and identify lock contention"

**❌ Avoid:**
- "Write the cache" (too vague)
- "Make it faster" (not specific)
- "Add tests" (what kind? for what?)

### Project Phases

**Currently in: Month 1 (Architecture & Learning)**

When ready to start implementation:
1. "Let's implement the core CacheEntry and Cache trait interface"
2. Claude will reference the architecture docs and create code
3. Iterate on implementation with specific feedback

**Monthly Focus:**
- **Month 1:** Architecture design (current phase)
- **Month 2:** Core implementation (Layers 1-3)
- **Month 3:** Observability and deployment (Layers 5-6)

### Memory & Context

Claude has access to:
- Your project architecture and layer roadmap (CLAUDE.md)
- Detailed architecture docs (docs/ARCHITECTURE.md)
- Your original design outline (referenced in memory)

If Claude seems to forget context:
- Reference specific docs: "Based on the architecture in ARCHITECTURE.md..."
- Point to the plan: "According to Phase 1 of the project plan..."

### Skills & Slash Commands

- `/commit` - Create git commit (auto-formats message)
- `/review` - Review PR before merging
- `/simplify` - Refactor code for quality
- `/help` - Get help with Claude Code

### Tips

1. **Start small:** Implement one component at a time
2. **Test as you go:** Ask for tests immediately after implementation
3. **Benchmark early:** Profile and optimize based on data
4. **Document decisions:** Record key technical decisions in CLAUDE.md
5. **Refer to sources:** "Implement this using the techniques from *Rust Atomics and Locks* chapter 5"

### Example Workflow

```
You: "Let's implement the basic Cache trait and a simple HashMap-backed implementation"

Claude: [Creates trait and implementation]

You: "Add unit tests for get and set operations"

Claude: [Adds tests]

You: "Run the tests"

Claude: [Runs cargo test, shows results]

You: "Now let's add TTL support to the CacheEntry struct"

... continue iterating ...

You: "/commit"

Claude: [Creates commit with proper message]
```

### When to Ask Claude

**Design Questions:**
- "Should we use DashMap or sharded RwLock for this use case?"
- "What's the tradeoff between background reaping and lazy expiration?"
- "How should we handle backpressure in the TCP server?"

**Implementation:**
- "Implement the RESP protocol parser"
- "Add Tokio task for background reaping"
- "Write benchmarks for cache operations"

**Debugging:**
- "This deadlock occurs under load - help me diagnose"
- "The eviction policy isn't working as expected"
- "Profile why set operations are slower than expected"

**Review:**
- "Review this concurrency code for correctness"
- "Check if this RESP implementation matches the spec"
- "Are there any unsafe edge cases in this code?"

## Project-Specific Context

### Technology Stack
- **Runtime:** Tokio for async
- **Data structures:** DashMap or parking_lot (TBD)
- **Protocol:** Redis RESP
- **Logging:** tracing crate
- **Metrics:** Prometheus
- **Deployment:** Kubernetes (StatefulSet)

### Key Design Constraints
1. **Performance first:** This is a cache - latency matters
2. **Read-optimized:** 90%+ read workload expected
3. **Memory-bounded:** Must enforce memory limits
4. **Observable:** Metrics and logging are not optional
5. **Production-grade:** Suitable for portfolio/interviews

### Interview Talking Points
Each layer maps to interview discussions:
- Layer 1-2: Concurrency, data structures, networking
- Layer 3: Systems design, async programming
- Layer 4: Distributed systems, consistency
- Layer 5: Production operations, observability
- Layer 6: DevOps, Kubernetes, containerization
