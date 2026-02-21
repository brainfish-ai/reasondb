#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:4444}"
TABLE_NAME="ReasonDB Docs"
TABLE_SLUG="reasondb_docs"
DOCS_DIR="docs"
POLL_INTERVAL=2
MAX_POLL_ATTEMPTS=120

# ── Helpers ──────────────────────────────────────────────

bold()  { printf "\033[1m%s\033[0m" "$1"; }
green() { printf "\033[32m%s\033[0m" "$1"; }
red()   { printf "\033[31m%s\033[0m" "$1"; }
dim()   { printf "\033[2m%s\033[0m" "$1"; }

check_deps() {
  for cmd in curl jq; do
    if ! command -v "$cmd" &>/dev/null; then
      echo "$(red "Error:") $cmd is required but not installed."
      exit 1
    fi
  done
}

health_check() {
  echo "Checking server at $BASE_URL ..."
  if ! curl -sf "$BASE_URL/health" >/dev/null 2>&1; then
    echo "$(red "Error:") Server is not running at $BASE_URL"
    echo "Start the server first:  make docker-up  or  reasondb serve"
    exit 1
  fi
  echo "$(green "✓") Server is healthy"
}

# Derive a human-readable title from a file path.
# docs/guides/search.mdx       -> "Search Guide"
# docs/tutorials/knowledge-base.mdx -> "Knowledge Base Tutorial"
# docs/quickstart.mdx           -> "Quickstart"
derive_title() {
  local filepath="$1"
  local filename dir_name base_name

  filename="$(basename "$filepath" .mdx)"
  dir_name="$(basename "$(dirname "$filepath")")"

  # Convert kebab-case / snake_case to Title Case
  base_name="$(echo "$filename" | tr '_-' ' ' | awk '{for(i=1;i<=NF;i++) $i=toupper(substr($i,1,1)) substr($i,2)}1')"

  case "$dir_name" in
    guides)        echo "$base_name Guide" ;;
    tutorials)     echo "$base_name Tutorial" ;;
    api-reference) echo "$base_name API Reference" ;;
    advanced)      echo "$base_name Advanced" ;;
    *)             echo "$base_name" ;;
  esac
}

# Derive tags from the directory.
derive_tags() {
  local filepath="$1"
  local dir_name
  dir_name="$(basename "$(dirname "$filepath")")"

  case "$dir_name" in
    guides)        echo '["guide"]' ;;
    tutorials)     echo '["tutorial"]' ;;
    api-reference) echo '["api-reference"]' ;;
    advanced)      echo '["advanced"]' ;;
    *)             echo '["overview"]' ;;
  esac
}

# ── Table Management ─────────────────────────────────────

find_existing_table() {
  local tables_json table_id
  tables_json="$(curl -sf "$BASE_URL/v1/tables")"
  table_id="$(echo "$tables_json" | jq -r --arg name "$TABLE_NAME" '.tables[] | select(.name == $name) | .id')"
  echo "$table_id"
}

delete_table() {
  local table_id="$1"
  echo "Deleting existing table $table_id (cascade) ..."
  curl -sf -X DELETE "$BASE_URL/v1/tables/$table_id" \
    -H "Content-Type: application/json" \
    -d '{"cascade": true}' >/dev/null
  echo "$(green "✓") Old table deleted"
}

create_table() {
  local response table_id
  echo "Creating table $(bold "$TABLE_NAME") ..."
  response="$(curl -sf -X POST "$BASE_URL/v1/tables" \
    -H "Content-Type: application/json" \
    -d "$(jq -n --arg name "$TABLE_NAME" --arg desc "ReasonDB documentation — auto-seeded from docs/" \
      '{name: $name, description: $desc}')")"

  table_id="$(echo "$response" | jq -r '.id')"
  if [ -z "$table_id" ] || [ "$table_id" = "null" ]; then
    echo "$(red "Error:") Failed to create table"
    echo "$response"
    exit 1
  fi
  echo "$(green "✓") Created table $table_id"
  echo "$table_id"
}

# ── Ingestion ────────────────────────────────────────────

build_batch_payload() {
  local table_id="$1"
  shift
  local files=("$@")

  local docs_json="[]"
  for filepath in "${files[@]}"; do
    local title tags content
    title="$(derive_title "$filepath")"
    tags="$(derive_tags "$filepath")"
    content="$(cat "$filepath")"

    docs_json="$(echo "$docs_json" | jq \
      --arg title "$title" \
      --arg content "$content" \
      --argjson tags "$tags" \
      '. + [{title: $title, content: $content, tags: $tags}]')"

    echo "  $(dim "→") $title" >&2
  done

  jq -n --arg table_id "$table_id" --argjson documents "$docs_json" \
    '{table_id: $table_id, documents: $documents}'
}

