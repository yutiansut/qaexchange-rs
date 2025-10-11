#!/bin/bash
# Workaround script to run OLTP storage benchmarks

set -e

echo "=== Building qaexchange in dev mode ==="
cargo build --lib

echo ""
echo "=== Building benchmark binary (may take 2-3 minutes) ==="
RUSTFLAGS="-C opt-level=2" cargo build --bench oltp_storage_bench --profile=dev

echo ""
echo "=== Running OLTP Storage Benchmarks ==="
./target/debug/deps/oltp_storage_bench-*[!\.][!d]

echo ""
echo "✅ Benchmarks completed"
