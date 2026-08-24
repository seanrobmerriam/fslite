import type { GatewayResult } from "../shared/contracts";
import type { VirtualPath } from "../shared/path";
import type { UpstreamResult } from "./fslite-client";
import {
  MAX_TEXT_BYTES,
  parsePublicOperation,
  type PublicOperation,
  virtualPathSchema,
} from "./schemas";
import {
  RollingWindowRateLimiter,
  type RateLimitBucket,
  type RateLimitResult,
} from "./rate-limit";

export interface ShowcaseClient {
  tree(path: VirtualPath): Promise<UpstreamResult<unknown>>;
  readFile(path: VirtualPath): Promise<UpstreamResult<Uint8Array>>;
  writeFile(
    path: VirtualPath,
    bytes: Uint8Array,
    expectedRevision?: number,
  ): Promise<UpstreamResult<unknown>>;
  mkdir(path: VirtualPath, parents: boolean): Promise<UpstreamResult<unknown>>;
  copy(
    from: VirtualPath,
    to: VirtualPath,
    options: { recursive: boolean; expectedRevision: number },
  ): Promise<UpstreamResult<unknown>>;
  move(
    from: VirtualPath,
    to: VirtualPath,
    options: { expectedRevision: number },
  ): Promise<UpstreamResult<unknown>>;
  trash(
    path: VirtualPath,
    expectedRevision?: number,
  ): Promise<UpstreamResult<unknown>>;
  remove(
    path: VirtualPath,
    options: { recursive: boolean; expectedRevision: number },
  ): Promise<UpstreamResult<unknown>>;
  listTrash(): Promise<UpstreamResult<unknown>>;
  restore(
    trashId: string,
    destination?: VirtualPath,
    expectedRevision?: number,
  ): Promise<UpstreamResult<unknown>>;
  purge(trashId: string): Promise<UpstreamResult<unknown>>;
  glob(pattern: string): Promise<UpstreamResult<unknown>>;
  find(
    root: VirtualPath,
    nameContains: string,
  ): Promise<UpstreamResult<unknown>>;
  searchContent(
    root: VirtualPath,
    text: string,
  ): Promise<UpstreamResult<unknown>>;
  changes(after?: string): Promise<UpstreamResult<unknown>>;
  usage(): Promise<UpstreamResult<unknown>>;
}

export class GatewayRateLimitError extends Error {
  readonly name = "GatewayRateLimitError";

  constructor(
    readonly bucket: RateLimitBucket,
    readonly retryAfterMs: number,
  ) {
    super(`Too many ${bucket} operations; try again shortly`);
  }
}

export class GatewayPurgeConfirmationError extends Error {
  readonly name = "GatewayPurgeConfirmationError";

  constructor() {
    super("Purge confirmation did not match the current trash entry");
  }
}

export interface ShowcaseGatewayDependencies {
  now?: () => number;
  statusCacheMs?: number;
}

function readBuckets(operation: PublicOperation): readonly RateLimitBucket[] {
  switch (operation.kind) {
    case "tree":
    case "read_file":
    case "list_trash":
    case "glob":
    case "find":
    case "search_content":
    case "changes":
    case "usage":
      return ["read"];
    case "write_file":
    case "mkdir":
    case "copy":
    case "move":
    case "trash":
    case "remove":
    case "restore":
    case "purge":
      return ["mutation"];
    default:
      return exhaustive(operation);
  }
}

function exhaustive(value: never): never {
  throw new Error(`Unhandled public operation: ${JSON.stringify(value)}`);
}

function toGatewayResult(
  result: UpstreamResult<unknown>,
): GatewayResult<unknown> {
  return { data: result.data, activity: result.activity };
}

/** Server-side operation dispatcher; it exposes only finite, fixed client routes. */
export class ShowcaseGateway {
  private readonly now: () => number;
  private readonly statusCacheMs: number;
  private usageCache:
    { readonly data: unknown; readonly expiresAt: number } | undefined;
  private usagePending: Promise<unknown> | undefined;

  constructor(
    private readonly client: ShowcaseClient,
    private readonly limiter = new RollingWindowRateLimiter(),
    dependencies: ShowcaseGatewayDependencies = {},
  ) {
    this.now = dependencies.now ?? Date.now;
    this.statusCacheMs = dependencies.statusCacheMs ?? 1_000;
  }

  /**
   * Status is charged to the same read bucket as JSON reads and downloads.
   * The short process-wide cache and in-flight promise prevent many browser
   * polls from becoming matching upstream usage queries.
   */
  async statusUsage(clientIp: string): Promise<unknown> {
    this.enforce(clientIp, ["read"]);
    const now = this.now();
    if (this.usageCache && this.usageCache.expiresAt > now) {
      return this.usageCache.data;
    }
    if (this.usagePending) {
      return this.usagePending;
    }

    const pending = this.client.usage().then((result) => {
      this.usageCache = {
        data: result.data,
        expiresAt: this.now() + this.statusCacheMs,
      };
      return result.data;
    });
    this.usagePending = pending;
    try {
      return await pending;
    } finally {
      if (this.usagePending === pending) this.usagePending = undefined;
    }
  }

