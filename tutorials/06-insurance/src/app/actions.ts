"use server"
import path from "path"
import fs from "fs/promises"

const TABLE_NAME = "aia_insurance"
const DATA_DIR = path.resolve(process.cwd(), "../data/insurance")

interface InsuranceMeta {
  slug: string
  title: string
  policy: string
  year: number
  type: string
  insurer: string
  file: string
  source_url: string
}

export async function initializeDataset(serverUrl: string, apiKey: string) {
  const base = serverUrl.replace(/\/$/, "")
  const authHeaders: Record<string, string> = apiKey ? { "X-API-Key": apiKey } : {}

  // Create table
  await fetch(`${base}/v1/tables`, {
    method: "POST",
    headers: { "Content-Type": "application/json", ...authHeaders },
    body: JSON.stringify({
      name: TABLE_NAME,
      description: "AIA Australia insurance policy documents",
    }),
  }).catch(() => {})

  const manifest: InsuranceMeta[] = JSON.parse(
    await fs.readFile(path.join(DATA_DIR, "manifest.json"), "utf-8")
  )

  const jobIds: string[] = []

  for (const doc of manifest) {
    // Read the extracted text file (slug.txt)
    const txtFile = path.join(DATA_DIR, `${doc.slug}.txt`)
    let content: string
    try {
      content = await fs.readFile(txtFile, "utf-8")
    } catch {
      // Skip if text file not found
      continue
    }

    const res = await fetch(`${base}/v1/tables/${TABLE_NAME}/ingest/text`, {
      method: "POST",
      headers: { "Content-Type": "application/json", ...authHeaders },
      body: JSON.stringify({
        title: doc.title,
        content,
        tags: ["insurance", "aia-australia", doc.type, doc.policy],
        metadata: {
          slug: doc.slug,
          policy: doc.policy,
          year: doc.year,
          type: doc.type,
          insurer: doc.insurer,
          source_url: doc.source_url,
        },
      }),
    })

    if (res.ok) {
      const job = await res.json()
      if (job.job_id) jobIds.push(job.job_id)
    }
  }

  return { jobIds, count: manifest.length }
}

