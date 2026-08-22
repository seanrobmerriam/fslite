import {
  buildActivity,
  redactActivityText,
  sanitizeActivityJson,
} from "./activity";
import { Buffer } from "node:buffer";
import type { ServerConfig } from "./config";
import type {
  ActivityRecord,
  Change,
  JsonValue,
  Node,
  TrashEntry,
  TreeEntry,
  WorkspaceUsage,
} from "../shared/contracts";
import { encodeVirtualPath, type VirtualPath } from "../shared/path";

const PAGE_LIMIT = 250;
const MAX_JSON_RESPONSE_BYTES = 1024 * 1024;
const MAX_BINARY_RESPONSE_BYTES = 1024 * 1024;

export interface UpstreamResult<T> {
  data: T;
  activity: ActivityRecord;
  contentType?: string;
}

export interface Page<T> {
  items: T[];
  next_cursor: string | null;
}

export interface Identity {
  workspace_id: string;
  capabilities: string[];
}

export interface SearchMatch {
  node: Node;
  path: VirtualPath;
  range: { start: number; end: number };
  preview_base64: string;
}

export interface ClientDependencies {
  requestId?: () => string;
  fetch?: typeof globalThis.fetch;
  now?: () => number;
}

export class UpstreamApiError extends Error {
  readonly name = "UpstreamApiError";

  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
    readonly details: JsonValue | null,
    readonly requestId: string,
  ) {
    super(message);
  }
}

export class UpstreamResponseTooLargeError extends Error {
  readonly name = "UpstreamResponseTooLargeError";

  constructor(readonly limitBytes: number) {
    super("Upstream response exceeded the showcase response limit");
  }
}

export class UpstreamRequestError extends Error {
  readonly name = "UpstreamRequestError";

  constructor(readonly requestId: string) {
    super("The upstream filesystem service is unavailable");
  }
}

interface RequestOptions {
  method: "GET" | "POST" | "PUT" | "DELETE";
  path: string;
  body?: BodyInit;
  activityRequest?: unknown;
  binary?: boolean;
  contentType?: string;
}

interface ErrorEnvelope {
  error?: {
    code?: unknown;
    message?: unknown;
    details?: unknown;
  };
}

function createRequestId(): string {
  return crypto.randomUUID();
}

function jsonBody(value: unknown): string {
  return JSON.stringify(value);
}

function query(
  parts: Array<[string, string | number | boolean | undefined]>,
): string {
  const serialized = parts
    .filter(([, value]) => value !== undefined)
    .map(
      ([key, value]) =>
        `${encodeURIComponent(key)}=${encodeURIComponent(String(value))}`,
    )
    .join("&");
  return serialized ? `?${serialized}` : "";
}

/** A server-only, fixed-route client for fslite's upstream HTTP API. */
export class FsliteClient {
  private workspaceId: string | undefined;
  private readonly fetchImpl: typeof fetch;
  private readonly requestId: () => string;
  private readonly now: () => number;

  constructor(
    private readonly config: ServerConfig,
    workspaceId?: string,
    dependencies: ClientDependencies = {},
  ) {
    this.workspaceId = workspaceId;
    this.fetchImpl = dependencies.fetch ?? globalThis.fetch;
    this.requestId = dependencies.requestId ?? createRequestId;
    this.now = dependencies.now ?? Date.now;
  }

  private route(path: string): string {
    return `${this.config.serverUrl.toString().replace(/\/$/, "")}${path}`;
  }

  private workspaceRoute(path: string): string {
    if (!this.workspaceId) {
      throw new Error(
        "FsliteClient requires identity before workspace operations",
      );
    }
    return `/v1/workspaces/${encodeURIComponent(this.workspaceId)}${path}`;
  }

