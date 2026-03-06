import fs from "fs";
import path from "path";
import { execSync } from "child_process";

const OUT_DIR = path.resolve("data/insurance");

// AIA Australia policy PDFs
// Note: URLs with .coredownload.inline.pdf suffix are AEM DAM transformation URLs —
// we strip to the bare PDF path which is more reliable for direct download.
const POLICIES = [
  {
    slug: "income-care-plus-2011",
    title: "AIA Income Care Plus Policy Document",
    url: "https://www.aia.com.au/content/dam/au-wise/en/docs/pds-policy-docs/income-care-plus-policy-doc-nov-2011.pdf",
    policy: "income-care-plus",
    year: 2011,
    type: "policy-document",
    insurer: "AIA Australia",
  },
  {
    slug: "priority-protection-pds-2025",
    title: "AIA Priority Protection Product Disclosure Statement (Nov 2025)",
    url: "https://www.aia.com.au/content/dam/au-wise/en/docs/policy-docs/Priority_Protection_Product_Disclosure_Statement.pdf",
    policy: "priority-protection-pds",
    year: 2025,
    type: "product-disclosure-statement",
    insurer: "AIA Australia",
  },
  {
    slug: "priority-protection-ibr-2025",
    title: "AIA Priority Protection Incorporated by Reference Material (Nov 2025)",
    url: "https://www.aia.com.au/content/dam/au-wise/en/docs/policy-docs/Priority_Protection_Incorporated_By_Reference_Material.pdf",
    policy: "priority-protection-ibr",
    year: 2025,
    type: "incorporated-by-reference",
    insurer: "AIA Australia",
  },
  {
    slug: "priority-protection-enhancement-2025",
    title: "AIA Priority Protection Policy Enhancement Summary (Nov 2025)",
    url: "https://www.aia.com.au/content/dam/au-wise/en/docs/policy-docs/priority-protection-policy-enhancement-summary-nov-25.pdf",
    policy: "priority-protection-enhancement",
    year: 2025,
    type: "policy-enhancement-summary",
    insurer: "AIA Australia",
  },
];

// Browser-like headers to avoid WAF/CDN blocking
const BROWSER_HEADERS = {
  "User-Agent":
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36",
  Accept: "application/pdf,*/*;q=0.9",
  "Accept-Language": "en-AU,en;q=0.9",
  "Accept-Encoding": "gzip, deflate, br",
  Referer: "https://www.aia.com.au/",
  Connection: "keep-alive",
};

async function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function downloadPdf(url: string, outPath: string): Promise<number> {
  console.log(`  Fetching: ${url}`);
  const res = await fetch(url, {
    headers: BROWSER_HEADERS,
    redirect: "follow",
  });
  if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
  const buffer = await res.arrayBuffer();
  fs.writeFileSync(outPath, Buffer.from(buffer));
  return buffer.byteLength;
}

export async function fetchAiaInsurance() {
  console.log("\n🛡️  AIA Australia — Insurance Policy Documents");
  fs.mkdirSync(OUT_DIR, { recursive: true });

  const manifest: object[] = [];

  for (const policy of POLICIES) {
    const outPath = path.join(OUT_DIR, `${policy.slug}.pdf`);
    const metaPath = path.join(OUT_DIR, `${policy.slug}.meta.json`);

    if (fs.existsSync(outPath)) {
      const size = fs.statSync(outPath).size;
      console.log(
        `  ✓ ${policy.slug}.pdf already exists (${(size / 1024).toFixed(0)} KB) — skipping`
      );
      manifest.push({ ...policy, file: `${policy.slug}.pdf` });
      continue;
    }

    try {
      // Be polite — AIA's CDN can be sensitive to rapid requests
      await sleep(1500);
      const bytes = await downloadPdf(policy.url, outPath);
      const meta = {
        ...policy,
        file: `${policy.slug}.pdf`,
        source_url: policy.url,
      };
      fs.writeFileSync(metaPath, JSON.stringify(meta, null, 2), "utf-8");
      console.log(
        `  ✓ Saved ${policy.slug}.pdf (${(bytes / 1024).toFixed(0)} KB) — ${policy.title}`
      );
      manifest.push(meta);
    } catch (err) {
      console.error(`  ✗ Failed to fetch ${policy.slug}: ${err}`);
    }
  }

  fs.writeFileSync(
    path.join(OUT_DIR, "manifest.json"),
    JSON.stringify(manifest, null, 2),
    "utf-8"
  );
  console.log("  ✓ Written manifest.json");

  // Pre-extract PDF text locally so the tutorial can use ingest/text
  // (avoids the markitdown Docker plugin 120s timeout on large PDFs)
  extractTextFromPdfs();
}

function extractTextFromPdfs() {
  const pdfs = fs.readdirSync(OUT_DIR).filter((f) => f.endsWith(".pdf"));
  if (pdfs.length === 0) return;

  const extractScript = path.resolve("scripts/extract-insurance-pdfs.py");
  console.log("\n  Extracting text from PDFs (via markitdown)…");
  try {
    execSync(`python3 "${extractScript}" "${OUT_DIR}"`, { stdio: "inherit" });
  } catch {
    console.log(
      "  ⚠ Text extraction skipped. Ensure markitdown is installed:\n" +
      "    pip install 'markitdown[all]'\n" +
      "  Then re-run: npx tsx scripts/fetch-all.ts"
    );
  }
}
