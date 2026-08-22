import { isIP } from "node:net";
import { ZodError } from "zod";

import { validateVirtualPath, type VirtualPath } from "../shared/path";
import {
  GatewayPurgeConfirmationError,
  GatewayRateLimitError,
} from "./gateway";
import {
  UpstreamApiError,
  UpstreamRequestError,
  UpstreamResponseTooLargeError,
} from "./fslite-client";
import { WorkspaceResettingError } from "./reset-coordinator";

export const MAX_REQUEST_BYTES = 1024 * 1024;

export interface PublicError {
  error: {
    code: string;
    message: string;
    status: number;
    requestId?: string;
    retryAfterMs?: number;
  };
}

export class PublicRequestError extends Error {
  readonly name: string = "PublicRequestError";

  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

export class BoundedBodyError extends PublicRequestError {
  readonly name = "BoundedBodyError";

  constructor(readonly limitBytes: number) {
    super(413, "payload_too_large", "The request body is too large.");
  }
}

export class ResponseTooLargeError extends Error {
  readonly name = "ResponseTooLargeError";

  constructor(readonly limitBytes: number) {
    super("The upstream response is too large.");
  }
}

function requestId(): string {
  return crypto.randomUUID();
}

function safeRequestId(value: string | undefined): string {
  return value && /^[A-Za-z0-9._-]{1,128}$/.test(value) ? value : requestId();
}

const UPSTREAM_PUBLIC_MESSAGES: Readonly<Record<string, string>> = {
  already_exists: "The destination already exists.",
  invalid_request: "The filesystem service rejected the request.",
  not_found: "The requested item was not found.",
  quota_exceeded: "The workspace quota does not allow this operation.",
  revision_conflict: "The file changed before the operation completed.",
};

function publicCode(value: string): string {
  return /^[a-z][a-z0-9_]{0,63}$/.test(value) ? value : "upstream_error";
}

/** Returns a JSON response with a stable UTF-8 media type. */
export function json(value: unknown, init: ResponseInit = {}): Response {
  const headers = new Headers(init.headers);
  headers.set("content-type", "application/json; charset=utf-8");
  return new Response(JSON.stringify(value), { ...init, headers });
}

/** Strictly accepts JSON and its one optional charset parameter. */
export function isJsonRequest(request: Request): boolean {
  const mediaType = request.headers.get("content-type");
  return (
    mediaType !== null &&
    /^application\/json(?:\s*;\s*charset\s*=\s*(?:"[^"]+"|[^;\s]+))?\s*$/i.test(
      mediaType,
    )
  );
}

/** Reads a request stream without allowing more than the declared byte quota. */
export async function readBoundedBody(
  request: Request,
  limitBytes = MAX_REQUEST_BYTES,
): Promise<Uint8Array> {
  const declaredLength = request.headers.get("content-length");
  if (declaredLength !== null) {
    if (!/^\d+$/.test(declaredLength) || Number(declaredLength) > limitBytes) {
      throw new BoundedBodyError(limitBytes);
    }
  }

  if (!request.body) {
    return new Uint8Array();
  }

  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    while (true) {
      const next = await reader.read();
      if (next.done) {
        break;
      }
      length += next.value.byteLength;
      if (length > limitBytes) {
        await reader.cancel();
        throw new BoundedBodyError(limitBytes);
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }

  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return bytes;
}

function requiredPath(value: string | null | undefined): string {
  if (value === undefined || value === null || value === "") {
    throw new PublicRequestError(400, "invalid_request", "A path is required.");
  }
  return value;
}

function validatePath(value: string): VirtualPath {
  try {
    return validateVirtualPath(value);
  } catch {
    throw new PublicRequestError(
      400,
      "invalid_request",
      "The path is invalid.",
    );
  }
}

/** Validates a path returned by URLSearchParams, which is already decoded. */
export function validateQueryPath(
  value: string | null | undefined,
): VirtualPath {
  return validatePath(requiredPath(value));
}

/** Decodes the raw dynamic catch-all once before applying virtual-path policy. */
export function decodeCatchAllPath(
  value: string | null | undefined,
): VirtualPath {
  let decoded: string;
  try {
    decoded = decodeURIComponent(`/${requiredPath(value)}`);
  } catch {
    throw new PublicRequestError(
      400,
      "invalid_request",
      "The path is invalid.",
    );
  }
  return validatePath(decoded);
}

/** Resolves a direct peer address unless ServerConfig explicitly trusts a proxy. */
export function clientIp(
  request: Request,
  directAddress: string,
  trustProxy: boolean,
): string {
  if (!trustProxy) {
    return directAddress;
  }
  const forwarded = request.headers.get("x-forwarded-for");
  const first = forwarded?.split(",", 1)[0]?.trim();
  return first && !/[\r\n\0%]/.test(first) && isIP(first) !== 0
    ? first
    : directAddress;
}

/** Maps only known failures into a browser-safe error envelope. */
export function gatewayErrorResponse(
  error: unknown,
  fallbackRequestId?: string,
): Response {
  let status = 502;
  let code = "bad_gateway";
  let message = "The filesystem service is unavailable.";
  let retryAfterMs: number | undefined;
  let responseRequestId = safeRequestId(fallbackRequestId);

  if (error instanceof PublicRequestError) {
    status = error.status;
    code = error.code;
    message = error.message;
  } else if (error instanceof ZodError || error instanceof SyntaxError) {
    status = 400;
    code = "invalid_request";
    message = "The request is invalid.";
  } else if (error instanceof GatewayPurgeConfirmationError) {
    status = 400;
    code = "invalid_request";
    message = "The request is invalid.";
  } else if (error instanceof GatewayRateLimitError) {
    status = 429;
    code = "rate_limited";
    message = "Too many requests; try again shortly.";
    retryAfterMs = Math.max(0, Math.ceil(error.retryAfterMs));
  } else if (error instanceof WorkspaceResettingError) {
    status = 503;
    code = "workspace_resetting";
    message = "The shared workspace is resetting; try again shortly.";
    retryAfterMs = Math.max(0, Math.ceil(error.retryAfterMs));
  } else if (error instanceof UpstreamApiError) {
    responseRequestId = safeRequestId(error.requestId);
    if (error.status >= 400 && error.status < 500) {
      status = error.status;
      code = publicCode(error.code);
      message =
        UPSTREAM_PUBLIC_MESSAGES[code] ??
        "The filesystem service rejected the request.";
    }
  } else if (
    error instanceof UpstreamRequestError ||
    error instanceof UpstreamResponseTooLargeError ||
    error instanceof ResponseTooLargeError
  ) {
    code =
      error instanceof UpstreamResponseTooLargeError ||
      error instanceof ResponseTooLargeError
        ? "upstream_response_too_large"
        : "upstream_unavailable";
  }

  const body: PublicError = {
    error: {
      code,
      message,
      status,
      requestId: responseRequestId,
      ...(retryAfterMs === undefined ? {} : { retryAfterMs }),
    },
  };
  const headers = new Headers({ "x-request-id": responseRequestId });
  if (retryAfterMs !== undefined) {
    headers.set(
      "retry-after",
      String(Math.max(1, Math.ceil(retryAfterMs / 1000))),
    );
  }
  return json(body, { status, headers });
}

export function methodNotAllowed(allow: string): Response {
  const response = gatewayErrorResponse(
    new PublicRequestError(
      405,
      "method_not_allowed",
      "The request method is not allowed.",
    ),
  );
  response.headers.set("allow", allow);
  return response;
}
