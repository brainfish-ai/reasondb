# 🧠 ReasonDB

> **A database that thinks, not just calculates.**

ReasonDB is a reasoning-native database optimized for AI agent workflows. Unlike Vector DBs (mathematical similarity) or SQL DBs (relational algebra), ReasonDB optimizes for **tree traversal** and **LLM-driven context management**.

## 🎯 Key Features

- **Hierarchical Document Storage**: Documents stored as navigable trees, not flat chunks
- **LLM-Guided Retrieval**: AI reasons through the tree structure, not just similarity search
- **Document Relationships**: Link documents with references, citations, and follow-ups
- **RQL Query Language**: SQL-like syntax with SEARCH, REASON, and RELATED TO clauses
- **BM25 Full-Text Search**: Fast keyword search using Tantivy
- **Parallel Branch Exploration**: Concurrent traversal using Rust's async runtime
- **Multi-Format Support**: PDFs, Markdown, HTML, and more (via MarkItDown)
- **Multi-Provider LLM Support**: OpenAI, Anthropic Claude, Google Gemini, Cohere
- **REST API**: Full HTTP API with Swagger UI documentation
- **CLI Tool**: Command-line interface with interactive RQL REPL

## 🚀 Quick Start

### Prerequisites

- Rust 1.70+
- An LLM API key (Anthropic, OpenAI, Gemini, or Cohere)

### Build & Configure

```bash
# Build
cargo build --release

# Configure your LLM API key (stored securely in ~/.config/reasondb/config.toml)
cargo run --bin reasondb -- config set llm.provider anthropic
cargo run --bin reasondb -- config set llm.api_key sk-ant-xxxxx

# Or use the interactive wizard
cargo run --bin reasondb -- config init
```

### Start the Server

```bash
# Start server (picks up config automatically)
cargo run --bin reasondb -- serve
```

Server starts at **http://localhost:4444** with Swagger UI at **http://localhost:4444/swagger-ui/**

<details>
<summary>Alternative: Using environment variables</summary>

```bash
# Set API key via environment
ANTHROPIC_API_KEY=sk-ant-xxxxx cargo run --bin reasondb-server

# Or with OpenAI
OPENAI_API_KEY=sk-xxxxx cargo run --bin reasondb-server
```

</details>

### CLI Usage

```bash
# Configuration management (like psql/git config)
reasondb config init                    # Interactive setup
reasondb config set llm.api_key xxx     # Set API key
reasondb config list                    # View all settings

# Start the server
reasondb serve --port 4444

# Interactive RQL REPL
reasondb query

# Execute a single query
reasondb query -q "SELECT * FROM contracts WHERE author = 'Alice'"

# Manage tables
reasondb tables list
reasondb tables create my_table --description "My documents"

# Manage documents
reasondb docs list --table contracts
reasondb docs ingest "My Document" --file ./doc.md --table contracts

# Search documents
reasondb search "payment terms" --table contracts

# Import/Export data
reasondb import ./documents.json --table contracts
reasondb export ./backup.json --table contracts

# Check server health
reasondb health

# Generate shell completions
reasondb completions zsh >> ~/.zshrc
```

### API Examples

#### Ingest a Document

```bash
curl -X POST http://localhost:4444/v1/ingest/text \
  -H "Content-Type: application/json" \
  -d '{
    "title": "AI Fundamentals",
    "content": "# AI Fundamentals\n\nArtificial Intelligence is the simulation of human intelligence..."
  }'
```

Response:
```json
{
  "document_id": "902dae45-4601-4b5d-ae69-71c819713b87",
  "title": "AI Fundamentals",
  "total_nodes": 2,
  "max_depth": 1,
  "stats": {
    "summaries_generated": 2,
    "total_time_ms": 6085
  }
}
```

#### Search with LLM Reasoning

```bash
curl -X POST http://localhost:4444/v1/search \
  -H "Content-Type: application/json" \
  -d '{"query": "What is machine learning?"}'
```

Response:
```json
{
  "results": [{
    "content": "Machine learning is a subset of AI...",
    "answer": "Machine learning is a subset of AI where systems learn from data without explicit programming.",
    "confidence": 0.95
  }],
  "stats": {
    "nodes_visited": 2,
    "llm_calls": 2,
    "total_time_ms": 5141
  }
}
```

#### List Documents

```bash
curl http://localhost:4444/v1/documents
```

#### Get Document Tree

```bash
curl http://localhost:4444/v1/documents/{id}/tree
```

#### Query with RQL

```bash
curl -X POST http://localhost:4444/v1/query \
  -H "Content-Type: application/json" \
  -d '{"query": "SELECT * FROM legal WHERE author = '\''Alice'\'' SEARCH '\''contract'\'' LIMIT 10"}'
```

#### Create Document Relationship

```bash
curl -X POST http://localhost:4444/v1/relations \
  -H "Content-Type: application/json" \
  -d '{
    "from_document_id": "doc_contract",
    "to_document_id": "doc_amendment",
    "relation_type": "references",
    "note": "Amendment to Section 5"
  }'
```

