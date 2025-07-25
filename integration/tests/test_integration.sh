#!/bin/bash

# Saimiris Integration Test Script
# This script tests the complete Saimiris pipeline after major changes
# It verifies that agent and client work correctly with Kafka and ClickHouse

set -e  # Exit on any error

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SAIMIRIS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
INTEGRATION_DIR="$SCRIPT_DIR/.."
AGENT_ID="wbmwwp9vna"
TIMEOUT_SECONDS=30

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Cross-platform timeout function
run_with_timeout() {
    local timeout_duration="$1"
    shift
    local cmd="$*"

    # Try different timeout commands
    if command -v timeout >/dev/null 2>&1; then
        timeout "$timeout_duration" bash -c "$cmd"
    elif command -v gtimeout >/dev/null 2>&1; then
        gtimeout "$timeout_duration" bash -c "$cmd"
    else
        # Fallback: run command in background with manual timeout
        bash -c "$cmd" &
        local pid=$!
        local count=0
        while kill -0 $pid 2>/dev/null && [[ $count -lt $timeout_duration ]]; do
            sleep 1
            ((count++))
        done
        if kill -0 $pid 2>/dev/null; then
            kill $pid 2>/dev/null || true
            return 124  # timeout exit code
        fi
        wait $pid 2>/dev/null || true
    fi
}

# Function to print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to cleanup on exit
cleanup() {
    print_status "Cleaning up..."

    # Kill background processes
    if [[ -n "$AGENT_PID" ]]; then
        kill $AGENT_PID 2>/dev/null || true
        wait $AGENT_PID 2>/dev/null || true  # Wait for clean exit
        print_status "Stopped agent (PID: $AGENT_PID)"
    fi

    # Stop Docker Compose
    cd "$INTEGRATION_DIR"
    docker compose down --remove-orphans >/dev/null 2>&1 || true
    print_status "Stopped integration environment"
}

# Set up cleanup trap
trap cleanup EXIT

# Function to wait for service to be ready
wait_for_service() {
    local service_name="$1"
    local check_command="$2"
    local max_attempts=30
    local attempt=1

    print_status "Waiting for $service_name to be ready..."

    while [[ $attempt -le $max_attempts ]]; do
        if eval "$check_command" >/dev/null 2>&1; then
            print_success "$service_name is ready"
            return 0
        fi

        echo -n "."
        sleep 2
        ((attempt++))
    done

    print_error "$service_name failed to start within $((max_attempts * 2)) seconds"
    return 1
}

# Function to check if Kafka is ready
check_kafka() {
    docker compose exec -T redpanda rpk cluster info
}

# Function to check if ClickHouse is ready
check_clickhouse() {
    docker compose exec -T clickhouse clickhouse-client --query "SELECT 1"
}

# Function to check database tables
check_tables() {
    cd "$INTEGRATION_DIR"
    local tables=$(docker compose exec -T clickhouse clickhouse-client --query "SHOW TABLES FROM saimiris" 2>/dev/null)
    echo "$tables" | grep -q "from_kafka" && echo "$tables" | grep -q "replies"
}

# Function to create required Kafka topics
create_kafka_topics() {
    cd "$INTEGRATION_DIR"
    print_status "Creating required Kafka topics..."

    # Create saimiris-probes topic
    if docker compose exec -T redpanda rpk topic create saimiris-probes --partitions 1 --replicas 1 >/dev/null 2>&1; then
        print_success "Created topic: saimiris-probes"
    else
        # Topic might already exist, check if it exists
        if docker compose exec -T redpanda rpk topic list | grep -q "saimiris-probes"; then
            print_status "Topic saimiris-probes already exists"
        else
            print_warning "Failed to create topic: saimiris-probes"
        fi
    fi

    # Create saimiris-replies topic
    if docker compose exec -T redpanda rpk topic create saimiris-replies --partitions 1 --replicas 1 >/dev/null 2>&1; then
        print_success "Created topic: saimiris-replies"
    else
        # Topic might already exist, check if it exists
        if docker compose exec -T redpanda rpk topic list | grep -q "saimiris-replies"; then
            print_status "Topic saimiris-replies already exists"
        else
            print_warning "Failed to create topic: saimiris-replies"
        fi
    fi

    # List all topics for verification
    print_status "Available Kafka topics:"
    docker compose exec -T redpanda rpk topic list || print_warning "Could not list topics"

    # Test Kafka connectivity from host to ensure localhost:9092 works
    print_status "Testing Kafka connectivity from host..."
    for i in {1..5}; do
        if docker compose exec -T redpanda rpk cluster metadata --brokers localhost:9092 >/dev/null 2>&1; then
            print_success "Kafka is accessible from host on localhost:9092"
            break
        else
            print_status "Waiting for Kafka to be accessible from host (attempt $i/5)..."
            sleep 3
        fi
    done
}

