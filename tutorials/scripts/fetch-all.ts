/**
 * ReasonDB Tutorials — Phase 1 Data Acquisition
 *
 * Downloads all real-world datasets needed for the tutorial apps.
 * Run from the tutorials/ directory:
 *
 *   npx tsx scripts/fetch-all.ts
 *
 * Output:
 *   data/books/        — Project Gutenberg classic novels (.txt)
 *   data/opinions/     — CourtListener SCOTUS opinions (.txt + .meta.json)
 *   data/papers/       — ArXiv ML paper PDFs (.pdf)
 *   data/wiki/         — Wikipedia ML articles (.txt)
 *   data/financials/   — SEC EDGAR 10-K annual reports (.txt)
 */

import { fetchBooks } from "./fetchers/gutenberg.js";
import { fetchOpinions } from "./fetchers/courtlistener.js";
import { fetchPapers } from "./fetchers/arxiv.js";
import { fetchWikiArticles } from "./fetchers/wikipedia.js";
import { fetchFinancials } from "./fetchers/sec-edgar.js";
import { fetchAiaInsurance } from "./fetchers/insurance.js";

async function main() {
  console.log("ReasonDB Tutorials — Data Acquisition");
  console.log("======================================");

  const start = Date.now();

  await fetchBooks();
  await fetchOpinions();
  await fetchPapers();
  await fetchWikiArticles();
  await fetchFinancials();
  await fetchAiaInsurance();

  const elapsed = ((Date.now() - start) / 1000).toFixed(1);
  console.log(`\n✅ All done in ${elapsed}s`);
  console.log("   Data is in tutorials/data/ (gitignored)");
}

main().catch((err) => {
  console.error("\n❌ Fatal error:", err);
  process.exit(1);
});
