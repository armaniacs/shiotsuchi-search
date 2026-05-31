#!/bin/bash
# Test runner with timing measurement and slow-test detection.
#
# Usage:
#   ./scripts/test-timing.sh                     # Run all tests (slow + fast)
#   ./scripts/test-timing.sh --fast              # Only fast tests (no model)
#   ./scripts/test-timing.sh --slow              # Only model-dependent tests
#   ./scripts/test-timing.sh --retry-slow        # Re-run only previously-slow tests
#
# Output:
#   - Timing per crate and per slow test
#   - Flags tests taking >2x the average
#   - Total execution time

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
MODEL_PATH="${SHIOTSUCHI_MODEL_PATH:-$PROJECT_DIR/models/bccwj-suw+unidic_pos+kana.model.zst}"
TIMING_LOG="$PROJECT_DIR/target/test-timing.log"
SLOW_LOG="$PROJECT_DIR/target/test-slow.log"

mkdir -p "$PROJECT_DIR/target"

# ── Helpers ──────────────────────────────────────────

# parse timing from "finished in X.XXs" in cargo test output
parse_timing() {
    grep -oE 'finished in [0-9]+(\.[0-9]+)?(ms|µs|s)' | sed 's/finished in //'
}

# convert timing to milliseconds
to_ms() {
    local val="$1"
    if echo "$val" | grep -q 'ms$'; then
        echo "${val%ms}"
    elif echo "$val" | grep -q 'µs$'; then
        local us="${val%µs}"
        echo "$((us / 1000))"
    elif echo "$val" | grep -q 's$'; then
        local s="${val%s}"
        # bc might not be available, use awk for float
        echo "$(awk "BEGIN { printf \"%d\", $s * 1000 }")"
    else
        echo "0"
    fi
}

# ── Run a test and capture timing ────────────────────

run_timed() {
    local label="$1"
    shift
    echo "─── $label ───"
    echo "[$(date '+%H:%M:%S')] Starting: $label"

    local start_ms
    start_ms=$(awk 'BEGIN{srand(); print srand() * 1000}')
    # macOS: use perl for ms precision
    if [[ "$(uname)" == "Darwin" ]]; then
        start_ms=$(perl -MTime::HiRes -e 'printf "%d\n", Time::HiRes::time() * 1000')
    fi

    # Run test, capture stdout/stderr, get timing
    local output
    output=$("$@" 2>&1) || true
    local exit_code=$?

    local end_ms
    if [[ "$(uname)" == "Darwin" ]]; then
        end_ms=$(perl -MTime::HiRes -e 'printf "%d\n", Time::HiRes::time() * 1000')
    fi

    local elapsed_ms=$((end_ms - start_ms))
    local elapsed_sec
    elapsed_sec=$(awk "BEGIN { printf \"%.1f\", $elapsed_ms / 1000 }")

    echo "[$(date '+%H:%M:%S')] Completed: $label (${elapsed_sec}s)"

    # Check result
    local passed
    passed=$(echo "$output" | grep -c 'test result: ok' || true)
    local failed
    failed=$(echo "$output" | grep -c 'test result: FAILED' || true)

    if [ "$failed" -gt 0 ]; then
        echo "  ❌ FAILED after ${elapsed_sec}s"
        echo "$output" | grep -E "FAILED|panicked" || true
        echo "::group::Full output"
        echo "$output"
        echo "::endgroup::"
    else
        echo "  ✅ Passed in ${elapsed_sec}s"
    fi

    # Log timing
    echo "$elapsed_sec|$elapsed_ms|$label|passed=$passed,failed=$failed,exit=$exit_code" >> "$TIMING_LOG"

    return $exit_code
}

# ── Report ───────────────────────────────────────────

generate_report() {
    echo ""
    echo "═══════════════════════════════════════"
    echo "  Timing Report"
    echo "═══════════════════════════════════════"

    if [ ! -f "$TIMING_LOG" ]; then
        echo "  No tests were run."
        return
    fi

    local total_ms=0
    local count=0
    local -a all_timings
    local -a all_labels

    while IFS='|' read -r sec ms label result; do
        all_timings+=("$ms")
        all_labels+=("$label")
        total_ms=$((total_ms + ms))
        count=$((count + 1))
    done < "$TIMING_LOG"

    if [ "$count" -eq 0 ]; then
        echo "  No tests were run."
        return
    fi

    # Calculate average
    local avg_ms=$((total_ms / count))
    local avg_sec
    avg_sec=$(awk "BEGIN { printf \"%.1f\", $avg_ms / 1000 }")

    echo ""
    echo "  Total:  $(awk "BEGIN { printf \"%.1f\", $total_ms / 1000 }")s across $count test groups"
    echo "  Avg:    ${avg_sec}s per group"
    echo "  Threshold (2x avg): $(awk "BEGIN { printf \"%.1f\", $avg_ms * 2 / 1000 }")s"
    echo ""

    # Find and flag slow tests
    local slow_count=0
    local -a slow_indices

    for i in "${!all_timings[@]}"; do
        if [ "${all_timings[$i]}" -gt "$((avg_ms * 2))" ]; then
            slow_indices+=("$i")
            slow_count=$((slow_count + 1))
        fi
    done

    if [ "$slow_count" -gt 0 ]; then
        echo "  🐌 Slow tests (>2x avg):"
        echo "  ─────────────────────────────"
        for idx in "${slow_indices[@]}"; do
            local t="${all_timings[$idx]}"
            local sec
            sec=$(awk "BEGIN { printf \"%.1f\", $t / 1000 }")
            local label="${all_labels[$idx]}"
            local ratio
            ratio=$(awk "BEGIN { printf \"%.1f\", $t / $avg_ms }")
            printf "  %6ss  %s\n" "$sec" "$label"

            # Save to slow log for retry
            echo "$sec|$label|$ratio" >> "$SLOW_LOG"
        done
        echo ""
        echo "  To re-run slow tests: ./scripts/test-timing.sh --retry-slow"
    else
        echo "  ✅ No slow tests detected."
    fi
    echo ""
}