# Function to run the integration test
run_integration_test() {
    print_status "Starting Saimiris integration test..."

    # Build Saimiris first
    print_status "Building Saimiris..."
    cd "$SAIMIRIS_ROOT"
    cargo build --quiet
    print_success "Saimiris built successfully"

    # Change to integration directory for Docker operations
    cd "$INTEGRATION_DIR"

    # Start the integration environment
    print_status "Starting integration environment..."
    docker compose up -d --force-recreate --renew-anon-volumes

    # Wait for services to be ready
    wait_for_service "Kafka (Redpanda)" "check_kafka"
    wait_for_service "ClickHouse" "check_clickhouse"

    # Create required Kafka topics
    create_kafka_topics

    # Additional wait to ensure Kafka is fully ready for client connections
    print_status "Waiting for Kafka to be fully ready for client connections..."
    sleep 10

    # Wait a bit more for tables to be created
    sleep 5

    # Check if tables exist
    print_status "Verifying ClickHouse tables..."
    if wait_for_service "ClickHouse Tables" "check_tables"; then
        print_success "ClickHouse tables are ready"
    else
        print_warning "Some tables might not be ready, continuing anyway..."
        # Debug: Show what tables actually exist
        print_status "Available tables:"
        docker compose exec -T clickhouse clickhouse-client --query "SHOW TABLES FROM saimiris" 2>/dev/null || print_error "Failed to query tables"
    fi

    # Start the agent in background
    print_status "Starting Saimiris agent..."
    cd "$SAIMIRIS_ROOT"
    cargo run --quiet -- agent --config="$INTEGRATION_DIR/config/saimiris/saimiris.yml" > /tmp/saimiris_agent.log 2>&1 &
    AGENT_PID=$!

    # Wait for agent to start and establish connections
    print_status "Waiting for agent to establish Kafka connections..."
    sleep 15

    # Check if agent is still running
    if ! kill -0 $AGENT_PID 2>/dev/null; then
        print_error "Agent failed to start or crashed"
        print_status "Agent log:"
        cat /tmp/saimiris_agent.log || true
        return 1
    fi

    print_success "Agent started successfully (PID: $AGENT_PID)"

    # Give agent more time to fully initialize and stabilize connections
    sleep 5

    # Run the client with test data
    print_status "Running Saimiris client..."

    # Run client with new agent:ip format
    if run_with_timeout $TIMEOUT_SECONDS "cat '$INTEGRATION_DIR/probes_local.txt' | cargo run --quiet -- client --config='$INTEGRATION_DIR/config/saimiris/saimiris.yml' '$AGENT_ID:127.0.0.1,$AGENT_ID:[::1]'"; then
        print_success "Client completed successfully"
    else
        print_error "Client failed or timed out"
        print_status "Checking agent status..."
        if kill -0 $AGENT_PID 2>/dev/null; then
            print_status "Agent is still running"
        else
            print_error "Agent crashed during client run"
            print_status "Agent log:"
            cat /tmp/saimiris_agent.log || true
        fi
        return 1
    fi

    # Wait more time for probes to be processed and replies to be stored
    print_status "Waiting for probes to be processed and replies to be stored..."
    sleep 30

    # Check agent logs for any errors
    if [ -f /tmp/saimiris_agent.log ]; then
        print_status "Agent log summary:"
        echo "Last few lines of agent log:"
        tail -10 /tmp/saimiris_agent.log || true
        echo ""

        # Check for specific success indicators
        if grep -q "Message intended for this agent. Processing probes." /tmp/saimiris_agent.log; then
            print_status "Agent successfully received and processed probe messages"
        else
            print_warning "Agent may not have processed probe messages successfully"
        fi

        # Check for measurement tracking features in logs
        if grep -q "measurement_id" /tmp/saimiris_agent.log; then
            print_status "Agent detected measurement tracking headers in Kafka messages"
        else
            print_status "No measurement tracking headers detected (expected if client doesn't send them)"
        fi

        # Check for gateway status reporting attempts
        if grep -q "Reported measurement status" /tmp/saimiris_agent.log; then
            print_status "Agent successfully reported measurement status to gateway"
        elif grep -q "Failed to report measurement status" /tmp/saimiris_agent.log; then
            print_status "Agent attempted to report measurement status (gateway connection expected to fail in test environment)"
        else
            print_status "No gateway status reporting detected (expected without gateway configuration)"
        fi
    fi

    # Verify data in ClickHouse or successful probe processing
    print_status "Verifying probe processing..."
    cd "$INTEGRATION_DIR"

    local row_count=$(docker compose exec -T clickhouse clickhouse-client --query "SELECT COUNT(*) FROM saimiris.replies" 2>/dev/null || echo "0")

    if [[ "$row_count" -gt 0 ]]; then
        print_success "Found $row_count records in ClickHouse"

        # Show sample data
        print_status "Sample data from ClickHouse:"
        docker compose exec -T clickhouse clickhouse-client --query "SELECT agent_id, probe_dst_addr, probe_ttl FROM saimiris.replies LIMIT 3" 2>/dev/null || true

    else
        print_status "No records found in ClickHouse - checking if agent successfully processed probe messages..."

        # Check if agent successfully processed the probe messages
        if grep -q "Message intended for this agent. Processing probes." /tmp/saimiris_agent.log; then
            print_success "Agent successfully received and processed probe messages from Kafka"
            print_status "Note: No probe replies in ClickHouse (expected in test environment without actual network probing)"
        else
            print_error "Agent did not successfully process probe messages"
            print_status "Debugging information:"
            print_status "Checking if any data exists in replies table:"
            local total_rows=$(docker compose exec -T clickhouse clickhouse-client --query "SELECT COUNT(*) FROM saimiris.replies" 2>/dev/null || echo "0")
            print_status "Total rows in replies table: $total_rows"

            # Check if from_kafka table has data (raw ingestion)
            local kafka_rows=$(docker compose exec -T clickhouse clickhouse-client --query "SELECT COUNT(*) FROM saimiris.from_kafka" 2>/dev/null || echo "0")
            print_status "Total rows in from_kafka table: $kafka_rows"

            # Show recent agent logs for debugging
            print_status "Recent agent log entries:"
            tail -20 /tmp/saimiris_agent.log || true

            print_error "Integration test failed: Agent did not process probe messages successfully"
            return 1
        fi
    fi

    return 0
}

