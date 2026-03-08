<br>

<p align="center">
    <img src="./assets/logo.svg" alt="ReasonDB" width="140" />
</p>

<h3 align="center">
    <strong>AI-Native Document Intelligence</strong>
</h3>

<p align="center">
    The database that understands your documents.<br/>
    Built for AI agents that need to reason, not just retrieve.
</p>

<br>

<p align="center">
    <a href="https://github.com/reasondb/reasondb/releases"><img src="https://img.shields.io/github/v/release/reasondb/reasondb?color=6C5CE7&include_prereleases&label=version&sort=semver&style=flat-square" alt="Version"></a>
    &nbsp;
    <a href="https://github.com/reasondb/reasondb"><img src="https://img.shields.io/badge/built_with-Rust-dca282.svg?style=flat-square" alt="Built with Rust"></a>
    &nbsp;
    <a href="https://github.com/reasondb/reasondb/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/reasondb/reasondb/ci.yml?style=flat-square&branch=main&label=CI" alt="CI"></a>
    &nbsp;
    <a href="https://github.com/reasondb/reasondb/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-ReasonDB_v1.0-00bfff.svg?style=flat-square" alt="License"></a>
</p>

<p align="center">
    <a href="https://hub.docker.com/r/brainfishai/reasondb"><img src="https://img.shields.io/docker/pulls/brainfishai/reasondb?label=docker%20pulls&style=flat-square" alt="Docker Pulls"></a>
    &nbsp;
    <a href="https://github.com/reasondb/reasondb"><img src="https://img.shields.io/github/stars/reasondb/reasondb?color=FFD700&style=flat-square&label=stars" alt="GitHub Stars"></a>
    &nbsp;
    <a href="https://github.com/reasondb/reasondb"><img src="https://img.shields.io/github/downloads/reasondb/reasondb/total?color=8259dd&label=downloads&style=flat-square" alt="Downloads"></a>
</p>

<p align="center">
    <a href="https://reason-db.devdoc.sh">Docs</a>
    &nbsp;&bull;&nbsp;
    <a href="https://reason-db.devdoc.sh/documentation/page/quickstart">Quick Start</a>
    &nbsp;&bull;&nbsp;
    <a href="https://reason-db.devdoc.sh/api-reference/introduction">API Reference</a>
</p>

<p align="center">
    <sub>&#9888;&#65039; <strong>Alpha Release</strong> &mdash; ReasonDB is under active development. APIs and features may change. We'd love your <a href="https://github.com/reasondb/reasondb/issues">feedback</a>!</sub>
</p>

<br>

<img src="./assets/ReasonDB_Client_Demo.gif" alt="ReasonDB Client Demo" width="100%">

<br>

<h2>What is ReasonDB?</h2>

ReasonDB is an AI-native document database built in Rust, designed to go beyond simple retrieval. While traditional databases and vector stores treat documents as data to be indexed, ReasonDB treats them as **knowledge to be understood** - preserving document structure, enabling LLM-guided traversal, and extracting precise answers with full context.

ReasonDB introduces **Hierarchical Reasoning Retrieval (HRR)**, a fundamentally new architecture where the LLM doesn't just consume retrieved content - it actively navigates your document structure to find exactly what it needs, like a human expert scanning summaries, drilling into relevant sections, and synthesizing answers.

> **ReasonDB is not another vector database.** It's a reasoning engine that preserves document hierarchy, enabling AI to traverse your knowledge the way a domain expert would.

**Key features of ReasonDB include:**

