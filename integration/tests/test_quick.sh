#!/bin/bash

# Saimiris Quick Test Script
# This script performs fast tests without Docker for quick validation

set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SAIMIRIS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

test_compilation() {
    print_status "Testing compilation..."
    cd "$SAIMIRIS_ROOT"
    cargo check --quiet
    cargo build --quiet
    print_success "Compilation successful"
}

test_cli_help() {
    print_status "Testing CLI help commands..."
    cd "$SAIMIRIS_ROOT"
    cargo run --quiet -- --help >/dev/null
    cargo run --quiet -- agent --help >/dev/null
    cargo run --quiet -- client --help >/dev/null
    print_success "All help commands work"
}

test_basic_functionality() {
    print_status "Testing basic client functionality..."

    cd "$SAIMIRIS_ROOT"

    # Test that client starts correctly (should fail on config)
    if echo "test" | cargo run --quiet -- client --config=/dev/null agent1 2>&1 | grep -q "invalid socket address"; then
        print_success "Client starts correctly (failed later on config as expected)"
    else
        print_error "Auto-generated UUID failed or unexpected error"
        return 1
    fi
}

test_config_structure() {
    print_status "Testing config module structure..."

    cd "$SAIMIRIS_ROOT"

    # Check that config files exist
    local config_files=(
        "src/config/mod.rs"
        "src/config/agent.rs"
        "src/config/client.rs"
        "src/config/kafka.rs"
    )

    for file in "${config_files[@]}"; do
        if [[ ! -f "$file" ]]; then
            print_error "Missing config file: $file"
            return 1
        fi
    done

    # Check that old client config is gone
    if [[ -f "src/client/config.rs" ]]; then
        print_error "Old client/config.rs still exists (should have been moved)"
        return 1
    fi

    print_success "Config structure is correct"
}

test_agent_src_ips() {
    print_status "Testing agent source IPs functionality..."

    cd "$SAIMIRIS_ROOT"

    # Test mismatched agent and source IP count (should fail)
    if echo "test" | cargo run --quiet -- client --config=/dev/null --agent-src-ips="192.168.1.1,192.168.1.2" agent1 2>&1 | grep -q "Number of agent source IPs must match"; then
        print_success "Agent source IP count validation works"
    else
        print_error "Agent source IP validation failed"
        return 1
    fi

    # Test matching count (should pass validation, fail on config)
    if echo "test" | cargo run --quiet -- client --config=/dev/null --agent-src-ips="192.168.1.1,192.168.1.2" agent1,agent2 2>&1 | grep -q "invalid socket address"; then
        print_success "Matching agent source IP count works"
    else
        print_error "Matching agent source IP count failed"
        return 1
    fi
}

main() {
    print_status "=== Saimiris Quick Test Suite ==="
    print_status "Running fast tests without Docker setup"
    print_status ""

    # Verify we're in the right directory
    cd "$SAIMIRIS_ROOT"
    if [[ ! -f "Cargo.toml" ]]; then
        print_error "Could not find Cargo.toml at $SAIMIRIS_ROOT"
        exit 1
    fi

    # Run tests
    test_compilation
    test_config_structure
    test_cli_help
    test_basic_functionality
    test_agent_src_ips

    print_success "=== All quick tests passed! ==="
    print_status "Ready for integration testing with Docker"
}

main "$@"
