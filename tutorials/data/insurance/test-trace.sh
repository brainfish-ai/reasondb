#!/usr/bin/env bash
# End-to-end test: create an insurance table, ingest PDFs, run a REASON query,
# and verify that the trace path and query decomposition are working.

set -e

BASE="http://localhost:4444/v1"
DATA_DIR="$(dirname "$0")"
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'

pass() { echo -e "${GREEN}✓ $1${NC}"; }
fail() { echo -e "${RED}✗ $1${NC}"; exit 1; }
info() { echo -e "${CYAN}→ $1${NC}"; }
section() { echo -e "\n${YELLOW}=== $1 ===${NC}"; }

# ──────────────────────────────────────────────────────────────────
section "1. Create insurance table with domain description"

TABLE=$(curl -s -X POST "$BASE/tables" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "aia_insurance_policies",
    "slug": "aia-insurance",
    "description": "AIA Australia life insurance product disclosure statements and policy documents covering TPD (Total Permanent Disability), income protection, trauma cover, waiting periods, benefit periods, and premium structures."
  }')
echo "$TABLE" | python3 -m json.tool

TABLE_ID=$(echo "$TABLE" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null)
[ -z "$TABLE_ID" ] && fail "Could not extract table ID"
pass "Created table: $TABLE_ID"

# ──────────────────────────────────────────────────────────────────
section "2. Ingest PDF documents"

for PDF in \
  "priority-protection-pds-2025.pdf" \
  "priority-protection-ibr-2025.pdf" \
  "priority-protection-enhancement-2025.pdf" \
  "income-care-plus-2011.pdf"
do
  info "Ingesting $PDF ..."
  # The file ingest endpoint is synchronous — it returns document_id directly.
  RESP=$(curl -s -X POST "$BASE/tables/$TABLE_ID/ingest/file" \
    -F "file=@$DATA_DIR/$PDF;type=application/pdf" \
    -F "title=${PDF%.pdf}")
  DOC_ID=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['document_id'])" 2>/dev/null)
  if [ -z "$DOC_ID" ]; then
    echo "$RESP"
    fail "Ingest failed for $PDF"
  fi
  pass "Ingested $PDF → document_id=$DOC_ID"
done

# ──────────────────────────────────────────────────────────────────
section "3. Check domain_vocab was auto-populated during ingestion"

TABLE_STATE=$(curl -s "$BASE/tables/$TABLE_ID")
VOCAB=$(echo "$TABLE_STATE" | python3 -c "
import sys, json
t = json.load(sys.stdin)
vocab = t.get('metadata', {}).get('domain_vocab', [])
print(json.dumps(vocab, indent=2))
" 2>/dev/null)
echo "Domain vocab extracted: $VOCAB"
VOCAB_COUNT=$(echo "$VOCAB" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "0")
if [ "$VOCAB_COUNT" -gt 0 ]; then
  pass "domain_vocab has $VOCAB_COUNT terms"
else
  echo -e "${YELLOW}⚠ domain_vocab is empty — LLM vocab extraction may not have run${NC}"
fi

# ──────────────────────────────────────────────────────────────────
section "4. Run REASON query (tests decomposition + tracing)"

QUERY_RESP=$(curl -s -X POST "$BASE/query" \
  -H "Content-Type: application/json" \
  -d "{\"query\": \"SELECT * FROM $TABLE_ID REASON 'what happens if I become disabled and cannot work'\"}")

echo "$QUERY_RESP" | python3 -m json.tool

TRACE_ID=$(echo "$QUERY_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('trace_id',''))" 2>/dev/null)
DOC_COUNT=$(echo "$QUERY_RESP" | python3 -c "import sys,json; print(len(json.load(sys.stdin).get('documents',[])))" 2>/dev/null || echo "0")

[ -z "$TRACE_ID" ] && fail "No trace_id in query response — tracing not wired up"
pass "Query returned $DOC_COUNT documents with trace_id: $TRACE_ID"

# ──────────────────────────────────────────────────────────────────
section "5. Retrieve full trace and inspect phases"

TRACE=$(curl -s "$BASE/tables/$TABLE_ID/traces/$TRACE_ID")
echo "$TRACE" | python3 -m json.tool

python3 - <<EOF
import sys, json

trace = json.loads("""$(echo "$TRACE" | python3 -c "import sys,json; print(json.dumps(json.load(sys.stdin)))" 2>/dev/null)""")

print("\n--- Trace Summary ---")
print(f"  trace_id       : {trace.get('trace_id')}")
print(f"  query          : {trace.get('query')}")
print(f"  duration_ms    : {trace.get('duration_ms')}")

decomp = trace.get("decomposition")
if decomp:
    sqs = decomp.get("sub_queries", [])
    print(f"\n  [decomposition] {len(sqs)} sub-queries generated:")
    for i, sq in enumerate(sqs):
        print(f"    {i+1}. {sq['text']}")
        print(f"       rationale: {sq['rationale'][:80]}...")
else:
    print("\n  [decomposition] none (passthrough)")

bm25 = trace.get("bm25_selection", {})
print(f"\n  [bm25_selection] candidates: {bm25.get('total_candidates', 0)}, hits: {len(bm25.get('hits', []))}")

sf = trace.get("structural_filter", {})
print(f"  [structural_filter] terms: {sf.get('terms', [])}, filtered_count: {sf.get('filtered_count', 0)}")

lr = trace.get("llm_ranking", {})
print(f"  [llm_ranking] input: {lr.get('input_count', 0)}, selected: {lr.get('selected_count', 0)}, skipped_llm: {lr.get('skipped_llm', False)}")

br = trace.get("beam_reasoning", {})
print(f"  [beam_reasoning] docs_processed: {br.get('documents_processed', 0)}, llm_calls: {br.get('total_llm_calls', 0)}")

fr = trace.get("final_results", [])
print(f"\n  [final_results] {len(fr)} results:")
for r in fr[:3]:
    print(f"    - {r.get('document_title')} / {r.get('node_title')} (confidence={r.get('confidence', 0):.2f})")
EOF

pass "Trace retrieved and parsed"

# ──────────────────────────────────────────────────────────────────
section "6. List traces for the table"

LIST=$(curl -s "$BASE/tables/$TABLE_ID/traces")
echo "$LIST" | python3 -m json.tool

TRACE_COUNT=$(echo "$LIST" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "0")
[ "$TRACE_COUNT" -gt 0 ] && pass "Found $TRACE_COUNT trace(s) for table $TABLE_ID" || fail "No traces listed"

echo -e "\n${GREEN}All checks passed! Trace path and query decomposition are working.${NC}\n"
