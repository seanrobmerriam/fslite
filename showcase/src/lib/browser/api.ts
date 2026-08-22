import type {
  ActivityRecord,
  GatewayResult,
  JsonValue,
} from "../shared/contracts";
import type { VirtualPath } from "../shared/path";
import type { PublicOperation } from "../server/schemas";

export const MAX_BROWSER_FILE_BYTES = 1024 * 1024;

export interface PublicErrorEnvelope {
  error: {
    code: string;
    message: string;
    status: number;
    requestId?: string;
    retryAfterMs?: number;
  };
}

export interface BrowserStatus {
  ready: true;
  generation: number;
  resetting: boolean;
  nextResetAt: number;
  now: number;
  usage: unknown;
}

export class ShowcaseError extends Error {
  readonly name = "ShowcaseError";

  constructor(
    readonly code: string,
    message: string,
    readonly status: number,
    readonly requestId?: string,
    readonly retryAfterMs?: number,
  ) {
    super(message);
  }
}

export interface ObjectUrlApi {
  createObjectURL(value: Blob): string;
  revokeObjectURL(value: string): void;
}

export interface ShowcaseApiDependencies {
  fetch?: typeof globalThis.fetch;
  document?: () => Document;
  objectUrl?: ObjectUrlApi;
}

function errorFromEnvelope(
  value: unknown,
  fallbackStatus: number,
): ShowcaseError {
  if (
    value &&
    typeof value === "object" &&
    "error" in value &&
    value.error &&
    typeof value.error === "object"
  ) {
    const error = value.error as Record<string, unknown>;
    if (
      typeof error.code === "string" &&
      typeof error.message === "string" &&
      typeof error.status === "number"
    ) {
      return new ShowcaseError(
        error.code,
        error.message,
        error.status,
        typeof error.requestId === "string" ? error.requestId : undefined,
        typeof error.retryAfterMs === "number" ? error.retryAfterMs : undefined,
      );
    }
  }
  return new ShowcaseError(
    "invalid_response",
    "The showcase returned an invalid error response.",
    fallbackStatus,
  );
}

async function json(response: Response): Promise<unknown> {
  try {
    return await response.json();
  } catch {
    return undefined;
  }
}

function assertGatewayResult<T>(value: unknown): GatewayResult<T> {
  if (
    !value ||
    typeof value !== "object" ||
    !("data" in value) ||
    !("activity" in value)
  ) {
    throw new ShowcaseError(
      "invalid_response",
      "The showcase returned an invalid response.",
      502,
    );
  }
  return value as GatewayResult<T>;
}

function assertStatus(value: unknown): BrowserStatus {
  if (
    !value ||
    typeof value !== "object" ||
    (value as Record<string, unknown>).ready !== true ||
    typeof (value as Record<string, unknown>).generation !== "number" ||
    typeof (value as Record<string, unknown>).resetting !== "boolean" ||
    typeof (value as Record<string, unknown>).nextResetAt !== "number" ||
    typeof (value as Record<string, unknown>).now !== "number"
  ) {
    throw new ShowcaseError(
      "invalid_response",
      "The showcase returned an invalid status response.",
      502,
    );
  }
  const status = value as Record<string, unknown>;
  return {
    ready: true,
    generation: status.generation as number,
    resetting: status.resetting as boolean,
    nextResetAt: status.nextResetAt as number,
    now: status.now as number,
    usage: status.usage,
  };
}

function safeHeader(
  value: string | null,
  fallback: string,
  maximum = 128,
): string {
  if (!value) {
    return fallback;
  }
  return value.replace(/[\r\n\0]/g, "_").slice(0, maximum) || fallback;
}

function safeMethod(value: string | null): string {
  return ["GET", "POST", "PUT", "DELETE"].includes(value ?? "")
    ? (value as string)
    : "GET";
}

function safeNumber(value: string | null, fallback: number): number {
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed >= 0 && parsed <= 3_600_000
    ? parsed
    : fallback;
}

