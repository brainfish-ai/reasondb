import type { Metadata } from "next"
import { Geist, Geist_Mono } from "next/font/google"
import "./globals.css"
const geist = Geist({ subsets: ["latin"], variable: "--font-geist-sans" })
const geistMono = Geist_Mono({ subsets: ["latin"], variable: "--font-geist-mono" })
export const metadata: Metadata = {
  title: "Insurance Policy Analysis · ReasonDB Tutorial",
  description: "Analyze income protection and life insurance policy documents using ReasonDB structural reasoning",
}
export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={`${geist.variable} ${geistMono.variable}`}>
      <body className="font-sans antialiased min-h-screen bg-background">{children}</body>
    </html>
  )
}