#### Query Related Documents

```bash
curl -X POST http://localhost:4444/v1/query \
  -d '{"query": "SELECT * FROM contracts RELATED TO '\''doc_contract'\''"}'
```

## 📦 Project Structure

```
reasondb/
├── crates/
│   ├── reasondb-core/      # Core library (models, storage, LLM engine)
│   ├── reasondb-ingest/    # Document ingestion pipeline  
│   ├── reasondb-cli/       # Command-line interface
│   └── reasondb-server/    # HTTP API server (axum)
├── PLAN.md                 # Detailed architecture & implementation plan
└── USE_CASES.md            # Use cases & competitive analysis
```

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────┐
│                       ReasonDB                          │
├─────────────────────────────────────────────────────────┤
│   HTTP API (axum)                                       │
│   /ingest  │  /search  │  /documents                    │
├─────────────────────────────────────────────────────────┤
│   Ingestion Pipeline    │    Search Engine              │
│   (Extract → Chunk →    │    (LLM Beam Search           │
│    Summarize → Store)   │     Tree Traversal)           │
├─────────────────────────────────────────────────────────┤
│   LLM Provider Layer                                    │
│   (OpenAI │ Anthropic │ Gemini │ Cohere)               │
├─────────────────────────────────────────────────────────┤
│   Storage Engine (redb)                                 │
│   Nodes Table  │  Documents Table                       │
└─────────────────────────────────────────────────────────┘
```

### How It Works

1. **Ingest**: Documents are parsed and converted into hierarchical trees
2. **Summarize**: LLM generates summaries for each node (bottom-up)
3. **Search**: LLM traverses tree, choosing branches based on summaries
4. **Return**: Relevant content with extracted answers and confidence scores

## 📊 Why ReasonDB?

| Approach | Best For | Limitation |
|----------|----------|------------|
| **Vector DB** | Simple factual queries | Loses structure, "similar" ≠ "relevant" |
| **SQL DB** | Structured data | Can't handle unstructured text |
| **Graph DB** | Relationships | Requires explicit entity extraction |
| **ReasonDB** | Complex reasoning | Optimized for AI agent workflows |

## 🛠️ Tech Stack

- **Storage**: `redb` - Pure Rust, ACID-compliant embedded database
- **Serialization**: `bincode` + `serde` - Fast binary encoding
- **Async Runtime**: `tokio` - Parallel branch exploration
- **HTTP Server**: `axum` - Fast, ergonomic web framework
- **LLM Integration**: `rig-core` - Multi-provider LLM abstraction
- **API Docs**: `utoipa` - OpenAPI 3.0 + Swagger UI

## 📅 Roadmap

- [x] **Phase 1**: Core storage (models, redb, CRUD) ✅
- [x] **Phase 2**: Reasoning engine (LLM trait, beam search) ✅
- [x] **Phase 3**: Ingestion pipeline (chunking, summarization) ✅
- [x] **Phase 4**: HTTP API (axum server, OpenAPI docs) ✅
- [x] **Phase 5A**: Tables & document organization ✅
- [x] **Phase 5B**: RQL query language (SEARCH, REASON, GROUP BY) ✅
- [x] **Phase 5C**: BM25 full-text search (Tantivy) ✅
- [x] **Phase 5D**: Performance (caching, parallel LLM calls) ✅
- [x] **Phase 5E**: Document relationships ✅
- [x] **Phase 5F**: CLI tool with RQL REPL ✅
- [x] **Phase 5G**: Configuration management (PostgreSQL-like) ✅
- [ ] **Phase 6**: Production features (auth, rate limiting, clustering)

## 🔧 Configuration

### Config File (Recommended)

```bash
# Interactive setup
reasondb config init

# Or set values directly
reasondb config set llm.provider anthropic
reasondb config set llm.api_key sk-ant-xxxxx
reasondb config set server.port 4444

# View configuration
reasondb config list
```

Config file location:
- **macOS**: `~/Library/Application Support/reasondb/config.toml`
- **Linux**: `~/.config/reasondb/config.toml`

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `ANTHROPIC_API_KEY` | Anthropic API key | - |
| `OPENAI_API_KEY` | OpenAI API key | - |
| `GOOGLE_API_KEY` | Google Gemini API key | - |
| `COHERE_API_KEY` | Cohere API key | - |
| `REASONDB_PORT` | Server port | 4444 |
| `REASONDB_HOST` | Server host | 127.0.0.1 |
| `REASONDB_PATH` | Database file path | reasondb.redb |

## 📄 Documentation

- [PLAN.md](./PLAN.md) - Detailed architecture and implementation plan
- [USE_CASES.md](./USE_CASES.md) - Real-world use cases and competitive analysis
- [Swagger UI](http://localhost:4444/swagger-ui/) - Interactive API documentation (when server is running)

## 📜 License

MIT OR Apache-2.0
