#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:4444}"
TABLE_NAME="ReasonDB Docs"
TABLE_SLUG="reasondb_docs"
RESULTS_DIR="scripts/results"

# ── Helpers ──────────────────────────────────────────────

bold()  { printf "\033[1m%s\033[0m" "$1"; }
green() { printf "\033[32m%s\033[0m" "$1"; }
red()   { printf "\033[31m%s\033[0m" "$1"; }
cyan()  { printf "\033[36m%s\033[0m" "$1"; }
yellow(){ printf "\033[33m%s\033[0m" "$1"; }

check_deps() {
  for cmd in curl jq; do
    if ! command -v "$cmd" &>/dev/null; then
      echo "$(red "Error:") $cmd is required but not installed."
      exit 1
    fi
  done
}

health_check() {
  if ! curl -sf "$BASE_URL/health" >/dev/null 2>&1; then
    echo "$(red "Error:") Server is not running at $BASE_URL"
    exit 1
  fi
}

verify_table() {
  local tables_json table_id
  tables_json="$(curl -sf "$BASE_URL/v1/tables")"
  table_id="$(echo "$tables_json" | jq -r --arg name "$TABLE_NAME" '.tables[] | select(.name == $name) | .id')"
  if [ -z "$table_id" ]; then
    echo "$(red "Error:") Table \"$TABLE_NAME\" not found."
    echo "Run $(bold "make seed-docs") first to ingest the documentation."
    exit 1
  fi
  echo "$table_id"
}

# ── Single query worker (runs in a subshell) ─────────────
# Fields: idx|category|label|query|qtype|expected_doc
# Writes: <idx>.tty, <idx>.md, <idx>.summary, <idx>.score

run_query_worker() {
  local idx="$1" category="$2" label="$3" query="$4" qtype="$5" expected_doc="$6" tmpdir="$7"
  local tty_file="$tmpdir/${idx}.tty"
  local md_file="$tmpdir/${idx}.md"
  local summary_file="$tmpdir/${idx}.summary"
  local score_file="$tmpdir/${idx}.score"
  local response http_code body

  response="$(curl -s -w "\n%{http_code}" -X POST "$BASE_URL/v1/query" \
    -H "Content-Type: application/json" \
    -d "$(jq -n --arg q "$query" '{query: $q}')")"

  http_code="$(echo "$response" | tail -1)"
  body="$(echo "$response" | sed '$d')"

  # ── Error case ──
  if [ "$http_code" != "200" ]; then
    local err_msg
    err_msg="$(echo "$body" | jq -r '.message // .error // "Unknown error"' 2>/dev/null || echo "$body")"

    {
      echo "┌──────────────────────────────────────────────────────────"
      echo "│ [$category] $(bold "$label")"
      echo "│ $(cyan "$query")"
      echo "│ $(red "HTTP $http_code") — $err_msg"
      echo "└──────────────────────────────────────────────────────────"
      echo ""
    } > "$tty_file"

    echo "| $idx | $label | $qtype | — | — | $expected_doc | ❌ ERR |" > "$summary_file"
    echo "0" > "$score_file"

    {
      echo ""
      echo "### $idx. [$category] $label"
      echo ""
      echo '```sql'
      echo "$query"
      echo '```'
      echo ""
      echo "**Expected:** $expected_doc"
      echo ""
      echo "> **Error:** HTTP $http_code — $err_msg"
      echo ""
    } > "$md_file"

    return 1
  fi

  # ── Success case ──
  local doc_count exec_time
  doc_count="$(echo "$body" | jq -r '.total_count // (.documents | length) // 0')"
  exec_time="$(echo "$body" | jq -r '.execution_time_ms // "?"')"

  # Check if expected doc appears in results.
  # Supports |-separated alternatives: "Rql Guide|Concepts" matches either.
  local pass="MISS"
  local pass_icon="❌"
  if [ "$expected_doc" = "*" ]; then
    pass="PASS"
    pass_icon="✅"
  elif [ "$doc_count" != "0" ]; then
    local alt
    IFS='|' read -ra alternatives <<< "$expected_doc"
    for alt in "${alternatives[@]}"; do
      local found
      found="$(echo "$body" | jq -r --arg exp "$alt" \
        '[.documents[].title] | map(ascii_downcase) | map(contains($exp | ascii_downcase)) | any' 2>/dev/null)"
      if [ "$found" = "true" ]; then
        pass="PASS"
        pass_icon="✅"
        break
      fi
    done
  fi

  # Score: 1=pass, 0=miss
  if [ "$pass" = "PASS" ]; then echo "1" > "$score_file"; else echo "0" > "$score_file"; fi

  # Terminal output
  local pass_tty
  if [ "$pass" = "PASS" ]; then pass_tty="$(green "PASS")"; else pass_tty="$(red "MISS")"; fi

  {
    echo "┌──────────────────────────────────────────────────────────"
    echo "│ [$category] $(bold "$label")  [$pass_tty]"
    echo "│ $(cyan "$query")"
    echo "│ $(green "$doc_count results") in ${exec_time}ms  (expected: $expected_doc)"
    echo "$body" | jq -r '
      .documents[:2][] |
      "│   📄 \(.title // "Untitled")" +
      (if .confidence then "  (confidence: \(.confidence))" else "" end) +
      (if .answer then "\n│      → \(.answer[:150])" + (if (.answer | length) > 150 then "..." else "" end) else "" end)
    ' 2>/dev/null || true
    echo "└──────────────────────────────────────────────────────────"
    echo ""
  } > "$tty_file"

  # Summary row
  echo "| $idx | $label | $qtype | $doc_count | ${exec_time}ms | $expected_doc | $pass_icon |" > "$summary_file"

  # Markdown detail
  {
    echo ""
    echo "### $idx. [$category] $label"
    echo ""
    echo '```sql'
    echo "$query"
    echo '```'
    echo ""
    echo "**Expected:** $expected_doc | **Result:** $pass_icon $pass ($doc_count results, ${exec_time}ms)"
    echo ""

    local has_docs
    has_docs="$(echo "$body" | jq '.documents | length')"
    if [ "$has_docs" -gt 0 ]; then
      echo "| # | Title | Confidence | Answer |"
      echo "|---|-------|------------|--------|"
      echo "$body" | jq -r '
        .documents[:5] | to_entries[] |
        "| \(.key + 1) | \(.value.title // "Untitled") | \(.value.confidence // "—") | \(
          if .value.answer then
            (.value.answer[:200] | gsub("\n"; " ") | gsub("\\|"; "\\\\|")) +
            (if (.value.answer | length) > 200 then "..." else "" end)
          else "—" end
        ) |"
      ' 2>/dev/null || true
      echo ""
    fi

    echo "<details>"
    echo "<summary>Raw JSON response</summary>"
    echo ""
    echo '```json'
    echo "$body" | jq '.' 2>/dev/null || echo "$body"
    echo '```'
    echo "</details>"
    echo ""
  } > "$md_file"
}

