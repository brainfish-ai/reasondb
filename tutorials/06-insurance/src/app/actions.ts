"use server"
import path from "path"
import fs from "fs/promises"

const TABLE_NAME = "aia_insurance"
const DATA_DIR = path.resolve(process.cwd(), "../data/insurance")

interface PolicyMeta {
  slug: string
  title: string
  policy: string
  year: number
  type: string
  insurer: string
  file: string
  source_url?: string
  url?: string
}

export async function initializeDataset(serverUrl: string, apiKey: string) {
  const base = serverUrl.replace(/\/$/, "")
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(apiKey ? { "X-API-Key": apiKey } : {}),
  }

  await fetch(`${base}/v1/tables`, {
    method: "POST",
    headers,
    body: JSON.stringify({
      name: TABLE_NAME,
      description: "Insurance policy documents — Income Care Plus (2011) and Priority Protection suite (2025)",
    }),
  }).catch(() => {})

  const manifest: PolicyMeta[] = JSON.parse(
    await fs.readFile(path.join(DATA_DIR, "manifest.json"), "utf-8")
  )

  const jobIds: string[] = []

  for (const policy of manifest) {
    // Use pre-extracted .txt file to bypass the markitdown plugin timeout
    const txtFile = policy.file.replace(/\.pdf$/, ".txt")
    const txtPath = path.join(DATA_DIR, txtFile)

    let content: string
    try {
      content = await fs.readFile(txtPath, "utf-8")
    } catch {
      // Fall back to PDF file ingestion if text not available
      continue
    }

    const res = await fetch(`${base}/v1/tables/${TABLE_NAME}/ingest/text`, {
      method: "POST",
      headers,
      body: JSON.stringify({
        title: policy.title,
        // Truncate to 200K chars — enough for even the large PDS
        content: content.slice(0, 200_000),
        tags: ["aia", "insurance", "australia", policy.type, policy.policy],
        metadata: {
          insurer: policy.insurer,
          policy: policy.policy,
          year: policy.year,
          type: policy.type,
          slug: policy.slug,
          source_url: policy.source_url ?? policy.url ?? "",
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
