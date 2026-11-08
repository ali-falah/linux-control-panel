import { describe, expect, it } from 'vitest';
import { smartHealthIssueLabel, smartHealthTone } from './smartHealth';

describe('SMART health presentation', () => {
  it('labels a per-device error with the device and sanitized reason', () => {
    expect(
      smartHealthIssueLabel({
        disk_path: '/dev/sda',
        error: 'SMART data could not be read',
      }),
    ).toBe('/dev/sda: SMART data could not be read');
  });

  it('maps health statuses to display tones', () => {
    expect(smartHealthTone('PASSED')).toBe('success');
    expect(smartHealthTone('OK')).toBe('success');
    expect(smartHealthTone('FAILED')).toBe('error');
    expect(smartHealthTone('BAD')).toBe('error');
    expect(smartHealthTone('UNKNOWN')).toBe('warning');
  });
});