# ── Query definitions ────────────────────────────────────
# Format: idx|category|label|query|type|expected_doc

T="$TABLE_SLUG"

QUERIES=(
  # ── A: Baseline (document-level topics, should always pass) ──
  "A1|A-Baseline|What is ReasonDB|SELECT * FROM $T REASON 'What is ReasonDB and what problem does it solve?' LIMIT 3|REASON|Index"
  "A2|A-Baseline|How ingestion works|SELECT * FROM $T REASON 'How does document ingestion work?' LIMIT 3|REASON|Ingestion Guide"
  "A3|A-Baseline|RQL overview|SELECT * FROM $T REASON 'What is RQL and how do I write queries?' LIMIT 3|REASON|Rql Guide"
  "A4|A-Baseline|Backup keyword search|SELECT * FROM $T SEARCH 'backup recovery' LIMIT 5|SEARCH|Backup"

  # ── B: Mid-depth (H2 section targets, probes Phase 2+3) ──
  "B1|B-MidDepth|Beam search in concepts|SELECT * FROM $T REASON 'What is beam search and how does it explore the document tree?' LIMIT 3|REASON|Concepts"
  "B2|B-MidDepth|Raft consensus clustering|SELECT * FROM $T REASON 'How does Raft consensus work in ReasonDB clustering?' LIMIT 3|REASON|Clustering"
  "B3|B-MidDepth|Relationship types|SELECT * FROM $T REASON 'What are the different document relationship types like references and supersedes?' LIMIT 3|REASON|Relationships Guide"
  "B4|B-MidDepth|Export and import|SELECT * FROM $T REASON 'How do I export and import data for backups?' LIMIT 3|REASON|Backup"
  "B5|B-MidDepth|Prometheus metrics|SELECT * FROM $T REASON 'What Prometheus metrics does ReasonDB expose for monitoring?' LIMIT 3|REASON|Monitoring"

  # ── C: Deep/specific (subsection details, probes beam search limits) ──
  "C1|C-Deep|Token bucket rate limiting|SELECT * FROM $T REASON 'What is the token bucket algorithm used for rate limiting?' LIMIT 3|REASON|Rate Limiting|Errors|Introduction"
  "C2|C-Deep|Default port number|SELECT * FROM $T REASON 'What is the default server port number for ReasonDB?' LIMIT 3|REASON|Configuration|Introduction|Quickstart"
  "C3|C-Deep|Rate limit error code|SELECT * FROM $T REASON 'What HTTP error code is returned when rate limited?' LIMIT 3|REASON|Errors|Introduction|Rate Limiting"
  "C4|C-Deep|API key format prefix|SELECT * FROM $T REASON 'What is the format of ReasonDB API keys and what prefix do they use?' LIMIT 3|REASON|Authentication"
  "C5|C-Deep|Storage engine tech|SELECT * FROM $T REASON 'What Rust crates does ReasonDB use for storage and text indexing?' LIMIT 3|REASON|Architecture"

  # ── D: SEARCH+REASON combos (same hard questions with BM25 pre-filter) ──
  "D1|D-Hybrid|Token bucket + SEARCH|SELECT * FROM $T SEARCH 'token bucket' REASON 'How does rate limiting work?' LIMIT 3|SEARCH+REASON|Rate Limiting"
  "D2|D-Hybrid|Port config + SEARCH|SELECT * FROM $T SEARCH 'port' REASON 'What is the default server port?' LIMIT 3|SEARCH+REASON|Configuration"
  "D3|D-Hybrid|Error codes + SEARCH|SELECT * FROM $T SEARCH 'error code 429' REASON 'What happens when rate limited?' LIMIT 3|SEARCH+REASON|Errors|Introduction|Rate Limiting"
  "D4|D-Hybrid|API key + SEARCH|SELECT * FROM $T SEARCH 'API key rdb_live' REASON 'What prefix do API keys use?' LIMIT 3|SEARCH+REASON|Authentication"
  "D5|D-Hybrid|Storage crates + SEARCH|SELECT * FROM $T SEARCH 'redb tantivy' REASON 'What storage engine is used?' LIMIT 3|SEARCH+REASON|Architecture"

  # ── E: Cross-doc and tag-filtered ──
  "E1|E-Filtered|Tutorial: legal contracts|SELECT * FROM $T WHERE tags CONTAINS ANY ('tutorial') REASON 'How do I search and analyze legal contracts?' LIMIT 3|WHERE+REASON|Legal Search"
  "E2|E-Filtered|Guide: manage API keys|SELECT * FROM $T WHERE tags CONTAINS ANY ('guide') REASON 'How do I create and manage API keys?' LIMIT 3|WHERE+REASON|Authentication"
  "E3|E-Filtered|Advanced: workspace crates|SELECT * FROM $T WHERE tags CONTAINS ANY ('advanced') REASON 'What crates make up the ReasonDB workspace?' LIMIT 3|WHERE+REASON|Architecture"
  "E4|E-Filtered|Contract management|SELECT * FROM $T REASON 'How do I build a contract management system with document relationships?' LIMIT 3|REASON|Document Relationships"

  # ── F: Edge cases and stress tests ──
  "F1|F-Edge|Irrelevant question|SELECT * FROM $T REASON 'What is the meaning of life?' LIMIT 3|REASON|*"
  "F2|F-Edge|Compound: monitoring stack|SELECT * FROM $T REASON 'How do I set up Prometheus monitoring with Grafana dashboards and alerting?' LIMIT 3|REASON|Monitoring"
  "F3|F-Edge|Plugin dev with SEARCH|SELECT * FROM $T SEARCH 'plugin python extractor' REASON 'How do I write a Python extractor plugin?' LIMIT 3|SEARCH+REASON|Plugins Development"
  "F4|F-Edge|SEARCH vs REASON difference|SELECT * FROM $T REASON 'What are the differences between SEARCH and REASON queries in RQL?' LIMIT 3|REASON|Concepts|Rql Guide|Rql Basics|Search Guide"
)

