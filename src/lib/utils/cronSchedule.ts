/**
 * Cron expression parsing, humanising and the next-run calculation used by
 * the Scheduled Tasks page. Pure functions only, so the arithmetic that
 * decides "next run" is unit tested instead of being trusted.
 */

const DAY_LONG = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
const DAY_ALIASES = ['sun', 'mon', 'tue', 'wed', 'thu', 'fri', 'sat'];
const MONTH_LONG = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const MONTH_ALIASES = ['jan', 'feb', 'mar', 'apr', 'may', 'jun', 'jul', 'aug', 'sep', 'oct', 'nov', 'dec'];

const NAMED_SCHEDULES: Record<string, string> = {
  '@reboot': 'At every system boot',
  '@yearly': 'Once a year, on 1 January',
  '@annually': 'Once a year, on 1 January',
  '@monthly': 'Once a month, on day 1',
  '@weekly': 'Once a week, on Sunday',
  '@daily': 'Once a day, at midnight',
  '@midnight': 'Once a day, at midnight',
  '@hourly': 'Once an hour, on the hour'
};

interface CronFields {
  minutes: number[];
  hours: number[];
  doms: number[];
  months: number[];
  dows: number[];
  domStar: boolean;
  dowStar: boolean;
}

function fieldValue(token: string, min: number, max: number, names?: string[]): number | null {
  if (names) {
    const index = names.indexOf(token);
    if (index >= 0) return min + index;
  }
  if (!/^\d+$/.test(token)) return null;
  const value = Number(token);
  return value >= min && value <= max ? value : null;
}

function expandField(field: string, min: number, max: number, names?: string[]): number[] | null {
  const values = new Set<number>();
  for (const part of field.split(',')) {
    if (!part) return null;
    const [base, stepText] = part.split('/');
    if (stepText !== undefined && !/^\d+$/.test(stepText)) return null;
    const step = stepText === undefined ? 1 : Number(stepText);
    if (step < 1) return null;

    let start: number;
    let end: number;
    if (base === '*') {
      start = min;
      end = max;
    } else if (base.includes('-')) {
      const [from, to] = base.split('-');
      const fromValue = fieldValue(from, min, max, names);
      const toValue = fieldValue(to, min, max, names);
      if (fromValue === null || toValue === null || fromValue > toValue) return null;
      start = fromValue;
      end = toValue;
    } else {
      const value = fieldValue(base, min, max, names);
      if (value === null) return null;
      start = value;
      end = stepText === undefined ? value : max;
    }

    for (let value = start; value <= end; value += step) values.add(value);
  }
  return [...values].sort((a, b) => a - b);
}

function cronFields(schedule: string): CronFields | null {
  const text = schedule.trim().toLowerCase();
  if (text.startsWith('@')) {
    const aliases: Record<string, string> = {
      '@yearly': '0 0 1 1 *', '@annually': '0 0 1 1 *', '@monthly': '0 0 1 * *',
      '@weekly': '0 0 * * 0', '@daily': '0 0 * * *', '@midnight': '0 0 * * *',
      '@hourly': '0 * * * *'
    };
    const mapped = aliases[text];
    if (!mapped) return null;
    return cronFields(mapped);
  }

  const parts = text.split(/\s+/);
  if (parts.length !== 5) return null;

  const minutes = expandField(parts[0], 0, 59);
  const hours = expandField(parts[1], 0, 23);
  const doms = expandField(parts[2], 1, 31);
  const months = expandField(parts[3], 1, 12, MONTH_ALIASES);
  const dows = expandField(parts[4], 0, 7, DAY_ALIASES);
  if (!minutes || !hours || !doms || !months || !dows) return null;

  return {
    minutes, hours, doms, months, dows,
    domStar: parts[2] === '*',
    dowStar: parts[4] === '*'
  };
}

export function scheduleError(expr: string): string | null {
  const text = expr.trim().toLowerCase();
  if (!text) return 'Enter a schedule.';
  if (text.startsWith('@')) {
    return text in NAMED_SCHEDULES
      ? null
      : 'Unknown shortcut — use @reboot, @hourly, @daily, @weekly, @monthly or @yearly.';
  }
  return cronFields(text)
    ? null
    : 'Use five fields (minute hour day month weekday) or a shortcut like @daily.';
}

/** Next wall-clock occurrence, or null for @reboot / unparseable input. */
export function nextRun(schedule: string, from = new Date()): Date | null {
  if (schedule.trim().toLowerCase() === '@reboot') return null;
  const fields = cronFields(schedule);
  if (!fields) return null;

  const start = new Date(from.getTime());
  start.setSeconds(0, 0);
  start.setMinutes(start.getMinutes() + 1);

  const dowMatches = (day: number) => fields.dows.includes(day) || (day === 0 && fields.dows.includes(7));

  for (let offset = 0; offset <= 366 * 8; offset++) {
    const day = new Date(start.getFullYear(), start.getMonth(), start.getDate() + offset);
    if (!fields.months.includes(day.getMonth() + 1)) continue;

    let dayMatches: boolean;
    if (!fields.domStar && !fields.dowStar) {
      // cron treats a restricted day-of-month and weekday as OR
      dayMatches = fields.doms.includes(day.getDate()) || dowMatches(day.getDay());
    } else if (!fields.domStar) {
      dayMatches = fields.doms.includes(day.getDate());
    } else if (!fields.dowStar) {
      dayMatches = dowMatches(day.getDay());
    } else {
      dayMatches = true;
    }
    if (!dayMatches) continue;

    for (const hour of fields.hours) {
      for (const minute of fields.minutes) {
        const candidate = new Date(day.getFullYear(), day.getMonth(), day.getDate(), hour, minute, 0, 0);
        if (candidate.getTime() >= start.getTime()) return candidate;
      }
    }
  }
  return null;
}

