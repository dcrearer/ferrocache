#!/bin/bash
# FerroCache Load Testing Suite

echo "======================================"
echo "FerroCache Performance Test Suite"
echo "======================================"
echo ""

if ! command -v redis-benchmark &> /dev/null; then
    echo "ERROR: redis-benchmark not found. Install redis-tools first."
    exit 1
fi

# Check if server is running
if ! redis-cli -p 6379 PING &> /dev/null; then
    echo "ERROR: FerroCache server not running on port 6379"
    echo "Start it with: cargo run --release"
    exit 1
fi

echo "Server is running. Starting tests..."
echo ""

# Test 1: Basic throughput
echo "==== Test 1: Basic Throughput (100k requests) ===="
redis-benchmark -p 6379 -t set,get -n 100000 -q
echo ""

# Test 2: Concurrent connections
echo "==== Test 2: Concurrent Connections (100 clients) ===="
redis-benchmark -p 6379 -t set,get -n 100000 -c 100 -q
echo ""

# Test 3: Pipelining
echo "==== Test 3: Pipelining (16 commands per batch) ===="
redis-benchmark -p 6379 -t set,get -n 100000 -c 50 -P 16 -q
echo ""

# Test 4: Large values
echo "==== Test 4: Large Values (1KB) ===="
redis-benchmark -p 6379 -t set,get -n 50000 -c 50 -d 1024 -q
echo ""

# Test 5: Mixed workload (only supported commands)
echo "==== Test 5: Mixed Workload (SET/GET/DEL) ===="
redis-benchmark -p 6379 -t set,get,del -n 50000 -c 50 -q
echo ""

# Test 6: Latency test
echo "==== Test 6: Latency Distribution ===="
echo "GET latency:"
redis-benchmark -p 6379 -t get -n 10000 -c 1 --csv | tail -1
echo ""
echo "SET latency:"
redis-benchmark -p 6379 -t set -n 10000 -c 1 --csv | tail -1
echo ""

echo "======================================"
echo "Load testing complete!"
echo "======================================"