# ── Main ─────────────────────────────────────────────────

main() {
  local timestamp wall_start wall_end wall_elapsed
  timestamp="$(date -u '+%Y%m%d-%H%M%S')"
  wall_start="$(date +%s)"

  echo ""
  echo "$(bold "ReasonDB Docs — Blind Spot Test Suite") (parallel)"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""

  check_deps
  health_check

  local table_id
  table_id="$(verify_table)"
  echo "$(green "✓") Using table $(bold "$TABLE_NAME") ($table_id)"
  echo "$(green "✓") Running ${#QUERIES[@]} queries in parallel ..."
  echo ""

  local tmpdir
  tmpdir="$(mktemp -d)"
  trap "rm -rf '$tmpdir'" EXIT

  # Launch all queries in parallel
  local pids=()
  for entry in "${QUERIES[@]}"; do
    IFS='|' read -r idx category label query qtype expected_doc <<< "$entry"
    run_query_worker "$idx" "$category" "$label" "$query" "$qtype" "$expected_doc" "$tmpdir" &
    pids+=($!)
  done

  # Wait for all
  local http_failures=0
  for pid in "${pids[@]}"; do
    wait "$pid" 2>/dev/null || http_failures=$((http_failures + 1))
  done

  wall_end="$(date +%s)"
  wall_elapsed=$(( wall_end - wall_start ))

  # Compute score
  local total_pass=0 total_queries=${#QUERIES[@]}
  for entry in "${QUERIES[@]}"; do
    IFS='|' read -r idx _ _ _ _ _ <<< "$entry"
    if [ -f "$tmpdir/${idx}.score" ]; then
      total_pass=$(( total_pass + $(cat "$tmpdir/${idx}.score") ))
    fi
  done

  # Print terminal output in order, grouped by category
  local current_cat=""
  for entry in "${QUERIES[@]}"; do
    IFS='|' read -r idx category _ _ _ _ <<< "$entry"
    if [ "$category" != "$current_cat" ]; then
      current_cat="$category"
      echo "$(bold "── $category ──")"
      echo ""
    fi
    [ -f "$tmpdir/${idx}.tty" ] && cat "$tmpdir/${idx}.tty"
  done

  # Print score
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""
  if [ "$total_pass" -eq "$total_queries" ]; then
    echo "  $(bold "Score: $(green "$total_pass/$total_queries")")"
  else
    echo "  $(bold "Score:") $(green "$total_pass")/$total_queries  ($(red "$((total_queries - total_pass)) missed"))"
  fi
  echo "  $(bold "Wall time:") ${wall_elapsed}s (parallel)"
  echo ""

  # Assemble markdown report
  mkdir -p "$RESULTS_DIR"
  REPORT_FILE="$RESULTS_DIR/test-results-${timestamp}.md"

  {
    echo "# ReasonDB Docs — Blind Spot Test Results"
    echo ""
    echo "## Score: $total_pass / $total_queries"
    echo ""
    echo "| | |"
    echo "|---|---|"
    echo "| **Date** | $(date -u '+%Y-%m-%d %H:%M:%S UTC') |"
    echo "| **Server** | \`$BASE_URL\` |"
    echo "| **Table** | $TABLE_NAME (\`$table_id\`) |"
    echo "| **Total wall time** | ${wall_elapsed}s (parallel) |"
    echo "| **Queries** | $total_queries |"
    echo "| **Passed** | $total_pass |"
    echo "| **Missed** | $((total_queries - total_pass)) |"
    echo ""
    echo "---"
    echo ""
    echo "## Summary"
    echo ""

    # Group by category
    current_cat=""
    for entry in "${QUERIES[@]}"; do
      IFS='|' read -r idx category _ _ _ _ <<< "$entry"
      if [ "$category" != "$current_cat" ]; then
        if [ -n "$current_cat" ]; then echo ""; fi
        current_cat="$category"
        echo "### $category"
        echo ""
        echo "| # | Query | Type | Results | Time | Expected Doc | Pass |"
        echo "|---|-------|------|---------|------|-------------|------|"
      fi
      [ -f "$tmpdir/${idx}.summary" ] && cat "$tmpdir/${idx}.summary"
    done

    echo ""
    echo "---"
    echo ""
    echo "## Details"

    for entry in "${QUERIES[@]}"; do
      IFS='|' read -r idx _ _ _ _ _ <<< "$entry"
      [ -f "$tmpdir/${idx}.md" ] && cat "$tmpdir/${idx}.md"
    done

    echo "---"
    echo ""
    echo "*Generated by \`make test-queries\` on $(date -u '+%Y-%m-%d %H:%M:%S UTC') — score $total_pass/$total_queries, wall time ${wall_elapsed}s*"
  } > "$REPORT_FILE"

  echo "$(green "✓") Report saved to $(bold "$REPORT_FILE")"
  echo ""
}

main "$@"