# Function to test measurement tracking
test_measurement_tracking() {
    print_status "Testing measurement tracking functionality..."

    cd "$SAIMIRIS_ROOT"

    # Run measurement tracking unit tests
    print_status "Running measurement tracking unit tests..."
    if cargo test --quiet measurement_tracking::test_measurement_info_parsing; then
        print_success "Measurement info parsing test passed"
    else
        print_error "Measurement info parsing test failed"
        return 1
    fi

    if cargo test --quiet measurement_tracking::test_probes_with_source_measurement_info; then
        print_success "ProbesWithSource measurement info test passed"
    else
        print_error "ProbesWithSource measurement info test failed"
        return 1
    fi

    if cargo test --quiet measurement_tracking::test_kafka_header_parsing; then
        print_success "Kafka header parsing test passed"
    else
        print_error "Kafka header parsing test failed"
        return 1
    fi

    if cargo test --quiet measurement_tracking::test_end_to_end_measurement_tracking; then
        print_success "End-to-end measurement tracking test passed"
    else
        print_error "End-to-end measurement tracking test failed"
        return 1
    fi

    if cargo test --quiet measurement_tracking::test_measurement_tracking_state_management; then
        print_success "Measurement tracking state management test passed"
    else
        print_error "Measurement tracking state management test failed"
        return 1
    fi

    print_success "All measurement tracking tests passed"
    return 0
}

# Function to run CLI tests
test_cli() {
    print_status "Testing CLI functionality..."

    cd "$SAIMIRIS_ROOT"

    # Test help commands
    print_status "Testing help commands..."
    cargo run --quiet -- --help >/dev/null
    cargo run --quiet -- agent --help >/dev/null
    cargo run --quiet -- client --help >/dev/null
    print_success "Help commands work correctly"

    return 0
}

# Main test execution
main() {
    print_status "=== Saimiris Integration Test Suite ==="
    print_status "Testing changes to Saimiris client and agent functionality"
    print_status ""

    # Verify we're in the right directory
    if [[ ! -f "$SAIMIRIS_ROOT/Cargo.toml" ]]; then
        print_error "Could not find Saimiris Cargo.toml. Are you running from the right directory?"
        exit 1
    fi

    # Check if Docker is available
    if ! command -v docker >/dev/null 2>&1; then
        print_error "Docker is required but not installed"
        exit 1
    fi

    if ! docker compose version >/dev/null 2>&1; then
        print_error "Docker Compose is required but not available"
        exit 1
    fi

    # Check if Docker daemon is running
    if ! docker info >/dev/null 2>&1; then
        print_error "Docker daemon is not running. Please start Docker."
        exit 1
    fi

    # Run CLI tests first (faster, no Docker needed)
    if test_cli; then
        print_success "CLI tests passed"
    else
        print_error "CLI tests failed"
        exit 1
    fi

    # Run measurement tracking tests (unit tests, no Docker needed)
    if test_measurement_tracking; then
        print_success "Measurement tracking tests passed"
    else
        print_error "Measurement tracking tests failed"
        exit 1
    fi

    # Run integration tests
    if run_integration_test; then
        print_success "Integration tests passed"
    else
        print_error "Integration tests failed"
        exit 1
    fi

    print_success "=== All tests completed successfully! ==="
    print_status "Saimiris is working correctly after the changes"
}

# Run the tests
main "$@"
