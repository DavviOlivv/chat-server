#!/usr/bin/env bash
set -euo pipefail

# Usage: ./scripts/run-stress.sh [N] [ITER]
# N: number of concurrent users (default 50)
# ITER: iterations per worker (default 1000)

N=${1:-50}
ITER=${2:-1000}

export STRESS_N=$N
export STRESS_ITER=$ITER
# number of test threads (optional override)
export RUST_TEST_THREADS=${RUST_TEST_THREADS:-8}

echo "Running stress test with STRESS_N=$STRESS_N STRESS_ITER=$STRESS_ITER RUST_TEST_THREADS=$RUST_TEST_THREADS"

# Run the ignored stress test
make stress-test
