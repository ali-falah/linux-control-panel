const RULES: Array<[RegExp, string]> = [
  [
    /-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----/gi,
    '[REDACTED PRIVATE KEY]',
  ],
  [/\bsk-(?:proj-)?[A-Za-z0-9_-]{16,}\b/g, '[REDACTED API KEY]'],
  [/\bAIza[0-9A-Za-z_-]{20,}\b/g, '[REDACTED API KEY]'],
  [/\bAKIA[0-9A-Z]{16}\b/g, '[REDACTED AWS KEY]'],
  [/\bgh[pousr]_[A-Za-z0-9]{20,}\b/g, '[REDACTED TOKEN]'],
  [/\b(Bearer|Basic)\s+[A-Za-z0-9._~+/=-]+/gi, '$1 [REDACTED]'],
  [
    /\b(password|passwd|pwd|secret|token|api[_-]?key|access[_-]?key|client[_-]?secret)\b\s*[:=]\s*("[^"]*"|'[^']*'|[^\s,;]+)/gi,
    '$1=[REDACTED]',
  ],
  [/[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/g, '[REDACTED EMAIL]'],
];

/**
 * Remove common secrets from text before it is sent to an AI provider.
 * This is deliberately conservative: it targets known credential shapes and
 * `key=value` assignments rather than guessing at arbitrary data.
 */
export function redactSensitiveData(input: string): string {
  if (!input) return input;

  let output = input;
  for (const [pattern, replacement] of RULES) {
    output = output.replace(pattern, replacement);
  }
  return output;
}
