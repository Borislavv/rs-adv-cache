# Advanced Cache (advCache) - High-Performance HTTP Cache & Reverse Proxy

[![Rust Version](https://img.shields.io/static/v1?label=Rust&message=1.82%2B&logo=rust&color=000000)](https://www.rust-lang.org/tools/install) [![Coverage](https://img.shields.io/codecov/c/github/Borislavv/rs-adv-cache?label=coverage)](https://codecov.io/gh/Borislavv/adv-cache) [![License](https://img.shields.io/badge/License-Apache--2.0-green.svg)](./LICENSE)

**AdvCache** is a high-performance, production-ready in-memory HTTP cache and reverse proxy designed for latency-sensitive workloads. Built with Rust using `tokio` and `axum`, it delivers exceptional throughput (165-300k RPS) with minimal memory overhead and zero-allocation hot paths.

## 🚀 Key Features

### Performance & Scalability
- **High Throughput**: 165k RPS locally, up to 300k RPS on 24-core bare-metal servers
- **Memory Efficient**: Only 3-4GB overhead for 40GB of cached data
- **Sharded Storage**: 1024 shards with per-shard LRU for optimal lock contention reduction

### Advanced Caching
- **TinyLFU Admission Control**: Intelligent cache admission using Count-Min Sketch and Doorkeeper
- **Background Refresh**: Automatic TTL-based cache refresh without blocking requests
- **Flexible Cache Keys**: Configurable query parameters and headers for precise cache control
- **Selective Header Forwarding**: Fine-grained control over cached and forwarded headers

### Production-Ready Features
- **Runtime Control Plane**: Dynamic toggles for admission, eviction, refresh, compression, and tracing
- **Observability**: Prometheus/VictoriaMetrics metrics and OpenTelemetry tracing with minimal overhead
- **Kubernetes Integration**: Health probes, ConfigMap support, and Docker images
- **Graceful Shutdown**: Safe resource cleanup and connection draining

### Developer Experience
- **Comprehensive API**: RESTful endpoints for cache management and monitoring
- **Rich Configuration**: YAML-based configuration with inline documentation
- **Extensive Testing**: Unit tests, integration tests, and end-to-end test coverage
- **OpenAPI Documentation**: Complete API specification via Swagger/OpenAPI

## 📋 Table of Contents

- [Quick Start](#-quick-start)
- [Architecture](#-architecture)
- [Configuration](#-configuration)
- [API Reference](#-api-reference)
- [Performance Tuning](#-performance-tuning)
- [Deployment](#-deployment)
- [Monitoring & Observability](#-monitoring--observability)
- [Development](#-development)
- [License](#-license)

## 🏃 Quick Start

### Prerequisites

- Rust 1.82+ (for building from source)
- Docker (for containerized deployment)
- YAML configuration file

### Installation

#### Using Docker (Recommended)

```bash
# Build the Docker image
docker build -t advcache .

# Run with custom configuration
docker run --rm -p 8020:8020 \
  -v "$PWD/cfg:/app/cfg" \
  -v "$PWD/public/dump:/app/public/dump" \
  advcache -cfg /app/cfg/advcache.cfg.yaml
```

#### Building from Source

```bash
# Clone the repository
git clone https://github.com/Borislavv/adv-cache.git
cd adv-cache

# Build in release mode
cargo build --release

# Run with configuration
./target/release/advcache -cfg ./cfg/advcache.cfg.yaml
```

### Configuration

Create a `cfg/advcache.cfg.yaml` file with the following setup:

```yaml
cache:
  env: "prod"                    # Runtime environment label (e.g., dev/stage/prod). Used for logs/metrics tagging.
  enabled: true                  # Master switch: enables the cache service.

  logs:
    level: "info"                # Log level: debug|info|warn|error. Prefer "info" in prod, "debug" for short bursts.

  runtime:
    num_cpus: 0                  # 0 = auto max available cores. Set explicit N to cap CPU usage.

  api:
    name: "adv_cache"            # Human-readable service name exposed in API/metrics.
    port: "8020"                 # HTTP port for the admin/API endpoints.

  upstream:
    backend:
      id: "mock_upstream"
      enabled: true
      policy: "await"
      host: "localhost:8021"
      scheme: "http"
      rate: 1500                  # Per-backend RPS cap (token bucket or equivalent).
      concurrency: 4096           # Max simultaneous requests.
      timeout: "10s"              # Base timeout for upstream requests.
      max_timeout: "1m"           # Hard cap if “slow path” header allows extending timeouts.
      use_max_timeout_header: ""  # If non-empty, presence of this header lifts timeout to max_timeout.
      healthcheck: "/healthz"     # Liveness probe path; 2xx = healthy.

  # Compression
  # - Supported levels:
  #   CompressNoCompression      = 0
  #   CompressBestSpeed          = 1
  #   CompressBestCompression    = 9
  #   CompressDefaultCompression = 6
  #   CompressHuffmanOnly        = -2
  compression:
    enabled: false
    level: 1

  data:
    dump:
      enabled: false              # Enable periodic dump to disk for warm restarts / backup.
      dump_dir: "public/dump"     # Directory to store dump files.
      dump_name: "cache.dump"     # Base filename (rotations will append indices/timestamps).
      crc32_control_sum: true     # Validate dump integrity via CRC32 on load.
      max_versions: 3             # Keep up to N rotated versions; older are deleted.
      gzip: false                 # Compress dumps with gzip (smaller disk, more CPU).
    mock:
      enabled: false              # If true, prefill cache with mock data (for local testing).
      length: 1000000             # Number of mock entries to generate.

  storage:
    mode: listing                 # Implementation of LRU algo through per-shard lists or Redis style sampling (values=sampling/listing).
    size: 10737418240             # Max memory budget for storage (bytes). Here: 50 GiB.

  admission:
    enabled: true
    capacity: 2000000               # global TinyLFU window size (how many recent events are tracked)
    sample_multiplier: 4            # multiplies the window for aging (larger = slower aging)
    shards: 256                     # number of independent shards (reduces contention)
    min_table_len_per_shard: 65536  # minimum counter slots per shard (larger = fewer collisions)
    door_bits_per_counter: 12       # bits per counter (2^bits-1 max count; larger = less saturation)

  eviction:
    enabled: true                 # Run background evictor to keep memory under configured thresholds.
    replicas: 32                  # Number of evictor workers (>=1). Increase for large heaps / high churn.
    soft_limit: 0.8               # At storage.size × soft_limit start gentle eviction + tighten admission.
    hard_limit: 0.99              # At storage.size × hard_limit trigger minimal hot-path eviction; also set debug memory limit.
    check_interval: "100ms"       # Defines how often the main evictor loop will check the memory limit.

  lifetime:
    enabled: true                 # Enable background refresh/remove for eligible entries.
    ttl: "2h"                     # Default TTL for successful (200) responses unless overridden by rules.
    on_ttl: refresh               # Values: 'remove' or 'refresh'. Defines behavior on TTL overcome.
    beta: 0.35                    # Jitter factor (0..1) to spread refreshes/removes and prevent thundering herd.
    rate: 1000                    # Global refresh/remove QPS cap to upstreams (safety valve).
    replicas: 32                  # Number of workers (>=1).
    coefficient: 0.25             # Start refresh/remove attempts at TTL × coefficient (e.g., 0.5 = at 50% of TTL).

  traces:
    enabled: true
    service_name: "adv_cache"
    service_version: "prod"
    exporter: "http"              # "stdout" | "grpc" (OTLP/4317) | "http" (OTLP/4318)
    endpoint: "localhost:4318"    # ignores when exporter=stdout
    insecure: true                # should use TLS?
    sampling_mode: "always"       # off | always | ratio
    sampling_rate: 1.0            # 10% head-sampling (используется при mode=ratio)
    export_batch_size: 512
    export_batch_timeout: "3s"
    export_max_queue: 1024

  metrics:
    enabled: true                 # Expose Prometheus-style metrics (and/or internal stats if logs.stats=true).

  k8s:
    probe:
      timeout: "5s"               # Liveness/readiness probe timeout for the service endpoints.

  rules:
    /api/v1/user:                 # OnTTL: Will inherit global refresh unless overridden here.
      cache_key:
        query:                    # Include query params by prefix into the cache key (order-insensitive).
          - user[id]
          - timezone
        headers:                  # Include these request headers into the cache key (exact match).
          - Accept-Encoding
      cache_value:
        headers:                  # Response headers to store/forward with cached value.
          - Vary
          - Server
          - Content-Type
          - Content-Length
          - Content-Encoding
          - Cache-Control
          - X-Error-Reason

    /api/v1/client:
      cache_key:
        query:
          - user[id]
          - domain
          - timezone
        headers:
          - Accept-Encoding
      cache_value:
        headers:
          - Vary
          - Server
          - Content-Type
          - Content-Length
          - Content-Encoding
          - Cache-Control
          - X-Error-Reason
```

## 🏗️ Architecture

### Core Components

#### Storage Layer
- **Sharded Map**: 1024 shards for distributed lock contention
- **LRU Implementation**: Doubly-linked list with raw pointers for O(1) operations

#### Admission Control
- **TinyLFU Algorithm**: Frequency-based admission using Count-Min Sketch
- **Doorkeeper**: Short-term frequency filter to prevent one-hit wonders
- **Configurable Sharding**: 256 shards with per-shard frequency tables

#### Background Workers
- **Eviction Worker**: Soft and hard memory limit enforcement with configurable intervals
- **Lifetime Manager**: TTL-based refresh and expiration with beta distribution for load spreading
- **Scalable Workers**: Runtime-adjustable replica count for both workers

### Request Flow

1. **Request Reception**: Axum server receives HTTP request
2. **Cache Key Generation**: Extract and filter query parameters and headers per rule
3. **Cache Lookup**: Sharded map lookup with O(1) complexity
4. **Cache Hit**: Return cached response with stored headers
5. **Cache Miss**: Forward to upstream, cache response, return to client
6. **Background Refresh**: Lifetime manager refreshes expired entries asynchronously

## ⚙️ Configuration

### Configuration File Structure

The configuration file (`advcache.cfg.yaml`) supports the following main sections:

- **`cache`**: Global cache settings (environment, logging, runtime)
- **`api`**: HTTP server configuration (port, name)
- **`upstream`**: Backend configuration (URL, timeouts, health checks, policy)
- **`compression`**: Response compression settings (gzip, brotli)
- **`data`**: Dump/load configuration and mock data settings
- **`storage`**: Cache storage settings (size, mode)
- **`admission`**: TinyLFU admission control parameters
- **`eviction`**: Eviction worker configuration
- **`lifetime`**: TTL and refresh policy settings
- **`traces`**: OpenTelemetry tracing configuration
- **`metrics`**: Prometheus metrics settings
- **`k8s`**: Kubernetes integration (health probes)
- **`rules`**: Path-based cache rules with key/value configuration

## 📡 API Reference

### Main Endpoints

| Endpoint | Method | Description |
|------|--------|-------------|
| `/*` | GET | Main cache/proxy route - serves cached content or proxies to upstream |
| `/k8s/probe` | GET | Kubernetes health probe endpoint |
| `/metrics` | GET | Prometheus/VictoriaMetrics metrics endpoint |

### Cache Control Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/advcache/bypass` | GET | Get bypass status |
| `/advcache/bypass/on` | GET | Enable cache bypass (all requests go to upstream) |
| `/advcache/bypass/off` | GET | Disable cache bypass |
| `/advcache/clear` | GET | Two-step cache clear (returns token) |
| `/advcache/clear?token={token}` | GET | Execute cache clear with token |
| `/advcache/invalidate?_path={path}&{queries}` | GET | Invalidate cache entries matching path and queries |
| `/advcache/invalidate?_path={path}&_remove=true` | GET | Remove cache entries (instead of marking outdated) |
| `/advcache/entry?key={uint64}` | GET | Get cache entry by key |

### Worker Management Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/advcache/eviction` | GET | Get eviction worker status |
| `/advcache/eviction/on` | GET | Enable eviction worker |
| `/advcache/eviction/off` | GET | Disable eviction worker |
| `/advcache/eviction/scale?to={n}` | GET | Scale eviction worker replicas |
| `/advcache/lifetime-manager` | GET | Get lifetime manager status |
| `/advcache/lifetime-manager/on` | GET | Enable lifetime manager |
| `/advcache/lifetime-manager/off` | GET | Disable lifetime manager |
| `/advcache/lifetime-manager/rate?to={n}` | GET | Set refresh rate |
| `/advcache/lifetime-manager/scale?to={n}` | GET | Scale lifetime manager replicas |
| `/advcache/lifetime-manager/policy` | GET | Get refresh policy |
| `/advcache/lifetime-manager/policy/remove` | GET | Set policy to remove on TTL |
| `/advcache/lifetime-manager/policy/refresh` | GET | Set policy to refresh on TTL |

### Configuration & Monitoring

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/advcache/config` | GET | Dump current configuration |
| `/advcache/admission` | GET | Get admission control status |
| `/advcache/admission/on` | GET | Enable admission control |
| `/advcache/admission/off` | GET | Disable admission control |
| `/advcache/upstream/policy` | GET | Get upstream policy (await/deny) |
| `/advcache/upstream/policy/await` | GET | Set upstream policy to await (back-pressure) |
| `/advcache/upstream/policy/deny` | GET | Set upstream policy to deny (fail-fast) |
| `/advcache/http/compression` | GET | Get compression status |
| `/advcache/http/compression/on` | GET | Enable response compression |
| `/advcache/http/compression/off` | GET | Disable response compression |
| `/advcache/traces` | GET | Get tracing status |
| `/advcache/traces/on` | GET | Enable OpenTelemetry tracing |
| `/advcache/traces/off` | GET | Disable OpenTelemetry tracing |

### OpenAPI Documentation

Complete API documentation is available via Swagger/OpenAPI specification at `api/swagger.yaml`.

## 🎯 Performance Tuning

### Throughput Optimization

1. **CPU Cores**: Set `runtime.num_cpus: 0` to use all available cores
2. **Sharding**: Default 1024 shards provide optimal lock contention distribution
3. **Admission Control**: Enable TinyLFU to protect hot cache set
4. **Worker Scaling**: Adjust `eviction.replicas` and `lifetime.replicas` based on load

### Memory Optimization

1. **Storage Size**: Set `storage.size` based on available memory (recommended: 50-80% of total)
2. **Object Pooling**: Enabled by default, no configuration needed
3. **Eviction Limits**: Configure `soft_limit` (0.8) and `hard_limit` (0.99) for memory pressure handling

### Latency Optimization

1. **Compression**: Enable with `level: 1` for minimal CPU overhead
2. **Upstream Timeout**: Set appropriate `timeout` and `max_timeout` values
3. **Connection Pooling**: Configure `concurrency` for upstream connections
4. **Tracing**: Use sampling (`sampling_rate: 0.1`) to reduce overhead

## 🚢 Deployment

### Docker Deployment

```bash
# Build image
docker build -t advcache:latest .

# Run with volume mounts
docker run -d \
  --name advcache \
  -p 8020:8020 \
  -v "$PWD/cfg:/app/cfg:ro" \
  -v "$PWD/public/dump:/app/public/dump" \
  advcache:latest \
  -cfg /app/cfg/advcache.cfg.yaml
```

### Production Considerations

1. **Resource Limits**: Set appropriate CPU and memory limits based on cache size
2. **Health Probes**: Configure liveness and readiness probes for Kubernetes
3. **Graceful Shutdown**: Allow sufficient time for connection draining
4. **Monitoring**: Enable metrics and tracing for production observability
5. **Backup**: Configure cache dump for persistence across restarts

## 📊 Monitoring & Observability

### Prometheus Metrics

Metrics are exposed at `/metrics` endpoint in Prometheus format:

- **Cache Metrics**: Hits, misses, hit ratio, cache size, memory usage
- **Request Metrics**: Request count, latency, status codes
- **Worker Metrics**: Eviction counts, refresh counts, worker status
- **Upstream Metrics**: Upstream requests, errors, timeouts

### OpenTelemetry Tracing

Tracing provides distributed tracing with minimal overhead:

- **Spans**: `ingress` (server), `upstream` (client on miss/proxy), `refresh` (background)
- **Exporters**: stdout (sync), OTLP HTTP/gRPC (batch)
- **Sampling**: Configurable ratio-based sampling (default: 0.1)
- **Fast No-Op**: When disabled, uses atomic toggle only (zero overhead)

### Health Checks

- **Endpoint**: `GET /k8s/probe`
- **Response**: 200 OK when healthy, 503 when unhealthy
- **Timeout**: Configurable via `k8s.probe.timeout` (default: 5s)

### Logging

Structured logging with configurable levels:

- **Format**: JSON (production) or ANSI (development)
- **Levels**: trace, debug, info, warn, error
- **Components**: Component-based filtering for focused debugging

## 🛠️ Development

### Building from Source

```bash
# Clone repository
git clone https://github.com/Borislavv/adv-cache.git
cd adv-cache

# Build in release mode
cargo build --release

# Run with local configuration
./target/release/advcache -cfg ./cfg/advcache.cfg.yaml
```

### Running Tests

```bash
# Run all tests
cargo test

# Run library tests only
cargo test --lib

# Run integration tests
cargo test --test e2e
```

### Project Structure

```
adv-cache/
├── src/
│   ├── app/              # Application initialization and server setup
│   ├── config/           # Configuration parsing and validation
│   ├── controller/       # HTTP request handlers (cache, admin, etc.)
│   ├── db/               # Storage layer (admission, persistence, storage)
│   ├── http/             # HTTP client/server utilities
│   ├── model/            # Cache entry models and serialization
│   ├── upstream/         # Upstream proxy and backend logic
│   ├── workers/          # Background workers (eviction, lifetime)
│   └── tests/            # Integration and end-to-end tests
├── cfg/                  # Configuration files
├── api/                  # OpenAPI/Swagger specification
└── docs/                 # Additional documentation
```

### Benchmark Results

- **Local (4-6 CPU, 1-16KB docs, 20-25GB store)**: 165k RPS steady
- **Bare-metal (24 CPU, 50GB store, production traffic)**: ~300k RPS sustained
- **Memory Overhead**: ~64 bytes per key.

> **Note**: Performance depends on workload characteristics (document size, cache hit ratio, request patterns).

### Performance testing

- **Small requests: 256 bytes up to 1kb; hit rate: 100%**
![1](https://github.com/user-attachments/assets/a9bf0ec2-d4c6-4bf9-8852-df5d14a6ad0b)

- **Medium requests: 1kb up to 8kb; hit rate: 100%**
![2](https://github.com/user-attachments/assets/8deb6dfa-1b5f-4e51-a8a7-0bc8bd116461)

- **Large requests: 1kb up to 16kb; hit rate: 100%**
![3](https://github.com/user-attachments/assets/8253b8de-9610-45f1-87e6-5ed0ec6130f0)

- **Backpreasure: 1kb up to 8kb reqeusts with active eviction; hit rate: ~92%**
![4](https://github.com/user-attachments/assets/1aabffbf-bc61-40f1-9239-794fd852d973)

- **Backpreasure: 1kb up to 8kb reqeusts with active eviction; hit rate: ~58%**
![6](https://github.com/user-attachments/assets/5b99b604-9fd7-47ac-be78-0cc5534b747b)

- **Proxy: req. weight  4kb**
![7](https://github.com/user-attachments/assets/057dad24-8caa-4e50-b7fd-17b4a618757d)

- **Proxy: req. weight 32kb**
![8](https://github.com/user-attachments/assets/ab59b0cc-b5b1-49c3-8f89-d999a73a1c70)


## 📄 License

Licensed under the Apache License, Version 2.0. See [LICENSE](./LICENSE) for details.

## 👤 Maintainer

**Borislav Glazunov**

- Email: glazunov2142@gmail.com
- Telegram: [@gl_c137](https://t.me/gl_c137)

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## 📚 Additional Resources

- [Metrics Documentation](./METRICS.md)
- [API Specification](./api/swagger.yaml)
- [Docker Compose Setup](./docker-compose.yml)

---

**Built with ❤️ using Rust**
