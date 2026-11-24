/**
 * Format one value as a CSV field.
 *
 * Quoting is not enough: a spreadsheet reads a cell that starts with `=`, `+`,
 * `-`, `@`, TAB or CR as a formula, and journal/auth/audit text is
 * attacker-influenced (`Failed password for invalid user =cmd|'/C calc'!A0`).
 * Those get a neutralising apostrophe first, as recommended by OWASP.
 */
export function csvCell(value: unknown): string {
  const raw = String(value ?? '');
  const safe = /^[=+\-@\t\r]/.test(raw) ? `'${raw}` : raw;
  return `"${safe.replace(/"/g, '""')}"`;
}
