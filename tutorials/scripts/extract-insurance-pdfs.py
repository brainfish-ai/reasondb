#!/usr/bin/env python3
"""
Pre-extract AIA policy PDFs to plain text using markitdown.

This runs locally so the tutorial can use ingest/text instead of ingest/file,
avoiding the 120-second markitdown plugin timeout inside the Docker container.

Usage:
    python3 scripts/extract-insurance-pdfs.py data/insurance

Requires:
    pip install 'markitdown[all]'
"""
import sys
import os

def main():
    if len(sys.argv) < 2:
        print("Usage: python3 extract-aia-pdfs.py <data_dir>")
        sys.exit(1)

    data_dir = sys.argv[1]
    if not os.path.isdir(data_dir):
        print(f"Directory not found: {data_dir}")
        sys.exit(1)

    try:
        from markitdown import MarkItDown
    except ImportError:
        print("markitdown not installed. Run: pip install 'markitdown[all]'")
        sys.exit(1)

    # Suppress pydub ffmpeg warning
    import warnings
    warnings.filterwarnings("ignore", category=RuntimeWarning)

    md = MarkItDown()
    pdfs = sorted(f for f in os.listdir(data_dir) if f.endswith(".pdf"))

    if not pdfs:
        print(f"No PDF files found in {data_dir}")
        sys.exit(0)

    for pdf in pdfs:
        txt = pdf.replace(".pdf", ".txt")
        txt_path = os.path.join(data_dir, txt)

        if os.path.exists(txt_path):
            size = os.path.getsize(txt_path)
            print(f"  skip {pdf} -> {txt} already exists ({size // 1024} KB)")
            continue

        pdf_path = os.path.join(data_dir, pdf)
        print(f"  extracting {pdf} ...", end=" ", flush=True)
        try:
            result = md.convert(pdf_path)
            with open(txt_path, "w", encoding="utf-8") as f:
                f.write(result.text_content)
            size = os.path.getsize(txt_path)
            print(f"done ({size // 1024} KB)")
        except Exception as e:
            print(f"FAILED: {e}")

    print("  Text extraction complete.")

if __name__ == "__main__":
    main()