  private async responseBytes(
    response: Response,
    limitBytes: number,
  ): Promise<Uint8Array> {
    const declaredLength = Number(response.headers.get("content-length"));
    if (Number.isFinite(declaredLength) && declaredLength > limitBytes) {
      throw new UpstreamResponseTooLargeError(limitBytes);
    }
    if (!response.body) {
      return new Uint8Array();
    }

    const reader = response.body.getReader();
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
          throw new UpstreamResponseTooLargeError(limitBytes);
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

  private async errorFrom(
    response: Response,
    requestId: string,
  ): Promise<never> {
    const bytes = await this.responseBytes(response, MAX_JSON_RESPONSE_BYTES);
    const envelope = JSON.parse(
      new TextDecoder().decode(bytes),
    ) as ErrorEnvelope;

    const upstreamRequestId = response.headers.get("x-request-id") || requestId;
    const errorBody = envelope.error;
    const code =
      typeof errorBody?.code === "string" ? errorBody.code : "upstream_error";
    const message = redactActivityText(
      typeof errorBody?.message === "string"
        ? errorBody.message
        : `Upstream request failed with status ${response.status}`,
      this.config.token,
    );
    throw new UpstreamApiError(
      response.status,
      code,
      message,
      errorBody?.details === undefined
        ? null
        : sanitizeActivityJson(errorBody.details, this.config.token),
      redactActivityText(upstreamRequestId, this.config.token),
    );
  }

  private async request<T>(
    options: RequestOptions,
  ): Promise<UpstreamResult<T>> {
    const visitorRequestId = this.requestId();
    const controller = new AbortController();
    const timeout = setTimeout(
      () => controller.abort(),
      this.config.requestTimeoutMs,
    );
    const headers = new Headers({
      authorization: `Bearer ${this.config.token}`,
      "x-request-id": visitorRequestId,
    });
    if (options.contentType) {
      headers.set("content-type", options.contentType);
    }

    const startedAt = this.now();
    try {
      const response = await this.fetchImpl(this.route(options.path), {
        method: options.method,
        headers,
        body: options.body,
        signal: controller.signal,
      });
      if (!response.ok) {
        return await this.errorFrom(response, visitorRequestId);
      }

      const requestId = redactActivityText(
        response.headers.get("x-request-id") || visitorRequestId,
        this.config.token,
      );
      const contentType = response.headers.get("content-type") || undefined;
      const bytes = await this.responseBytes(
        response,
        options.binary ? MAX_BINARY_RESPONSE_BYTES : MAX_JSON_RESPONSE_BYTES,
      );
      const data = options.binary
        ? (bytes as T)
        : bytes.byteLength === 0
          ? (undefined as T)
          : (JSON.parse(new TextDecoder().decode(bytes)) as T);
      const durationMs = this.now() - startedAt;
      const activity = buildActivity({
        token: this.config.token,
        serverUrl: this.config.serverUrl.toString(),
        method: options.method,
        path: options.path,
        status: response.status,
        durationMs,
        request: options.activityRequest,
        response: options.binary ? bytes : data,
        contentType: options.binary ? contentType : undefined,
        requestId,
      });
      return options.binary
        ? { data, activity, contentType }
        : { data, activity };
    } catch (error) {
      if (
        error instanceof UpstreamApiError ||
        error instanceof UpstreamResponseTooLargeError
      ) {
        throw error;
      }
      throw new UpstreamRequestError(
        redactActivityText(visitorRequestId, this.config.token),
      );
    } finally {
      clearTimeout(timeout);
    }
  }

  async identity(): Promise<UpstreamResult<Identity>> {
    const result = await this.request<Identity>({
      method: "GET",
      path: "/v1/me",
    });
    this.workspaceId = result.data.workspace_id;
    return result;
  }

  tree(path: VirtualPath): Promise<UpstreamResult<Page<TreeEntry>>> {
    return this.request({
      method: "GET",
      path: this.workspaceRoute(
        `/directories/${encodeVirtualPath(path)}/tree?limit=${PAGE_LIMIT}`,
      ),
    });
  }

  usage(): Promise<UpstreamResult<WorkspaceUsage>> {
    return this.request({ method: "GET", path: this.workspaceRoute("/usage") });
  }

  stat(path: VirtualPath): Promise<UpstreamResult<Node>> {
    return this.request({
      method: "GET",
      path: this.workspaceRoute(`/fs/${encodeVirtualPath(path)}`),
    });
  }

  readFile(path: VirtualPath): Promise<UpstreamResult<Uint8Array>> {
    return this.request({
      method: "GET",
      path: this.workspaceRoute(`/content/${encodeVirtualPath(path)}`),
      binary: true,
    });
  }

  writeFile(
    path: VirtualPath,
    bytes: Uint8Array,
    expectedRevision?: number,
  ): Promise<UpstreamResult<Node>> {
    return this.request({
      method: "PUT",
      path: this.workspaceRoute(
        `/content/${encodeVirtualPath(path)}${query([["expected_revision", expectedRevision]])}`,
      ),
      // Node's fetch accepts Uint8Array; DOM's BodyInit declaration in the
      // pinned TypeScript lib is narrower than that runtime contract.
      body: bytes as unknown as BodyInit,
      activityRequest: { binary: true, bytes: bytes.byteLength },
      contentType: "application/octet-stream",
    });
  }

  mkdir(
    path: VirtualPath,
    parents: boolean,
    expectedRevision?: number,
  ): Promise<UpstreamResult<Node>> {
    const body = {
      parents,
      exist_ok: false,
      expected_revision: expectedRevision ?? null,
    };
    return this.request({
      method: "PUT",
      path: this.workspaceRoute(
        `/fs/${encodeVirtualPath(path)}?type=directory`,
      ),
      body: jsonBody(body),
      activityRequest: body,
      contentType: "application/json",
    });
  }

  copy(
    from: VirtualPath,
    to: VirtualPath,
    options: {
      recursive: boolean;
      overwrite?: boolean;
      expectedRevision?: number;
    },
  ): Promise<UpstreamResult<Node>> {
    const body = {
      to,
      recursive: options.recursive,
      overwrite: options.overwrite ?? false,
      expected_revision: options.expectedRevision ?? null,
    };
    return this.request({
      method: "POST",
      path: this.workspaceRoute(`/fs/${encodeVirtualPath(from)}?action=copy`),
      body: jsonBody(body),
      activityRequest: body,
      contentType: "application/json",
    });
  }

  move(
    from: VirtualPath,
    to: VirtualPath,
    options: { overwrite?: boolean; expectedRevision?: number } = {},
  ): Promise<UpstreamResult<Node>> {
    const body = {
      to,
      recursive: false,
      overwrite: options.overwrite ?? false,
      expected_revision: options.expectedRevision ?? null,
    };
    return this.request({
      method: "POST",
      path: this.workspaceRoute(`/fs/${encodeVirtualPath(from)}?action=move`),
      body: jsonBody(body),
      activityRequest: body,
      contentType: "application/json",
    });
  }

  trash(
    path: VirtualPath,
    expectedRevision?: number,
  ): Promise<UpstreamResult<TrashEntry>> {
    const body =
      expectedRevision === undefined
        ? {}
        : { expected_revision: expectedRevision };
    return this.request({
      method: "POST",
      path: this.workspaceRoute(`/fs/${encodeVirtualPath(path)}?action=trash`),
      body: jsonBody(body),
      activityRequest: body,
      contentType: "application/json",
    });
  }

  remove(
    path: VirtualPath,
    options: { recursive: boolean; expectedRevision?: number },
  ): Promise<UpstreamResult<void>> {
    return this.request({
      method: "DELETE",
      path: this.workspaceRoute(
        `/fs/${encodeVirtualPath(path)}${query([
          ["recursive", options.recursive],
          ["expected_revision", options.expectedRevision],
        ])}`,
      ),
    });
  }

  listTrash(): Promise<UpstreamResult<Page<TrashEntry>>> {
    return this.request({
      method: "GET",
      path: this.workspaceRoute(`/trash?limit=${PAGE_LIMIT}`),
    });
  }

  restore(
    trashId: string,
    destination?: VirtualPath,
    expectedRevision?: number,
  ): Promise<UpstreamResult<Node>> {
    const body = {
      destination: destination ?? null,
      expected_revision: expectedRevision ?? null,
    };
    return this.request({
      method: "POST",
      path: this.workspaceRoute(
        `/trash/${encodeURIComponent(trashId)}/restore`,
      ),
      body: jsonBody(body),
      activityRequest: body,
      contentType: "application/json",
    });
  }

  purge(trashId: string): Promise<UpstreamResult<void>> {
    return this.request({
      method: "DELETE",
      path: this.workspaceRoute(`/trash/${encodeURIComponent(trashId)}`),
    });
  }

  glob(pattern: string): Promise<UpstreamResult<Page<Node>>> {
    return this.request({
      method: "GET",
      path: this.workspaceRoute(
        `/search/glob${query([
          ["pattern", pattern],
          ["limit", PAGE_LIMIT],
        ])}`,
      ),
    });
  }

  find(
    root: VirtualPath,
    nameContains: string,
  ): Promise<UpstreamResult<Page<Node>>> {
    const body = {
      query: { root, name_contains: nameContains },
      page: { limit: PAGE_LIMIT },
    };
    return this.request({
      method: "POST",
      path: this.workspaceRoute("/search/find"),
      body: jsonBody(body),
      activityRequest: body,
      contentType: "application/json",
    });
  }

  searchContent(
    root: VirtualPath,
    text: string,
  ): Promise<UpstreamResult<Page<SearchMatch>>> {
    const body = {
      root,
      needle_base64: Buffer.from(text, "utf8").toString("base64"),
      page: { limit: PAGE_LIMIT },
    };
    return this.request({
      method: "POST",
      path: this.workspaceRoute("/search/content"),
      body: jsonBody(body),
      activityRequest: body,
      contentType: "application/json",
    });
  }

  changes(after?: string): Promise<UpstreamResult<Page<Change>>> {
    return this.request({
      method: "GET",
      path: this.workspaceRoute(
        `/changes${query([
          ["after", after],
          ["limit", PAGE_LIMIT],
        ])}`,
      ),
    });
  }

  /** Reset is private runtime infrastructure and is never surfaced by the gateway. */
  resetWorkspace(): Promise<UpstreamResult<WorkspaceUsage>> {
    return this.request({
      method: "POST",
      path: this.workspaceRoute("/reset"),
    });
  }
}
