import { describe, expect, it } from 'vitest';
import { redactSensitiveData } from './aiRedaction';

describe('redactSensitiveData', () => {
  it('redacts OpenAI and Gemini keys', () => {
    expect(redactSensitiveData('key sk-proj-abcdefghijklmnop1234')).toBe(
      'key [REDACTED API KEY]',
    );
    expect(redactSensitiveData('key AIzaSyA1234567890abcdefghijklmnopq')).toBe(
      'key [REDACTED API KEY]',
    );
  });

  it('redacts authorization headers and assignments', () => {
    expect(redactSensitiveData('Authorization: Bearer abc.def-ghi')).toBe(
      'Authorization: Bearer [REDACTED]',
    );
    expect(redactSensitiveData('password=hunter2 user=ali')).toBe(
      'password=[REDACTED] user=ali',
    );
    expect(redactSensitiveData('client_secret: "s3cr3t"')).toBe(
      'client_secret=[REDACTED]',
    );
    expect(redactSensitiveData('token=abc123')).toBe('token=[REDACTED]');
  });

  it('redacts private keys and email addresses', () => {
    expect(
      redactSensitiveData('-----BEGIN RSA PRIVATE KEY-----\nMIIE\n-----END RSA PRIVATE KEY-----'),
    ).toBe('[REDACTED PRIVATE KEY]');
    expect(redactSensitiveData('contact admin@example.com for access')).toBe(
      'contact [REDACTED EMAIL] for access',
    );
  });

  it('leaves ordinary log lines intact', () => {
    const line = 'nginx.service: Failed to start at 12:00:01 (status=1/FAILURE)';
    expect(redactSensitiveData(line)).toBe(line);
  });
});
