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
  readFile(path: VirtualPath): Promise<UpstreamResult<unknown>>;
  writeFile(
    path: VirtualPath,
    bytes: Uint8Array,
    expectedRevision?: number,
  ): Promise<UpstreamResult<unknown>>;
  mkdir(path: VirtualPath, parents: boolean): Promise<UpstreamResult<unknown>>;
  copy(
    from: VirtualPath,
    to: VirtualPath,
    options: { recursive: boolean },
  ): Promise<UpstreamResult<unknown>>;
  move(from: VirtualPath, to: VirtualPath): Promise<UpstreamResult<unknown>>;
  trash(
    path: VirtualPath,
    expectedRevision?: number,
  ): Promise<UpstreamResult<unknown>>;
  remove(
    path: VirtualPath,
    options: { recursive: boolean },
  ): Promise<UpstreamResult<unknown>>;
  listTrash(): Promise<UpstreamResult<unknown>>;
  restore(
    trashId: string,
    destination?: VirtualPath,
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
  constructor(
    private readonly client: ShowcaseClient,
    private readonly limiter = new RollingWindowRateLimiter(),
  ) {}

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
          }),
        );
      case "move":
        return toGatewayResult(
          await this.client.move(operation.from, operation.to),
        );
      case "trash":
        return toGatewayResult(
          await this.client.trash(operation.path, operation.expectedRevision),
        );
      case "remove":
        return toGatewayResult(
          await this.client.remove(operation.path, {
            recursive: operation.recursive,
          }),
        );
      case "list_trash":
        return toGatewayResult(await this.client.listTrash());
      case "restore":
        return toGatewayResult(
          await this.client.restore(operation.trashId, operation.destination),
        );
      case "purge":
        return toGatewayResult(await this.client.purge(operation.trashId));
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

  private enforce(clientIp: string, buckets: readonly RateLimitBucket[]): void {
    const result = this.limiter.checkAll(clientIp, buckets);
    if (!result.allowed) {
      throwRateLimit(result);
    }
  }
}

function throwRateLimit(result: RateLimitResult): never {
  throw new GatewayRateLimitError(result.bucket ?? "read", result.retryAfterMs);
}