# ── Main ─────────────────────────────────────────────

rm -f "$TIMING_LOG" "$SLOW_LOG"

MODE="${1:-all}"

if [ "$MODE" = "--fast" ] || [ "$MODE" = "all" ]; then
    echo "╔══════════════════════════════════════╗"
    echo "║  Fast tests (no model required)      ║"
    echo "╚══════════════════════════════════════╝"
    # Unset model path to keep fast tests fast
    _saved_model=""
    if [ -n "${SHIOTSUCHI_MODEL_PATH+x}" ]; then _saved_model="$SHIOTSUCHI_MODEL_PATH"; fi
    unset SHIOTSUCHI_MODEL_PATH
    run_timed "shiotsuchi (CLI)" cargo test -p shiotsuchi
    run_timed "shiotsuchi-mcp" cargo test -p shiotsuchi-mcp
    if [ -n "$_saved_model" ]; then SHIOTSUCHI_MODEL_PATH="$_saved_model"; fi
fi

if [ "$MODE" = "--slow" ] || [ "$MODE" = "all" ]; then
    echo ""
    echo "╔══════════════════════════════════════╗"
    echo "║  Slow tests (model-dependent)        ║"
    echo "╚══════════════════════════════════════╝"

    # Core tests with model
    SHIOTSUCHI_MODEL_PATH="$MODEL_PATH" \
        run_timed "shiotsuchi-core (with model)" cargo test -p shiotsuchi-core

    # Individual slow model-dependent tests
    SHIOTSUCHI_MODEL_PATH="$MODEL_PATH" \
        run_timed "chart::test_chart_indexes_files" cargo test -p shiotsuchi -- chart::tests::test_chart_indexes_files

    SHIOTSUCHI_MODEL_PATH="$MODEL_PATH" \
        run_timed "clean::test_run_clean_full_flow" cargo test -p shiotsuchi -- clean::tests::test_run_clean_full_flow

    SHIOTSUCHI_MODEL_PATH="$MODEL_PATH" \
        run_timed "doctor::test_index_vault_creates_db" cargo test -p shiotsuchi -- doctor::tests::test_index_vault_creates_db_with_files

    SHIOTSUCHI_MODEL_PATH="$MODEL_PATH" \
        run_timed "doctor::test_rebuild_db_reindexes" cargo test -p shiotsuchi -- doctor::tests::test_rebuild_db_reindexes_after_damage

    # MCP handler test with model
    SHIOTSUCHI_MODEL_PATH="$MODEL_PATH" \
        run_timed "mcp::search_local_notes_fts" cargo test -p shiotsuchi-mcp -- handler::tests::test_search_local_notes_fts_returns_content
fi

if [ "$MODE" = "--retry-slow" ]; then
    echo ""
    echo "╔══════════════════════════════════════╗"
    echo "║  Retrying slow tests                 ║"
    echo "╚══════════════════════════════════════╝"
    if [ -f "$SLOW_LOG" ]; then
        while IFS='|' read -r sec label ratio; do
            echo "─── Retry: $label (was ${sec}s, ${ratio}x avg) ───"
            # Extract crate and test name from label
            case "$label" in
                "shiotsuchi (CLI)")         SHIOTSUCHI_MODEL_PATH="$MODEL_PATH" run_timed "(retry) $label" cargo test -p shiotsuchi ;;
                "shiotsuchi-mcp (no model)") run_timed "(retry) $label" cargo test -p shiotsuchi-mcp ;;
                "shiotsuchi-core (with model)") SHIOTSUCHI_MODEL_PATH="$MODEL_PATH" run_timed "(retry) $label" cargo test -p shiotsuchi-core ;;
                chart*) SHIOTSUCHI_MODEL_PATH="$MODEL_PATH" run_timed "(retry) $label" cargo test -p shiotsuchi -- chart::tests::"$(echo "$label" | sed 's/.*:://')" ;;
                clean*) SHIOTSUCHI_MODEL_PATH="$MODEL_PATH" run_timed "(retry) $label" cargo test -p shiotsuchi -- clean::tests::"$(echo "$label" | sed 's/.*:://')" ;;
                doctor*) SHIOTSUCHI_MODEL_PATH="$MODEL_PATH" run_timed "(retry) $label" cargo test -p shiotsuchi -- doctor::tests::"$(echo "$label" | sed 's/.*:://')" ;;
                mcp*) SHIOTSUCHI_MODEL_PATH="$MODEL_PATH" run_timed "(retry) $label" cargo test -p shiotsuchi-mcp -- handler::tests::"$(echo "$label" | sed 's/.*:://')" ;;
                *) echo "  ⚠️  Unknown test label: $label" ;;
            esac
        done < "$SLOW_LOG"
    else
        echo "  No slow test log found. Run tests first."
    fi
fi

generate_report

# Cleanup old logs (keep last run)
mv "$TIMING_LOG" "${TIMING_LOG}.bak" 2>/dev/null || true
mv "$SLOW_LOG" "${SLOW_LOG}.bak" 2>/dev/null || true