- **Hierarchical Reasoning Retrieval**: LLM-guided tree traversal with parallel beam search - AI navigates document structure instead of relying on similarity matching
- **RQL Query Language**: SQL-like syntax with built-in `SEARCH` (BM25) and `REASON` (LLM) clauses in a single query
- **Plugin Architecture**: Extensible extraction pipeline - PDF, Office, images, audio, and URLs out of the box via [MarkItDown](https://github.com/microsoft/markitdown)
- **Multi-Provider LLM Support**: Anthropic, OpenAI, Gemini, Cohere, Vertex AI, AWS Bedrock, and more — switch providers without code changes
- **Production Ready**: ACID-compliant storage, API key auth, rate limiting, async parallel traversal - all in a single Rust binary

<h2>Contents</h2>

- [What is ReasonDB?](#what-is-reasondb)
- [The Problem](#the-problem)
- [Benchmark](#benchmark)
- [Insurance Policy Analyser — Live Demo](#insurance-policy-analyser--live-demo)
- [How It Works](#how-it-works)
- [Quick Start](#quick-start)
- [Interactive Tutorials](#interactive-tutorials)
- [Query with RQL](#query-with-rql)
- [Plugin Architecture](#plugin-architecture)
- [Use Cases](#use-cases)
- [Tech Stack](#tech-stack)
- [Documentation](#documentation)
- [Community](#community)
- [Contributing](#contributing)
- [License](#license)

<h2>The Problem</h2>

AI agents today are limited by their databases:

| Approach | What It Does | Why It Fails |
|----------|--------------|--------------|
| **Vector DBs** | Finds "similar" chunks | Loses structure. A contract's termination clause isn't "similar" to your question about exit terms - but it's the answer. |
| **RAG Pipelines** | Retrieves then generates | Garbage in, garbage out. Wrong chunks retrieved means wrong answers, no matter how capable the LLM. |
| **Knowledge Graphs** | Maps explicit relationships | Requires manual entity extraction. Can't handle the messy reality of real documents. |

**The result?** AI agents that hallucinate, miss critical context, or drown in irrelevant chunks.

ReasonDB solves this by letting the LLM **reason through** your documents - not just search them.

<h2>Benchmark</h2>

Results on a real-world insurance document corpus (4 policy documents, ~1,900 nodes, 12 queries across 6 complexity tiers). Full benchmark script: [`tutorials/data/insurance/benchmark.py`](./tutorials/data/insurance/benchmark.py).

### Retrieval quality vs. typical RAG

| Metric | ReasonDB | Typical RAG |
|---|---|---|
| **Pass rate** | **100%** (12 / 12) | 55 – 70% |
| **Context recall** (term match) | **90%** avg | 60 – 75% |
| **Median latency** (RQL `REASON`) | **6.1 s** | 15 – 45 s |

> "Typical RAG" = chunked-retrieval pipelines (LlamaIndex / LangChain defaults) on the same corpus. ReasonDB uses BM25 candidate selection + LLM-guided hierarchical tree traversal instead of flat similarity matching.

### Per-category breakdown

| Category | Avg latency | Term recall | Pass |
|---|---|---|---|
| Simple | 7.1 s | 100% | 2 / 2 |
| Specific | 5.9 s | 75% | 2 / 2 |
| Multi-condition | 5.6 s | 83% | 2 / 2 |
| Comparative | 6.2 s | 100% | 2 / 2 |
| Multi-hop | 6.5 s | 83% | 2 / 2 |
| Synthesis | 6.5 s | 100% | 2 / 2 |

### Cross-section reference retrieval

ReasonDB detects and follows **intra-document cross-references** during ingestion (LLM-extracted during summarization) and surfaces the referenced sections alongside primary results. This closes the "answer is split across two clauses" gap that defeats flat-chunk retrieval.

| Metric | Value |
|---|---|
| Queries with ≥ 1 cross-ref surfaced | **4 / 5** |
| Avg recall, primary content only | 62% |
| Avg recall, primary + cross-refs | **80%** (+18 pp) |
| Example gain | Recurrent disability query: 67% → **100%** once cross-referenced policy schedule clause is included |

### Insurance Policy Analyser — Live Demo

The benchmark above is powered by this tutorial app. It queries four insurance policy documents using `REASON` and shows the full traversal trace — which nodes the LLM visited, why it selected them, and how it synthesized the final answer.

<img src="./assets/Insurance_Policy_Analyser_Demo.gif" alt="Insurance Policy Analyser Demo" width="100%">

> Full tutorial source: [`tutorials/06-insurance/`](./tutorials/06-insurance/)

<h2>How It Works</h2>

```mermaid
%%{init: {'theme': 'dark'}}%%
flowchart TD
    subgraph Ingestion["Ingestion Pipeline (Plugin-Driven)"]
        A["Documents / URLs"] -->|Extractor Plugin| B["Markdown"]
        B -->|Post-Processor Plugin| C["Cleaned Markdown"]
        C -->|Chunker| D["Semantic Chunks"]
        D --> E["Build Hierarchical Tree"]
        E -->|Bottom-up| F["LLM Summarizes Each Node"]
    end

    subgraph Search["Search & Reasoning"]
        G["Natural Language Query"] --> G1["BM25 Candidates + Title Boost"]
        G1 --> G2["Recursive Tree-Grep Pre-Filter"]
        G2 --> H["LLM Ranks by Summaries + Match Signals"]
        H -->|Selects relevant branches| I["Traverse Tree"]
        I -->|Parallel beam search| J["Drill Into Leaf Nodes"]
    end

    subgraph Result["Response"]
        J --> K["Extract Answer"]
        K --> L["Confidence Score + Reasoning Path"]
    end

    Ingestion --> Search
```

1. **Extract** - Extractor plugins convert documents and URLs to Markdown (built-in: [MarkItDown](https://github.com/microsoft/markitdown))
2. **Chunk** - Content is split into semantic chunks with heading detection
3. **Build Tree** - Chunks are organized into a hierarchical tree structure
4. **Summarize** - LLM generates summaries for each node (bottom-up)
5. **Search** - 4-phase pipeline: BM25 candidate selection → recursive tree-grep filtering → LLM summary ranking → parallel beam-search traversal
6. **Return** - Relevant content with extracted answers, confidence scores, and the full reasoning path

<h2>Quick Start</h2>

Get from zero to intelligent document search in under 5 minutes.

#### Download pre-built binaries

Grab the [latest release](https://github.com/reasondb/reasondb/releases/latest) for your platform:

| Platform | Architecture | Download |
|----------|-------------|----------|
| **macOS** | Apple Silicon (M1/M2/M3/M4) | [aarch64-apple-darwin](https://github.com/reasondb/reasondb/releases/latest) |
| **macOS** | Intel | [x86_64-apple-darwin](https://github.com/reasondb/reasondb/releases/latest) |
| **Linux** | x86_64 | [x86_64-unknown-linux-gnu](https://github.com/reasondb/reasondb/releases/latest) |
| **Linux** | ARM64 | [aarch64-unknown-linux-gnu](https://github.com/reasondb/reasondb/releases/latest) |
| **Windows** | x86_64 | [x86_64-pc-windows-msvc](https://github.com/reasondb/reasondb/releases/latest) |

> **ReasonDB Client:** A desktop app is also available for [macOS (.dmg) and Windows (.msi)](https://github.com/reasondb/reasondb/releases/latest).

<details>
<summary><strong>macOS: "ReasonDB.app" Not Opened</strong></summary>

<br>

Since ReasonDB is in alpha, the desktop app is not yet signed with an Apple Developer certificate. macOS Gatekeeper will block it on first launch. To open it:

1. **Right-click** (or Control-click) on `ReasonDB.app` and select **Open**
2. Click **Open** in the confirmation dialog

Or remove the quarantine attribute from the terminal:

```bash
xattr -cr /Applications/ReasonDB.app
```

You can also go to **System Settings → Privacy & Security**, scroll down, and click **Open Anyway** next to the ReasonDB message.

</details>

#### Install with Homebrew (macOS / Linux)

```bash
brew tap reasondb/tap
brew install reasondb
```

#### Install from source

```bash
git clone https://github.com/reasondb/reasondb.git && cd reasondb
cargo build --release
```

To also set up the desktop app and tutorials, install JS dependencies from the repo root:

```bash
yarn install          # installs apps/, packages/, and tutorials/ in one step
yarn build:packages   # builds the shared @reasondb/rql-editor package
```

#### Configure your LLM provider

```bash
reasondb config init
```

| Variable | Description | Required |
|---|---|---|
| `REASONDB_LLM_PROVIDER` | `openai`, `anthropic`, `gemini`, `cohere`, `glm`, `kimi`, `ollama`, `vertex`, `bedrock` | Yes |
| `REASONDB_LLM_API_KEY` | API key for the chosen provider | Yes |
| `REASONDB_MODEL` | Override the default model for the provider | No |

#### Start the server

```bash
reasondb serve
```

Server starts at **http://localhost:4444** with Swagger UI at **http://localhost:4444/swagger-ui/**

#### Run using Docker

```bash
docker run --rm --pull always --name reasondb -p 4444:4444 \
  -e REASONDB_LLM_PROVIDER=openai \
  -e REASONDB_LLM_API_KEY=sk-... \
  brainfishai/reasondb:latest serve
```

Or use the Makefile for local development:

```bash
make docker-up        # Build and start
make docker-up-d      # Start in background
make docker-logs      # View logs
make docker-down      # Stop containers
make docker-down-v    # Stop and remove data volume
make docker-ps        # Check health status
```

#### ReasonDB Client (Desktop App)

The repo uses a **Yarn workspace** — a single `yarn install` at the root installs all JS dependencies across `apps/`, `packages/`, and `tutorials/`.

```bash
# One-time setup from repo root
yarn install
yarn build:packages   # build the shared @reasondb/rql-editor package

# Run the desktop app
make client-app          # dev mode (Tauri + Vite)
make client-app-build    # production build

# Or use yarn workspace commands directly
yarn workspace reasondb-client dev         # Vite web dev server only
yarn workspace reasondb-client tauri dev   # full Tauri desktop app (dev)
yarn workspace reasondb-client build       # production build
```

#### Interactive Tutorials

Six hands-on tutorial apps are included, each running a Next.js app against a live ReasonDB server:

| Tutorial | Workspace name | Port |
|----------|---------------|------|
| 01 — RQL Basics | `tutorial-rql-basics` | 5000 |
| 02 — Legal Search | `tutorial-legal-search` | 5001 |
| 03 — Research Papers | `tutorial-research-papers` | 5002 |
| 04 — Knowledge Base | `tutorial-knowledge-base` | 5003 |
| 05 — PDF Financials | `tutorial-pdf-financials` | 5004 |
| 06 — Insurance Analyser | `tutorial-insurance-demo` | 5005 |

```bash
# Start any tutorial by its workspace name
yarn workspace tutorial-rql-basics dev
yarn workspace tutorial-insurance-demo dev

# Fetch sample data for all tutorials
yarn tutorials:fetch-all
```

> All tutorials require a running ReasonDB server — start one with `reasondb serve` before launching a tutorial.

<h2>Query with RQL</h2>

ReasonDB uses **RQL** - a SQL-like query language with built-in `SEARCH` and `REASON` clauses.

Here's ReasonDB answering a question **about itself** — the README ingested as a document:

```sql
SELECT * FROM docs REASON 'What is ReasonDB?';
```

```json
{
  "documents": [
    {
      "title": "ReasonDB README",
      "score": 0.97,
      "matched_nodes": [
        {
          "title": "What is ReasonDB?",
          "content": "ReasonDB is an AI-native document database built in Rust, designed to go beyond simple retrieval. It treats documents as knowledge to be understood — preserving document structure, enabling LLM-guided traversal, and extracting precise answers with full context.",
          "path": ["ReasonDB README", "What is ReasonDB?"],
          "confidence": 0.97,
          "reasoning_trace": [
            {
              "node_title": "ReasonDB README",
              "decision": "Introduction section directly addresses the query",
              "confidence": 0.91
            },
            {
              "node_title": "What is ReasonDB?",
              "decision": "Node title matches query exactly — drilling into content",
              "confidence": 0.97
            }
          ]
        }
      ]
    }
  ],
  "total_count": 1,
  "execution_time_ms": 4213
}
```

Every answer includes the **matched node content**, the **path** through the document tree the LLM traversed, a **reasoning trace** explaining each navigation decision, and a **confidence score**. No black-box retrieval — full transparency.

```sql
-- Fast keyword search (BM25, ~50ms)
SELECT * FROM contracts SEARCH 'payment terms' LIMIT 5;

-- LLM-guided reasoning (navigates the document tree)
SELECT * FROM contracts REASON 'What are the late fees and penalties?';

-- Combine filters, search, and reasoning in one query
SELECT * FROM contracts
WHERE tags CONTAINS ANY ('nda') AND metadata.value_usd > 10000
SEARCH 'termination clause'
REASON 'What are the exit conditions?'
LIMIT 5;
```

<details>
<summary><strong>Compare: Vector DB vs ReasonDB</strong></summary>

<br>

**Vector DB Approach**
```
Query: "What are the termination conditions?"

→ Embed query as vector
→ Find 5 "similar" chunks
→ Hope one contains the answer

Result: Random paragraphs mentioning "termination"
        scattered across the document. No context.
        LLM hallucinates missing details.
```

**ReasonDB Approach**
```
Query: "What are the termination conditions?"

→ LLM reads document summary
→ Identifies "Section 8: Termination" as relevant
→ Navigates to section, reads subsection summaries
→ Drills into "8.2 Conditions for Termination"
→ Extracts complete answer with full context

Result: Precise answer citing specific clauses,
        with confidence score and reasoning path.
```

</details>

<h2>Plugin Architecture</h2>

ReasonDB uses a **plugin system** for document extraction. Plugins are external processes (Python, Node.js, Bash, or compiled binaries) that communicate via JSON over stdin/stdout.

| What ships out of the box | What you can add |
|---------------------------|------------------|
| **markitdown** - PDF, Word, Excel, PowerPoint, HTML, images (OCR), audio, YouTube, and more | Custom extractors, post-processors, chunkers, summarizers |

```bash
# List installed plugins
curl http://localhost:4444/v1/plugins

# Test a plugin
curl -X POST http://localhost:4444/v1/plugins/markitdown/test \
  -H "Content-Type: application/json" \
  -d '{"operation":"extract","params":{"source_type":"file","path":"/tmp/doc.pdf"}}'
```

Community plugins can be installed by dropping a directory into `$REASONDB_PLUGINS_DIR` (default: `./plugins`). See the [Plugin Guide](https://reason-db.devdoc.sh/documentation/page/guides/plugins) for details.

<h2>Use Cases</h2>

- **Legal Document Analysis** - Navigate complex contracts, find specific clauses, compare terms across agreements
- **Research & Knowledge Management** - Build searchable knowledge bases from papers, reports, and documentation
- **Customer Support Intelligence** - Transform support docs into an AI agent that finds precise answers
- **Compliance & Policy** - Query policy documents in natural language with full section references
- **AI Agent Data Layer** - Give your agents structured access to unstructured knowledge with reasoning capabilities

<h2>Tech Stack</h2>

| Component | Technology | Purpose |
|-----------|-----------|---------|
| **Storage** | [redb](https://github.com/cberner/redb) | Pure Rust, ACID-compliant embedded database |
| **Search** | [tantivy](https://github.com/quickwit-oss/tantivy) | Blazing fast BM25 full-text search |
| **Extraction** | Plugin System | Process-based plugins (Python, Node.js, Bash, binaries) |
| **Runtime** | [tokio](https://tokio.rs) | Async parallel branch exploration |
| **HTTP** | [axum](https://github.com/tokio-rs/axum) | Fast, ergonomic web framework |
| **LLM** | [rig-core](https://github.com/0xPlaygrounds/rig) | Multi-provider LLM abstraction |
| **API Docs** | [utoipa](https://github.com/juhaku/utoipa) | OpenAPI 3.0 + Swagger UI |
| **Container** | Docker (Alpine) | Python 3, Node.js, and Bash runtimes |

<h2>Documentation</h2>

| Resource | Link |
|----------|------|
| Full Documentation | [reason-db.devdoc.sh](https://reason-db.devdoc.sh) |
| Quick Start Guide | [reason-db.devdoc.sh/documentation/page/quickstart](https://reason-db.devdoc.sh/documentation/page/quickstart) |
| Core Concepts | [reason-db.devdoc.sh/documentation/page/concepts](https://reason-db.devdoc.sh/documentation/page/concepts) |
| **Contributing** | [Contributing guide](CONTRIBUTING.md) · [docs](https://reason-db.devdoc.sh/documentation/page/guides/contributing) |
| Plugin Guide | [reason-db.devdoc.sh/documentation/page/guides/plugins](https://reason-db.devdoc.sh/documentation/page/guides/plugins) |
| API Reference | [reason-db.devdoc.sh/api-reference](https://reason-db.devdoc.sh/api-reference/introduction) |
| Swagger UI | [localhost:4444/swagger-ui](http://localhost:4444/swagger-ui/) *(when server is running)* |

<h2>Community</h2>

Join our growing community for help, ideas, and discussions regarding ReasonDB.

- View our [Blog](https://dev.to/ajainvivek/why-dont-databases-understand-documents-1hk3)
- Star us on [GitHub](https://github.com/reasondb/reasondb)

<h2>Contributing</h2>

We’d love your help. See the **[contributing guide](CONTRIBUTING.md)** for development setup, running tests, and how to submit PRs. You can also read it in the [docs](https://reason-db.devdoc.sh/documentation/page/guides/contributing).

<h2>License</h2>

ReasonDB is source-available under the [ReasonDB License v1.0](./LICENSE).

**You can:**
- Use ReasonDB for any purpose (commercial or non-commercial)
- Modify the source code
- Distribute copies and derivative works
- Use in your own products and services

**You cannot:**
- Offer ReasonDB as a hosted/managed database service (DBaaS)
- Provide ReasonDB's functionality as a service to third parties

Copyright (c) 2026-present [Brainfish Pty Ltd](https://brainfi.sh) — For commercial licensing, please [contact us](mailto:ajain@brainfi.sh).
