/**
 * Lightweight RQL syntax highlighter — no Monaco, no external deps.
 * Mirrors the token categories from packages/rql-editor/src/rql-language.ts.
 */

const KEYWORDS = new Set([
  // Standard SQL
  'SELECT', 'FROM', 'WHERE', 'INSERT', 'INTO', 'UPDATE', 'DELETE',
  'CREATE', 'DROP', 'ALTER', 'TABLE', 'INDEX',
  'SET', 'VALUES', 'AND', 'OR', 'NOT', 'IN', 'LIKE', 'BETWEEN',
  'IS', 'NULL', 'TRUE', 'FALSE',
  'ORDER', 'BY', 'ASC', 'DESC', 'LIMIT', 'OFFSET',
  'GROUP', 'HAVING', 'JOIN', 'LEFT', 'RIGHT', 'INNER', 'OUTER', 'ON',
  'AS', 'DISTINCT', 'ALL', 'EXISTS',
  // ReasonDB-specific
  'REASON', 'SEARCH', 'SEMANTIC', 'EMBED', 'SIMILAR', 'TO',
  'SUMMARIZE', 'EXTRACT', 'CHUNK', 'RELATE', 'LINK',
  'WITH', 'CONTEXT', 'THRESHOLD', 'TOP', 'VECTOR', 'CONTAINS', 'ANY',
])

const BUILTINS = new Set([
  'LOWER', 'UPPER', 'TRIM', 'LENGTH', 'SUBSTRING', 'CONCAT', 'REPLACE',
  'ABS', 'CEIL', 'FLOOR', 'ROUND', 'SQRT', 'POW', 'MOD',
  'COUNT', 'SUM', 'AVG', 'MIN', 'MAX',
  'NOW', 'DATE', 'TIME', 'YEAR', 'MONTH', 'DAY',
  'SIMILARITY', 'DISTANCE', 'EMBEDDING', 'TOKENS', 'CHUNKS',
])

type TokenType =
  | 'keyword'
  | 'builtin'
  | 'string'
  | 'number'
  | 'operator'
  | 'delimiter'
  | 'comment'
  | 'identifier'
  | 'space'

interface Token {
  type: TokenType
  value: string
}

// Very small hand-rolled tokeniser — scans left-to-right, longest match wins
function tokenize(src: string): Token[] {
  const tokens: Token[] = []
  let i = 0

  while (i < src.length) {
    // Line comment
    if (src[i] === '-' && src[i + 1] === '-') {
      const end = src.indexOf('\n', i)
      const val = end === -1 ? src.slice(i) : src.slice(i, end)
      tokens.push({ type: 'comment', value: val })
      i += val.length
      continue
    }

    // Block comment
    if (src[i] === '/' && src[i + 1] === '*') {
      const end = src.indexOf('*/', i + 2)
      const val = end === -1 ? src.slice(i) : src.slice(i, end + 2)
      tokens.push({ type: 'comment', value: val })
      i += val.length
      continue
    }

    // String (single-quoted)
    if (src[i] === "'") {
      let j = i + 1
      while (j < src.length) {
        if (src[j] === '\\') { j += 2; continue }
        if (src[j] === "'") { j++; break }
        j++
      }
      tokens.push({ type: 'string', value: src.slice(i, j) })
      i = j
      continue
    }

    // String (double-quoted)
    if (src[i] === '"') {
      let j = i + 1
      while (j < src.length) {
        if (src[j] === '\\') { j += 2; continue }
        if (src[j] === '"') { j++; break }
        j++
      }
      tokens.push({ type: 'string', value: src.slice(i, j) })
      i = j
      continue
    }

    // Whitespace / newlines
    if (/\s/.test(src[i])) {
      let j = i + 1
      while (j < src.length && /\s/.test(src[j])) j++
      tokens.push({ type: 'space', value: src.slice(i, j) })
      i = j
      continue
    }

    // Number (float, hex, int)
    if (/\d/.test(src[i]) || (src[i] === '.' && /\d/.test(src[i + 1] ?? ''))) {
      const m = src.slice(i).match(/^(0[xX][0-9a-fA-F]+|\d*\.\d+([eE][+-]?\d+)?|\d+)/)
      if (m) {
        tokens.push({ type: 'number', value: m[0] })
        i += m[0].length
        continue
      }
    }

    // Identifier or keyword
    if (/[a-zA-Z_$]/.test(src[i])) {
      const m = src.slice(i).match(/^[a-zA-Z_$][\w$.]*/)
      if (m) {
        const word = m[0]
        const upper = word.toUpperCase()
        const type: TokenType = KEYWORDS.has(upper)
          ? 'keyword'
          : BUILTINS.has(upper)
          ? 'builtin'
          : 'identifier'
        tokens.push({ type, value: word })
        i += word.length
        continue
      }
    }

    // Operator / symbol
    if (/[=><!~?:&|+\-*/^%]/.test(src[i])) {
      // Greedy 2-char operators first
      const two = src.slice(i, i + 2)
      if (['==', '<=', '>=', '!=', '&&', '||', '++', '--', '+=', '-=', '*=', '/='].includes(two)) {
        tokens.push({ type: 'operator', value: two })
        i += 2
        continue
      }
      tokens.push({ type: 'operator', value: src[i] })
      i++
      continue
    }

    // Delimiter / bracket
    if (/[;,.()\[\]{}]/.test(src[i])) {
      tokens.push({ type: 'delimiter', value: src[i] })
      i++
      continue
    }

    // Fallthrough — emit as identifier
    tokens.push({ type: 'identifier', value: src[i] })
    i++
  }

  return tokens
}

// Token → Tailwind colour class
function tokenClass(type: TokenType): string {
  switch (type) {
    case 'keyword':    return 'text-blue-400 font-semibold'
    case 'builtin':    return 'text-teal-400'
    case 'string':     return 'text-amber-300'
    case 'number':     return 'text-orange-300'
    case 'operator':   return 'text-slate-400'
    case 'delimiter':  return 'text-slate-500'
    case 'comment':    return 'text-slate-500 italic'
    case 'identifier': return 'text-slate-200'
    case 'space':      return ''
  }
}

interface RqlHighlightProps {
  code: string
  className?: string
}

export function RqlHighlight({ code, className = '' }: RqlHighlightProps) {
  const tokens = tokenize(code)

  return (
    <pre
      className={`text-[11px] font-mono leading-relaxed whitespace-pre-wrap overflow-x-auto bg-[#1e2433] rounded-md px-3 py-2.5 ${className}`}
    >
      {tokens.map((tok, i) =>
        tok.type === 'space' ? (
          tok.value
        ) : (
          <span key={i} className={tokenClass(tok.type)}>
            {tok.value}
          </span>
        )
      )}
    </pre>
  )
}
