export interface SmartHealthDisk {
  disk_path: string;
  model: string;
  health_status: string;
}

export interface SmartHealthIssue {
  disk_path: string;
  error: string;
}

export interface SmartHealthReport {
  disks: SmartHealthDisk[];
  errors: SmartHealthIssue[];
}

export type SmartHealthTone = 'success' | 'warning' | 'error';

export function smartHealthIssueLabel(issue: SmartHealthIssue): string {
  return `${issue.disk_path}: ${issue.error}`;
}

export function smartHealthTone(status: string): SmartHealthTone {
  const normalized = status.trim().toUpperCase();
  if (normalized.includes('PASSED') || normalized === 'OK') return 'success';
  if (normalized.includes('FAIL') || normalized.includes('BAD')) return 'error';
  return 'warning';
}

export interface SmartHealthGateDecision {
  fetch: boolean;
  loaded: boolean;
}

/** Gate SMART fetches on admin mode and fetch only once per admin session. */
export function smartHealthGate(
  adminModeOn: boolean,
  loaded: boolean,
): SmartHealthGateDecision {
  if (!adminModeOn) return { fetch: false, loaded: false };
  if (loaded) return { fetch: false, loaded: true };
  return { fetch: true, loaded: true };
}
