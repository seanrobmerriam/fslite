import type {
  ActivityRecord,
  GatewayResult,
  JsonValue,
} from "../shared/contracts";
import type { VirtualPath } from "../shared/path";
import type { PublicOperation } from "../server/schemas";
import { z } from "zod";

export const MAX_BROWSER_FILE_BYTES = 1024 * 1024;

export interface PublicErrorEnvelope {
  error: {
    code: string;
    message: string;
    status: number;
    requestId?: string;
    retryAfterMs?: number;
    activity?: ActivityRecord;
  };
}

export interface BrowserStatus {
  ready: true;
  generation: number;
  resetting: boolean;
  nextResetAt: number | null;
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
    readonly activity?: ActivityRecord,
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

const safeString = z.string().min(1).max(4_096);
const nonNegativeInteger = z.number().int().min(0).max(Number.MAX_SAFE_INTEGER);
const publicStatus = z.number().int().min(100).max(599);
const hasControlCharacter = (value: string) =>
  [...value].some((character) => {
    const code = character.codePointAt(0) ?? 0;
    return code <= 0x1f || code === 0x7f;
  });
const publicPath = z
  .string()
  .min(1)
  .max(4_096)
  .refine((value) => value.startsWith("/") && !hasControlCharacter(value));
const requestId = z.string().regex(/^[A-Za-z0-9._-]{1,128}$/);
const jsonValueSchema: z.ZodType<JsonValue> = z.lazy(() =>
  z.union([
    z.boolean(),
    z.number().refine(Number.isFinite),
    z.string(),
    z.null(),
    z.array(jsonValueSchema),
    z.record(z.string(), jsonValueSchema),
  ]),
);
const nodeSchema = z
  .object({
    workspace_id: safeString,
    id: safeString,
    parent_id: safeString.nullable(),
    name: z.string().min(1).max(255),
    kind: z.enum(["directory", "file", "symlink"]),
    logical_size: nonNegativeInteger,
    created_at_ms: nonNegativeInteger,
    modified_at_ms: nonNegativeInteger,
    accessed_at_ms: nonNegativeInteger,
    revision: z.number().int().positive().max(Number.MAX_SAFE_INTEGER),
    attributes: z.record(z.string(), jsonValueSchema),
  })
  .strict();
const treeEntrySchema = z
  .object({ path: publicPath, depth: nonNegativeInteger, node: nodeSchema })
  .strict();
const trashEntrySchema = z
  .object({
    id: safeString,
    node: nodeSchema,
    original_path: publicPath,
    trashed_at_ms: nonNegativeInteger,
    actor_metadata: z.record(z.string(), jsonValueSchema),
  })
  .strict();
const changeSchema = z
  .object({
    sequence: nonNegativeInteger,
    kind: z.enum([
      "created",
      "modified",
      "copied",
      "moved",
      "removed",
      "trashed",
      "restored",
      "purged",
      "attribute_set",
      "attribute_removed",
    ]),
    node_id: safeString.nullable(),
    old_path: publicPath.nullable(),
    new_path: publicPath.nullable(),
    revision: z
      .number()
      .int()
      .positive()
      .max(Number.MAX_SAFE_INTEGER)
      .nullable(),
    created_at_ms: nonNegativeInteger,
    actor_metadata: z.record(z.string(), jsonValueSchema),
  })
  .strict();
const usageSchema = z
  .object({
    workspace_id: safeString,
    active_logical_bytes: nonNegativeInteger,
    trashed_logical_bytes: nonNegativeInteger,
    staged_bytes: nonNegativeInteger,
    active_nodes: nonNegativeInteger,
    trashed_nodes: nonNegativeInteger,
    max_logical_bytes: nonNegativeInteger,
    max_nodes: nonNegativeInteger,
    max_file_bytes: nonNegativeInteger,
  })
  .strict();
const activitySchema = z
  .object({
    id: safeString,
    timestamp: z.iso.datetime({ offset: true }),
    method: z.enum(["GET", "POST", "PUT", "DELETE"]),
    path: publicPath,
    status: publicStatus,
    durationMs: z.number().int().min(0).max(3_600_000),
    requestId,
    request: jsonValueSchema.nullable(),
    response: jsonValueSchema.nullable(),
    curl: z.string().min(1).max(8_192),
  })
  .strict();
const errorSchema = z
  .object({
    error: z
      .object({
        code: z.string().regex(/^[a-z][a-z0-9_]{0,63}$/),
        message: z.string().min(1).max(1_024),
        status: publicStatus,
        requestId: requestId.optional(),
        retryAfterMs: nonNegativeInteger.optional(),
        activity: z.lazy(() => activitySchema).optional(),
      })
      .strict(),
  })
  .strict();
const statusSchema = z
  .object({
    ready: z.literal(true),
    generation: nonNegativeInteger,
    resetting: z.boolean(),
    nextResetAt: nonNegativeInteger.nullable(),
    now: nonNegativeInteger,
    usage: usageSchema,
  })
  .strict()
  .refine(
    (value) =>
      value.nextResetAt === null ||
      value.nextResetAt >= value.now ||
      value.resetting,
    { message: "next reset must not predate status time" },
  );
const byteSchema = z.number().int().min(0).max(255);
const byteRecordSchema = z
  .record(z.string().regex(/^\d+$/), byteSchema)
  .superRefine((value, context) => {
    const keys = Object.keys(value)
      .map(Number)
      .sort((left, right) => left - right);
    if (keys.some((key, index) => key !== index)) {
      context.addIssue({
        code: "custom",
        message: "byte data must be contiguous",
      });
    }
  });
const readFileSchema = z.union([z.array(byteSchema), byteRecordSchema]);
const page = <T extends z.ZodTypeAny>(item: T) =>
  z
    .object({ items: z.array(item), next_cursor: z.string().min(1).nullable() })
    .strict();
const searchMatchSchema = z
  .object({
    node: nodeSchema,
    path: publicPath,
    range: z
      .object({ start: nonNegativeInteger, end: nonNegativeInteger })
      .strict()
      .refine((value) => value.end >= value.start),
    preview_base64: z.string().min(1).max(4_194_304),
  })
  .strict();

function operationDataSchema(operation: PublicOperation): z.ZodTypeAny {
  switch (operation.kind) {
    case "tree":
      return page(treeEntrySchema);
    case "read_file":
      return readFileSchema;
    case "write_file":
    case "mkdir":
    case "copy":
    case "move":
    case "restore":
      return nodeSchema;
    case "trash":
      return trashEntrySchema;
    case "remove":
    case "purge":
      return z.null().optional();
    case "list_trash":
      return page(trashEntrySchema);
    case "glob":
    case "find":
      return page(nodeSchema);
    case "search_content":
      return page(searchMatchSchema);
    case "changes":
      return page(changeSchema);
    case "usage":
      return usageSchema;
    default: {
      const exhaustive: never = operation;
      return exhaustive;
    }
  }
}

function invalidResponse(message: string): ShowcaseError {
  return new ShowcaseError("invalid_response", message, 502);
}

function errorFromEnvelope(
  value: unknown,
  fallbackStatus: number,
): ShowcaseError {
  const parsed = errorSchema.safeParse(value);
  if (!parsed.success || parsed.data.error.status !== fallbackStatus) {
    return invalidResponse("The showcase returned an invalid error response.");
  }
  const error = parsed.data.error;
  return new ShowcaseError(
    error.code,
    error.message,
    error.status,
    error.requestId,
    error.retryAfterMs,
    error.activity,
  );
}

async function json(response: Response): Promise<unknown> {
  try {
    return await response.json();
  } catch {
    return undefined;
  }
}

function assertGatewayResult<T>(
  value: unknown,
  operation: PublicOperation,
): GatewayResult<T> {
  const envelope = z
    .object({ data: z.unknown().optional(), activity: activitySchema })
    .strict()
    .safeParse(value);
  if (!envelope.success) {
    throw invalidResponse("The showcase returned an invalid response.");
  }
  const data = operationDataSchema(operation).safeParse(envelope.data.data);
  if (!data.success) {
    throw invalidResponse(
      "The showcase returned an invalid operation response.",
    );
  }
  return { data: data.data as T, activity: envelope.data.activity };
}

function assertStatus(value: unknown): BrowserStatus {
  const parsed = statusSchema.safeParse(value);
  if (!parsed.success) {
    throw invalidResponse("The showcase returned an invalid status response.");
  }
  const status = parsed.data;
  return {
    ready: true,
    generation: status.generation as number,
    resetting: status.resetting as boolean,
    nextResetAt: status.nextResetAt as number | null,
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
  return (
    [...value]
      .map((character) => (hasControlCharacter(character) ? "_" : character))
      .join("")
      .slice(0, maximum) || fallback
  );
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

function safeActivityPath(value: string | null, fallback: VirtualPath): string {
  return value &&
    value.startsWith("/") &&
    value.length <= 4_096 &&
    !hasControlCharacter(value)
    ? value
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
    path: safeActivityPath(response.headers.get("x-fslite-path"), path),
    status: safeNumber(
      response.headers.get("x-fslite-status"),
      response.status,
    ),
    durationMs: safeNumber(response.headers.get("x-fslite-duration-ms"), 0),
    requestId,
    request: { path } as unknown as JsonValue,
    response: { binary: true, bytes: blob.size },
    curl: `curl -X GET -H 'Authorization: Bearer $FSLITE_TOKEN' '$FSLITE_SERVER_URL${safeActivityPath(response.headers.get("x-fslite-path"), path).replaceAll("'", "'%27")}'`,
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
    return assertGatewayResult<T>(body, operation);
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
    const envelope = z
      .object({ data: nodeSchema, activity: activitySchema })
      .strict()
      .safeParse(body);
    if (!envelope.success) {
      throw invalidResponse(
        "The showcase returned an invalid upload response.",
      );
    }
    return envelope.data;
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
        "upstream_unavailable",
        "The filesystem service is unavailable.",
        502,
      );
    }
  }
}
