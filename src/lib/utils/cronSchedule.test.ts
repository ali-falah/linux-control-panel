import { describe, expect, it } from 'vitest';
import {
  humanizeSchedule,
  nextRun,
  relativeLabel,
  relativePast,
  scheduleError
} from './cronSchedule.ts';

describe('scheduleError', () => {
  it('accepts five-field expressions and shortcuts', () => {
    for (const expression of [
      '* * * * *',
      '*/15 * * * *',
      '0 9 * * 1-5',
      '0 0 1 jan *',
      '30 4 * * sun',
      '5/15 * * * *',
      '@reboot',
      '@daily'
    ]) {
      expect(scheduleError(expression), expression).toBeNull();
    }
  });

  it('rejects malformed and out-of-range fields', () => {
    for (const expression of [
      '',
      '* * * *',
      '60 * * * *',
      '* 24 * * *',
      '* * 0 * *',
      '* * * 13 *',
      '* * * * 8',
      '*/0 * * * *',
      '10-5 * * * *',
      '@sometimes',
      '1,,2 * * * *'
    ]) {
      expect(scheduleError(expression), expression).not.toBeNull();
    }
  });
});

describe('nextRun', () => {
  it('finds the next daily occurrence', () => {
    const from = new Date(2026, 0, 5, 4, 0, 0);
    expect(nextRun('0 3 * * *', from)).toEqual(new Date(2026, 0, 6, 3, 0, 0));
  });

  it('respects a minute step', () => {
    const from = new Date(2026, 0, 5, 10, 7, 0);
    expect(nextRun('*/15 * * * *', from)).toEqual(new Date(2026, 0, 5, 10, 15, 0));
  });

  it('treats a restricted day-of-month and weekday as OR', () => {
    // Tuesday 6 Jan 2026: the next Friday (9 Jan) comes before the 13th.
    const from = new Date(2026, 0, 6, 10, 0, 0);
    expect(nextRun('0 0 13 * fri', from)).toEqual(new Date(2026, 0, 9, 0, 0, 0));
  });

  it('expands month names', () => {
    const from = new Date(2026, 11, 20, 0, 0, 0);
    expect(nextRun('0 0 1 jan *', from)).toEqual(new Date(2027, 0, 1, 0, 0, 0));
  });

  it('has no wall clock for @reboot', () => {
    expect(nextRun('@reboot', new Date(2026, 0, 5, 10, 0, 0))).toBeNull();
  });
});

describe('humanizeSchedule', () => {
  it('describes the common shapes', () => {
    expect(humanizeSchedule('0 3 * * *')).toBe('Every day at 03:00');
    expect(humanizeSchedule('*/15 * * * *')).toBe('Every 15 minutes');
    expect(humanizeSchedule('0 9 * * 1-5')).toBe('Weekdays at 09:00');
    expect(humanizeSchedule('01 * * * *')).toBe('Every hour at :01');
    expect(humanizeSchedule('30 4 * * sun')).toBe('Every Sunday at 04:30');
    expect(humanizeSchedule('0 0 1 jan *')).toBe('On Jan 1 at 00:00');
    expect(humanizeSchedule('@reboot')).toBe('At every system boot');
    expect(humanizeSchedule('5 4 3 2 1')).toBe('On Feb 3 at 04:05');
  });
});

describe('relative labels', () => {
  const now = new Date(2026, 0, 5, 10, 0, 0).getTime();

  it('formats upcoming times', () => {
    expect(relativeLabel(new Date(now + 10 * 60_000), now)).toBe('in 10 min');
    expect(relativeLabel(new Date(now + 90 * 60_000), now)).toBe('in 1h 30m');
    expect(relativeLabel(new Date(now + 2 * 24 * 60 * 60_000), now)).toBe('in 2 days');
    expect(relativeLabel(new Date(now), now)).toBe('due now');
  });

  it('formats the past', () => {
    expect(relativePast(now / 1000 - 2 * 3600, now)).toBe('2h ago');
    expect(relativePast(now / 1000 - 5 * 24 * 3600, now)).toBe('5 days ago');
  });
});