  async execute(
    input: unknown,
    clientIp: string,
  ): Promise<GatewayResult<unknown>> {
    const operation = parsePublicOperation(input);
    this.enforce(clientIp, readBuckets(operation));

    switch (operation.kind) {
      case "tree":
        return toGatewayResult(await this.client.tree(operation.path));
      case "read_file":
        return toGatewayResult(await this.client.readFile(operation.path));
      case "write_file":
        return toGatewayResult(
          await this.client.writeFile(
            operation.path,
            new TextEncoder().encode(operation.text),
            operation.expectedRevision,
          ),
        );
      case "mkdir":
        return toGatewayResult(
          await this.client.mkdir(operation.path, operation.parents),
        );
      case "copy":
        return toGatewayResult(
          await this.client.copy(operation.from, operation.to, {
            recursive: operation.recursive,
            expectedRevision: operation.expectedRevision,
          }),
        );
      case "move":
        return toGatewayResult(
          await this.client.move(operation.from, operation.to, {
            expectedRevision: operation.expectedRevision,
          }),
        );
      case "trash":
        return toGatewayResult(
          await this.client.trash(operation.path, operation.expectedRevision),
        );
      case "remove":
        return toGatewayResult(
          await this.client.remove(operation.path, {
            recursive: operation.recursive,
            expectedRevision: operation.expectedRevision,
          }),
        );
      case "list_trash":
        return toGatewayResult(await this.client.listTrash());
      case "restore":
        return toGatewayResult(
          await this.client.restore(
            operation.trashId,
            operation.destination,
            operation.expectedRevision,
          ),
        );
      case "purge":
        return this.purge(operation.trashId, operation.confirmedName);
      case "glob":
        return toGatewayResult(await this.client.glob(operation.pattern));
      case "find":
        return toGatewayResult(
          await this.client.find(operation.root, operation.nameContains),
        );
      case "search_content":
        return toGatewayResult(
          await this.client.searchContent(operation.root, operation.text),
        );
      case "changes":
        return toGatewayResult(await this.client.changes(operation.after));
      case "usage":
        return toGatewayResult(await this.client.usage());
      default:
        return exhaustive(operation);
    }
  }

  /**
   * Raw file uploads use a separate bucket and the mutation bucket together.
   * This method is intentionally separate from JSON write_file operations so
   * route code never has to construct an arbitrary upstream request.
   */
  async upload(
    path: unknown,
    bytes: Uint8Array,
    clientIp: string,
  ): Promise<GatewayResult<unknown>> {
    const validatedPath = virtualPathSchema.parse(path);
    if (bytes.byteLength > MAX_TEXT_BYTES) {
      throw new Error(`upload must not exceed ${MAX_TEXT_BYTES} bytes`);
    }
    this.enforce(clientIp, ["mutation", "upload"]);
    return toGatewayResult(await this.client.writeFile(validatedPath, bytes));
  }

  /**
   * Binary downloads are a separate route from JSON operations, but retain the
   * same canonical-path and per-IP read controls as read_file.
   */
  async download(
    path: unknown,
    clientIp: string,
  ): Promise<UpstreamResult<Uint8Array>> {
    const validatedPath = virtualPathSchema.parse(path);
    this.enforce(clientIp, ["read"]);
    return this.client.readFile(validatedPath);
  }

  private enforce(clientIp: string, buckets: readonly RateLimitBucket[]): void {
    const result = this.limiter.checkAll(clientIp, buckets);
    if (!result.allowed) {
      throwRateLimit(result);
    }
  }

  private async purge(
    trashId: string,
    confirmedName: string,
  ): Promise<GatewayResult<unknown>> {
    const listing = await this.client.listTrash();
    const name = trashEntryName(listing.data, trashId);
    if (name === undefined || name !== confirmedName) {
      throw new GatewayPurgeConfirmationError();
    }

    // Trash IDs are immutable. If the entry disappears after this lookup, the
    // fixed purge request fails upstream rather than targeting another entry.
    return toGatewayResult(await this.client.purge(trashId));
  }
}

function throwRateLimit(result: RateLimitResult): never {
  throw new GatewayRateLimitError(result.bucket ?? "read", result.retryAfterMs);
}

function trashEntryName(data: unknown, trashId: string): string | undefined {
  if (!data || typeof data !== "object" || !("items" in data)) {
    return undefined;
  }
  const { items } = data;
  if (!Array.isArray(items)) {
    return undefined;
  }
  for (const entry of items) {
    if (
      !entry ||
      typeof entry !== "object" ||
      !("id" in entry) ||
      entry.id !== trashId
    ) {
      continue;
    }
    if (!("node" in entry) || !entry.node || typeof entry.node !== "object") {
      return undefined;
    }
    if (!("name" in entry.node) || typeof entry.node.name !== "string") {
      return undefined;
    }
    return entry.node.name;
  }
  return undefined;
}