ingest_batch() {
  local table_id="$1"
  shift
  local files=("$@")
  local payload response

  payload="$(build_batch_payload "$table_id" "${files[@]}")"

  response="$(curl -sf -X POST "$BASE_URL/v1/ingest/batch" \
    -H "Content-Type: application/json" \
    -d "$payload" 2>&1)" || {
    echo "$(red "Error:") Batch ingestion request failed"
    return 1
  }

  local jobs_created
  jobs_created="$(echo "$response" | jq -r '.jobs_created')"
  echo "" >&2
  echo "$(green "✓") Enqueued $jobs_created jobs in a single request" >&2

  echo "$response" | jq -r '.job_ids[]'
}

poll_job() {
  local job_id="$1" attempt=0 status
  while [ $attempt -lt $MAX_POLL_ATTEMPTS ]; do
    status="$(curl -sf "$BASE_URL/v1/jobs/$job_id" | jq -r '.status')"
    case "$status" in
      completed) return 0 ;;
      failed)    return 1 ;;
      *)         sleep "$POLL_INTERVAL"; attempt=$((attempt + 1)) ;;
    esac
  done
  return 2  # timed out
}

poll_all_jobs() {
  local job_ids=("$@")
  local total=${#job_ids[@]}
  local completed=0 failed=0 attempt=0
  local remaining=$total

  echo "Polling $total jobs concurrently ..."

  local pending_list=("${job_ids[@]}")

  while [ $remaining -gt 0 ] && [ $attempt -lt $MAX_POLL_ATTEMPTS ]; do
    local next_pending=()
    for jid in "${pending_list[@]}"; do
      local status
      status="$(curl -sf "$BASE_URL/v1/jobs/$jid" | jq -r '.status' 2>/dev/null || echo "unknown")"
      case "$status" in
        completed) completed=$((completed + 1)); remaining=$((remaining - 1)) ;;
        failed)    failed=$((failed + 1)); remaining=$((remaining - 1)); echo "  $(red "✗") $jid failed" ;;
        *)         next_pending+=("$jid") ;;
      esac
    done

    pending_list=("${next_pending[@]+"${next_pending[@]}"}")
    printf "\r  %d/%d done" "$((completed + failed))" "$total"
    [ $remaining -gt 0 ] && sleep "$POLL_INTERVAL"
    attempt=$((attempt + 1))
  done

  echo ""
  if [ $remaining -gt 0 ]; then
    echo "  $(red "Warning:") $remaining jobs timed out"
  fi

  SEED_COMPLETED=$completed
  SEED_FAILED=$failed
}

# ── Main ─────────────────────────────────────────────────

main() {
  echo ""
  echo "$(bold "ReasonDB Docs Seeder")"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""

  check_deps
  health_check
  echo ""

  # Clean re-seed: delete existing table if present
  local existing_id
  existing_id="$(find_existing_table)"
  if [ -n "$existing_id" ]; then
    delete_table "$existing_id"
  fi

  # Create fresh table (capture only the last line = table_id)
  local table_id
  table_id="$(create_table | tail -1)"
  echo ""

  # Collect .mdx files
  local files=()
  while IFS= read -r -d '' f; do
    files+=("$f")
  done < <(find "$DOCS_DIR" -name '*.mdx' -not -path '*/node_modules/*' -print0 | sort -z)

  if [ ${#files[@]} -eq 0 ]; then
    echo "$(red "Error:") No .mdx files found in $DOCS_DIR/"
    exit 1
  fi

  echo "Ingesting $(bold "${#files[@]}") docs into table $(bold "$table_id") (batch) ..."
  echo ""

  local job_ids_output
  job_ids_output="$(ingest_batch "$table_id" "${files[@]}")"

  local job_ids=()
  while IFS= read -r jid; do
    [[ -n "$jid" ]] && job_ids+=("$jid")
  done <<< "$job_ids_output"

  echo ""

  # Poll all jobs concurrently
  SEED_COMPLETED=0
  SEED_FAILED=0
  if [ ${#job_ids[@]} -gt 0 ]; then
    poll_all_jobs "${job_ids[@]}"
  fi
  echo ""

  # Write titles manifest for test script reference
  local manifest_dir
  manifest_dir="$(cd "$(dirname "$0")" && pwd)/results"
  mkdir -p "$manifest_dir"
  local manifest_file="$manifest_dir/seeded-titles.txt"
  : > "$manifest_file"
  for filepath in "${files[@]}"; do
    echo "$(derive_title "$filepath")" >> "$manifest_file"
  done
  sort -o "$manifest_file" "$manifest_file"

  # Summary
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "$(bold "Done!")"
  echo "  Table:     $TABLE_NAME ($table_id)"
  echo "  Slug:      $TABLE_SLUG"
  echo "  Documents: ${#job_ids[@]} ingested"
  if [ "${SEED_FAILED:-0}" -gt 0 ]; then
    echo "  Failed:    $(red "$SEED_FAILED")"
  fi
  echo "  Manifest:  $manifest_file"
  echo ""
  echo "Run queries:  $(bold "make test-queries")"
  echo ""
}

main "$@"
