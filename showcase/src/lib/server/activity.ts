import type { ActivityRecord, JsonValue } from "../shared/contracts";

const MAX_ACTIVITY_JSON_BYTES = 64 * 1024;
const PREVIEW_BYTES = 16 * 1024;
const REDACTED = "[REDACTED]";
const SENSITIVE_KEY = /(?:authorization|cookie|password|secret|token)/i;

export interface ActivityInput {
  token: string;
  serverUrl: string;
  method: string;
  path: string;
  status: number;
  durationMs: number;
  request?: unknown;
  response?: unknown;
  contentType?: string;
  requestId: string;
  /** Deliberately ignored: headers can contain bearer credentials. */
  headers?: HeadersInit;
}

function redactText(value: string, token: string): string {
  return token ? value.split(token).join(REDACTED) : value;
}

function sanitizedJson(
  value: unknown,
  token: string,
  seen = new WeakSet<object>(),
): JsonValue {
  if (
    value === null ||
    typeof value === "boolean" ||
    typeof value === "number"
  ) {
    return value;
  }
  if (typeof value === "string") {
    return redactText(value, token);
  }
  if (Array.isArray(value)) {
    return value.map((entry) => sanitizedJson(entry, token, seen));
  }
  if (typeof value === "object") {
    if (seen.has(value)) {
      return "[CIRCULAR]";
    }
    seen.add(value);
    const output: Record<string, JsonValue> = {};
    for (const [key, entry] of Object.entries(value)) {
      output[key] = SENSITIVE_KEY.test(key)
        ? REDACTED
        : sanitizedJson(entry, token, seen);
    }
    return output;
  }
  return String(value);
}

function utf8Prefix(value: string, maxBytes: number): string {
  const encoder = new TextEncoder();
  if (encoder.encode(value).byteLength <= maxBytes) {
    return value;
  }

  let end = value.length;
  while (end > 0 && encoder.encode(value.slice(0, end)).byteLength > maxBytes) {
    end -= Math.max(1, Math.ceil((end * 0.1) / 2));
  }
  while (
    end < value.length &&
    encoder.encode(value.slice(0, end + 1)).byteLength <= maxBytes
  ) {
    end += 1;
  }
  return value.slice(0, end);
}

function boundedJson(value: unknown, token: string): JsonValue {
  const sanitized = sanitizedJson(value, token);
  const serialized = JSON.stringify(sanitized);
  const originalBytes = new TextEncoder().encode(serialized).byteLength;
  if (originalBytes <= MAX_ACTIVITY_JSON_BYTES) {
    return sanitized;
  }

  return {
    truncated: true,
    originalBytes,
    preview: utf8Prefix(serialized, PREVIEW_BYTES),
  };
}

/** Sanitizes arbitrary upstream JSON before it can cross a server boundary. */
export function sanitizeActivityJson(value: unknown, token: string): JsonValue {
  return boundedJson(value, token);
}

function binarySummary(
  value: unknown,
  contentType: string | undefined,
): JsonValue | undefined {
  const bytes =
    value instanceof Uint8Array
      ? value.byteLength
      : value instanceof ArrayBuffer
        ? value.byteLength
        : undefined;
  if (bytes === undefined) {
    return undefined;
  }
  return contentType
    ? { binary: true, bytes, contentType }
    : { binary: true, bytes };
}

/** Produces the only upstream metadata that may reach browser activity UI. */
export function buildActivity(input: ActivityInput): ActivityRecord {
  const response = binarySummary(input.response, input.contentType);
  const request = binarySummary(input.request, input.contentType);

  return {
    id: crypto.randomUUID(),
    timestamp: new Date().toISOString(),
    method: input.method,
    path: input.path,
    status: input.status,
    durationMs: Math.max(0, Math.round(input.durationMs)),
    requestId: redactText(input.requestId, input.token),
    request:
      request ??
      (input.request === undefined
        ? null
        : sanitizeActivityJson(input.request, input.token)),
    response:
      response ??
      (input.response === undefined
        ? null
        : sanitizeActivityJson(input.response, input.token)),
    curl: `curl -X ${input.method} -H 'Authorization: Bearer $FSLITE_TOKEN' '$FSLITE_SERVER_URL${input.path.replaceAll("'", "'%27")}'`,
  };
}

/** Used by typed error handling without making server secrets observable. */
export function redactActivityText(value: string, token: string): string {
  return redactText(value, token);
}