const pad = (value: string | number) => String(value).padStart(2, '0');

function describeDays(dow: string): string | null {
  if (dow === '*') return null;
  if (dow === '1-5' || dow === 'mon-fri') return 'Weekdays';
  if (dow === '0,6' || dow === '6,0' || dow === 'sat,sun' || dow === 'sun,sat') return 'Weekends';
  if (!dow.includes(',') && !dow.includes('-')) {
    const alias = DAY_ALIASES.indexOf(dow);
    const day = alias >= 0 ? alias : (/^\d$/.test(dow) ? Number(dow) % 7 : -1);
    if (day >= 0) return `Every ${DAY_LONG[day]}`;
  }
  const names = dow.split(',').map((part) => {
    const index = DAY_ALIASES.indexOf(part) >= 0 ? DAY_ALIASES.indexOf(part) : (/^\d$/.test(part) ? Number(part) % 7 : -1);
    return index >= 0 ? DAY_LONG[index].slice(0, 3) : null;
  });
  return names.every(Boolean) ? names.join(', ') : null;
}

function describeMonths(mon: string): string | null {
  if (/^\d+$/.test(mon)) {
    const index = Number(mon) - 1;
    return MONTH_LONG[index] ?? null;
  }
  const parts = mon.split(',').map((part) => {
    const index = MONTH_ALIASES.indexOf(part) >= 0 ? MONTH_ALIASES.indexOf(part) : (/^\d+$/.test(part) ? Number(part) - 1 : -1);
    return index >= 0 ? MONTH_LONG[index] : null;
  });
  return parts.every(Boolean) ? parts.join(', ') : null;
}

/** Turns a raw cron expression into a sentence a human can scan. */
export function humanizeSchedule(expr: string): string {
  const text = expr.trim().toLowerCase();
  if (NAMED_SCHEDULES[text]) return NAMED_SCHEDULES[text];

  const parts = text.split(/\s+/);
  if (parts.length !== 5) return 'Custom schedule';
  const [minute, hour, dom, mon, dow] = parts;

  if (minute === '*' && hour === '*' && dom === '*' && mon === '*' && dow === '*') return 'Every minute';
  if (/^\*\/\d+$/.test(minute) && hour === '*' && dom === '*' && mon === '*' && dow === '*') {
    return `Every ${Number(minute.slice(2))} minutes`;
  }
  if (minute === '0' && hour === '*' && dom === '*' && mon === '*' && dow === '*') {
    return 'Every hour, on the hour';
  }
  if (/^\d+$/.test(minute) && hour === '*' && dom === '*' && mon === '*' && dow === '*') {
    return `Every hour at :${pad(minute)}`;
  }
  if (/^\*\/\d+$/.test(hour) && dom === '*' && mon === '*' && dow === '*' && /^\d+$/.test(minute)) {
    const every = Number(hour.slice(2));
    return minute === '0' ? `Every ${every} hours` : `Every ${every} hours at :${pad(minute)}`;
  }

  if (/^\d+$/.test(minute) && /^\d+$/.test(hour)) {
    const time = `${pad(hour)}:${pad(minute)}`;

    if (dom === '*' && mon === '*' && dow === '*') return `Every day at ${time}`;

    if (dom === '*' && mon === '*') {
      const days = describeDays(dow);
      if (days) return `${days} at ${time}`;
    }

    if (mon === '*' && dow === '*') {
      if (/^\d+$/.test(dom)) return `On day ${dom} of each month at ${time}`;
      if (/^\*\/\d+$/.test(dom)) return `Every ${Number(dom.slice(2))} days at ${time}`;
    }

    if (/^\d+$/.test(dom) && mon !== '*') {
      const month = describeMonths(mon);
      if (month) return `On ${month} ${dom} at ${time}`;
    }
  }

  return 'Custom schedule';
}

export function relativeLabel(target: Date, now = Date.now()): string {
  const diff = target.getTime() - now;
  if (diff <= 0) return 'due now';
  const minutes = Math.round(diff / 60_000);
  if (minutes < 1) return 'in <1 min';
  if (minutes < 60) return `in ${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (hours < 24) return rest ? `in ${hours}h ${rest}m` : `in ${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `in ${days} day${days === 1 ? '' : 's'}`;
  return `in ${Math.round(days / 7)} wk`;
}

export function relativePast(epochSeconds: number, now = Date.now()): string {
  const minutes = Math.round((now - epochSeconds * 1000) / 60_000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days} day${days === 1 ? '' : 's'} ago`;
}

export function absoluteLabel(target: Date): string {
  return target.toLocaleString([], {
    weekday: 'short', month: 'short', day: 'numeric',
    hour: '2-digit', minute: '2-digit'
  });
}

export function epochLabel(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString([], {
    weekday: 'short', month: 'short', day: 'numeric',
    hour: '2-digit', minute: '2-digit'
  });
}