function downloadFilename(path: VirtualPath): string {
  const basename = path.split("/").at(-1) || "download";
  return basename.replace(/[^\x20-\x7e]|[\\/"\r\n\0]/g, "_") || "download";
}

function localDownloadActivity(
  response: Response,
  blob: Blob,
  path: VirtualPath,
): ActivityRecord {
  const requestId = safeHeader(response.headers.get("x-request-id"), "browser");
  return {
    id: crypto.randomUUID(),
    timestamp: new Date().toISOString(),
    method: safeMethod(response.headers.get("x-fslite-method")),
    // Do not expose the upstream activity path; it can contain server topology.
    path: "/api/download",
    status: safeNumber(
      response.headers.get("x-fslite-status"),
      response.status,
    ),
    durationMs: safeNumber(response.headers.get("x-fslite-duration-ms"), 0),
    requestId,
    request: { path } as unknown as JsonValue,
    response: { binary: true, bytes: blob.size },
    curl: `curl -X GET '/api/download?path=${encodeURIComponent(path)}'`,
  };
}

/** Browser-only gateway client. It deliberately contains only relative API paths. */
export class ShowcaseApi {
  private readonly fetchImpl: typeof globalThis.fetch | undefined;
  private readonly documentFactory: () => Document;
  private readonly objectUrl: ObjectUrlApi;

  constructor(dependencies: ShowcaseApiDependencies = {}) {
    this.fetchImpl = dependencies.fetch;
    this.documentFactory = dependencies.document ?? (() => globalThis.document);
    this.objectUrl =
      dependencies.objectUrl ??
      ({
        createObjectURL: (value) => globalThis.URL.createObjectURL(value),
        revokeObjectURL: (value) => globalThis.URL.revokeObjectURL(value),
      } satisfies ObjectUrlApi);
  }

  async status(signal?: AbortSignal): Promise<BrowserStatus> {
    const response = await this.fetchResponse("/api/status", {
      method: "GET",
      signal,
    });
    const body = await json(response);
    if (!response.ok) {
      throw errorFromEnvelope(body, response.status);
    }
    return assertStatus(body);
  }

  async operation<T>(
    operation: PublicOperation,
    signal?: AbortSignal,
  ): Promise<GatewayResult<T>> {
    const response = await this.fetchResponse("/api/operation", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(operation),
      signal,
    });
    const body = await json(response);
    if (!response.ok) {
      throw errorFromEnvelope(body, response.status);
    }
    return assertGatewayResult<T>(body);
  }

  async upload(
    path: VirtualPath,
    file: File,
    signal?: AbortSignal,
  ): Promise<GatewayResult<unknown>> {
    if (file.size > MAX_BROWSER_FILE_BYTES) {
      throw new ShowcaseError(
        "payload_too_large",
        `Files must not exceed ${MAX_BROWSER_FILE_BYTES} bytes.`,
        413,
      );
    }
    const response = await this.fetchResponse(
      `/api/upload?path=${encodeURIComponent(path)}`,
      {
        method: "POST",
        headers: { "content-type": "application/octet-stream" },
        body: file,
        signal,
      },
    );
    const body = await json(response);
    if (!response.ok) {
      throw errorFromEnvelope(body, response.status);
    }
    return assertGatewayResult(body);
  }

  async download(
    path: VirtualPath,
    signal?: AbortSignal,
  ): Promise<{ activity: ActivityRecord }> {
    const response = await this.fetchResponse(
      `/api/download?path=${encodeURIComponent(path)}`,
      {
        method: "GET",
        signal,
      },
    );
    if (!response.ok) {
      throw errorFromEnvelope(await json(response), response.status);
    }
    const blob = await response.blob();
    const url = this.objectUrl.createObjectURL(blob);
    let anchor: HTMLAnchorElement | undefined;
    try {
      const document = this.documentFactory();
      anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = downloadFilename(path);
      document.body.append(anchor);
      anchor.click();
    } finally {
      anchor?.remove();
      this.objectUrl.revokeObjectURL(url);
    }
    return { activity: localDownloadActivity(response, blob, path) };
  }

  private fetch(): typeof globalThis.fetch {
    return this.fetchImpl ?? globalThis.fetch;
  }

  private async fetchResponse(
    input: RequestInfo | URL,
    init?: RequestInit,
  ): Promise<Response> {
    try {
      return await this.fetch()(input, init);
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") {
        throw error;
      }
      throw new ShowcaseError(
        "network_error",
        "The showcase gateway is unavailable.",
        502,
      );
    }
  }
}
