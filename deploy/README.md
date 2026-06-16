# FerroCache Observability Stack (Elasticsearch + Kibana + OTel Collector)

A single, unified backend for all three observability pillars:

| Pillar  | Viewed in Kibana            |
|---------|-----------------------------|
| Traces  | Observability → APM         |
| Logs    | Discover / Logs             |
| Metrics | Discover / dashboards       |

Data path: **FerroCache → OTLP/gRPC :4317 → OTel Collector → OTLP → APM Server :8200 → Elasticsearch → Kibana APM**.

The OTel Collector forwards to the **Elastic APM Server**, which routes into
APM-native data streams (`traces-apm-*`, `logs-apm-*`, `metrics-apm-*`) that
Kibana's APM UI reads directly. (The collector remains the vendor-neutral hub —
swapping the APM Server for a different backend is a collector-config change.)

> ⚠️ **Local dev only.** Elasticsearch security (TLS/auth) is disabled and a
> single node is used. Do not deploy this configuration.

---

## Prerequisites (Podman)

### 1. Bump `vm.max_map_count` (Elasticsearch requires ≥ 262144)

This kernel setting must be raised in the **Podman VM**, not on macOS itself.

```bash
# macOS / Podman machine:
podman machine ssh 'sudo sysctl -w vm.max_map_count=262144'
```

On Linux hosts (rootful), set it directly: `sudo sysctl -w vm.max_map_count=262144`.
Without this, Elasticsearch exits on startup with a `max_map_count` bootstrap error.

### 2. Give the Podman machine enough RAM

ES (1 GB heap) + Kibana + Collector need headroom. If your machine is small:

```bash
podman machine stop
podman machine set --memory 4096   # 4 GB
podman machine start
```

### 3. `podman compose` support

`podman compose` delegates to an external provider. If it's missing:

```bash
pip install podman-compose         # or: brew install podman-compose
```

(`docker compose` against the Podman socket also works if you prefer.)

---

## Run it

```bash
cd deploy
podman compose up -d

# Watch startup (ES is slowest; wait for it to be healthy):
podman compose ps
podman compose logs -f elasticsearch
```

Endpoints once healthy:
- Elasticsearch: http://localhost:9200
- Kibana:        http://localhost:5601
- OTLP ingest:   localhost:4317 (gRPC), localhost:4318 (HTTP)

---

## Point FerroCache at the collector

FerroCache runs on the **host** and connects to the collector's published 4317.
Either set the env var:

```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 \
RUST_LOG=ferrocache=debug \
cargo run
```

…or put it in `ferrocache.toml`:

```toml
[observability]
log_filter   = "ferrocache=debug"
otlp_endpoint = "http://localhost:4317"
```

Then drive some traffic:

```bash
redis-cli -p 6379 SET foo bar
redis-cli -p 6379 GET foo
redis-cli -p 6379 GET missing
```

---

## Verify data is flowing

1. **Collector received it** — the `debug` exporter prints spans/logs to stdout:
   ```bash
   podman compose logs -f otel-collector
   ```
2. **Elasticsearch indexed it** — data streams should appear:
   ```bash
   curl -s 'http://localhost:9200/_data_stream' | jq '.data_streams[].name'
   ```
3. **Kibana shows it** — open http://localhost:5601 → Observability → APM.
   The service `ferrocache` appears once trace data is in `traces-apm-*`.
   - If APM shows "Add your data", widen the **time picker** (top-right) to
     "Last 24 hours" — the default 15-minute window can miss recent spans if
     there's any clock skew, and APM's service list is built from rollup
     metrics that lag a minute behind ingest.
   - Logs: Discover → data view `logs-apm*`.

### Verify metrics

Cache metrics export on a ~60s interval, so wait about a minute after driving
traffic. To prove a known hit/miss ratio end-to-end:

```bash
# 100 SET, 100 GET(existing)=hits, 50 GET(missing)=misses  → hit rate 66.7%
for i in $(seq 0 99); do redis-cli -p 6379 SET "key$i" val >/dev/null; done
for i in $(seq 0 99); do redis-cli -p 6379 GET "key$i"     >/dev/null; done
for i in $(seq 0 49); do redis-cli -p 6379 GET "nope$i"    >/dev/null; done

sleep 65   # wait for the metric export interval

# Read the latest value of each cache metric from Elasticsearch:
for m in hits misses evictions keys memory.used_bytes; do
  printf '%-22s ' "ferrocache.cache.$m"
  curl -s "http://localhost:9200/metrics-apm*/_search?size=1" \
    -H 'Content-Type: application/json' \
    -d "{\"query\":{\"exists\":{\"field\":\"ferrocache.cache.$m\"}},
         \"sort\":[{\"@timestamp\":\"desc\"}],
         \"_source\":[\"ferrocache.cache.$m\"]}" \
  | jq -c '.hits.hits[0]._source'
done
```

Metrics emitted by FerroCache:

| Metric | Type | Meaning |
|--------|------|---------|
| `ferrocache.cache.hits` | counter | GETs that found a live entry |
| `ferrocache.cache.misses` | counter | GETs that found nothing/expired |
| `ferrocache.cache.evictions` | counter | entries dropped under memory pressure |
| `ferrocache.cache.memory.used_bytes` | gauge | current memory usage |
| `ferrocache.cache.keys` | gauge | current key count |
| `ferrocache.command.duration` | histogram (ms) | per-command latency, tagged by command |

### Visualize metrics in Kibana

The custom `ferrocache.cache.*` metrics aren't in APM's prebuilt views — build
charts with **Lens**:

1. Kibana → ☰ → **Visualize Library** → **Create visualization** → **Lens**
2. Pick the existing APM data view (it already covers `metrics-apm*`)
3. X-axis: `@timestamp` (date histogram). Y-axis:
   - gauges (`...keys`, `...memory.used_bytes`): use **Max**
   - counters (`...hits`, `...misses`): use **Counter rate** / Differences to
     see per-interval rates rather than the ever-climbing total
4. **Save** each chart to a dashboard.

> Drive *sustained* traffic (e.g. a `redis-benchmark` loop) for a few minutes —
> with the 60s export interval, a single burst yields only one data point.

For latency/throughput, the `command.duration` histogram and span timings show
up directly in **Observability → APM → ferrocache**.

---

## Teardown

```bash
podman compose down          # stop containers, KEEP the ES data volume
podman compose down -v       # stop AND delete indexed data (es-data volume)
```

`down` stops and removes the containers but leaves the downloaded images and the
`es-data` volume in place, so the next `up` is fast and your data persists.

---

## Images & disk management

The four service images total ~2.5 GB and **persist on disk even after
`podman compose down`** (they're separate from containers and volumes):

```bash
podman images | grep -E 'elasticsearch|kibana|apm-server|opentelemetry'
#   elasticsearch  8.15.3   ~829 MB
#   kibana         8.15.3   ~1.28 GB
#   apm-server     8.15.3   ~181 MB
#   otel-collector-contrib 0.111.0  ~264 MB
```

To reclaim that space (you'll re-download on the next `up`):

```bash
podman compose down -v                 # first remove containers + data volume
podman rmi docker.elastic.co/elasticsearch/elasticsearch:8.15.3 \
           docker.elastic.co/kibana/kibana:8.15.3 \
           docker.elastic.co/apm/apm-server:8.15.3 \
           otel/opentelemetry-collector-contrib:0.111.0
# or, more broadly:  podman image prune -a
```

> **macOS/Podman caveat:** images and volumes live inside the Podman VM's disk
> image (`~/.local/share/containers/podman/machine/.../*.raw`). `rmi` frees
> space *inside* the VM (reusable by other containers) but does **not** shrink
> the `.raw` file on your Mac — VM disk images don't auto-reclaim freed space.

---

## Notes & gotchas

- **`:Z` volume label** on the collector config mount is for SELinux hosts
  (Fedora/RHEL). Harmless elsewhere; remove if your platform rejects it.
- **Collector image must be `-contrib`** — the `elasticsearch` exporter is not
  in the core collector distribution.
- **All three pillars are wired and verified.** FerroCache emits logs, traces,
  AND metrics over OTLP. Metrics export on a periodic interval (~60s by
  default), so cache metrics appear in `metrics-apm.app.ferrocache-default`
  about a minute after traffic — see "Verify metrics" below.
- **Version pins** (ES/Kibana 8.15.3, collector-contrib 0.111.0) are a
  known-compatible set at time of writing. The `elasticsearch` exporter's
  `mapping.mode` options evolve across collector releases — if you bump the
  collector image and exports fail, check the exporter's docs for that version.
