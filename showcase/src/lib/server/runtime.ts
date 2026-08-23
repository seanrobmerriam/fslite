import type { GatewayResult } from "../shared/contracts";
import type { VirtualPath } from "../shared/path";
import { loadServerConfig, type ServerConfig } from "./config";
import {
  FsliteClient,
  type Identity,
  type UpstreamResult,
} from "./fslite-client";
import { ShowcaseGateway, type ShowcaseClient } from "./gateway";
import { ResetCoordinator, type ResetSnapshot } from "./reset-coordinator";
import { seedWorkspace } from "./seed";

interface RuntimeClient extends ShowcaseClient {
  identity(): Promise<UpstreamResult<Identity>>;
  resetWorkspace(): Promise<unknown>;
  readFile(path: VirtualPath): Promise<UpstreamResult<Uint8Array>>;
}

interface RuntimeGateway {
  execute(input: unknown, clientIp: string): Promise<GatewayResult<unknown>>;
  upload(
    path: unknown,
    bytes: Uint8Array,
    clientIp: string,
  ): Promise<GatewayResult<unknown>>;
  download(
    path: unknown,
    clientIp: string,
  ): Promise<UpstreamResult<Uint8Array>>;
}

interface RuntimeCoordinator {
  start(): Promise<void>;
  snapshot(): ResetSnapshot;
  withOperation<T>(operation: () => Promise<T>): Promise<T>;
}

export interface RuntimeDependencies {
  loadConfig?: () => ServerConfig;
  createClient?: (config: ServerConfig) => RuntimeClient;
  createGateway?: (client: RuntimeClient) => RuntimeGateway;
  createCoordinator?: (
    client: RuntimeClient,
    config: ServerConfig,
  ) => RuntimeCoordinator;
}

export interface ShowcaseRuntimeStatus extends ResetSnapshot {
  ready: true;
  workspaceId: string;
  usage: unknown;
}

export interface ShowcaseRuntime {
  readonly workspaceId: string;
  liveness(): { ok: true };
  readiness(): Promise<{ ready: true; workspaceId: string }>;
  status(): Promise<ShowcaseRuntimeStatus>;
  execute(input: unknown, clientIp: string): Promise<GatewayResult<unknown>>;
  upload(
    path: unknown,
    bytes: Uint8Array,
    clientIp: string,
  ): Promise<GatewayResult<unknown>>;
  download(
    path: unknown,
    clientIp: string,
  ): Promise<UpstreamResult<Uint8Array>>;
}

class ProcessShowcaseRuntime implements ShowcaseRuntime {
  constructor(
    readonly workspaceId: string,
    private readonly client: RuntimeClient,
    private readonly gateway: RuntimeGateway,
    private readonly coordinator: RuntimeCoordinator,
  ) {}

  liveness(): { ok: true } {
    return { ok: true };
  }

  async readiness(): Promise<{ ready: true; workspaceId: string }> {
    return { ready: true, workspaceId: this.workspaceId };
  }

  async status(): Promise<ShowcaseRuntimeStatus> {
    // Status is observational and must remain available while the coordinator
    // closes the mutation gate, otherwise the browser cannot render its reset
    // banner or disable controls deterministically.
    const usage = await this.client.usage();
    return {
      ready: true,
      workspaceId: this.workspaceId,
      ...this.coordinator.snapshot(),
      usage: usage.data,
    };
  }

  execute(input: unknown, clientIp: string): Promise<GatewayResult<unknown>> {
    return this.coordinator.withOperation(() =>
      this.gateway.execute(input, clientIp),
    );
  }

  upload(
    path: unknown,
    bytes: Uint8Array,
    clientIp: string,
  ): Promise<GatewayResult<unknown>> {
    return this.coordinator.withOperation(() =>
      this.gateway.upload(path, bytes, clientIp),
    );
  }

  download(
    path: unknown,
    clientIp: string,
  ): Promise<UpstreamResult<Uint8Array>> {
    return this.coordinator.withOperation(() =>
      this.gateway.download(path, clientIp),
    );
  }
}

export const SHOWCASE_RUNTIME_SYMBOL = Symbol.for("fslite.showcase.runtime");

function validateIdentity(identity: Identity): string {
  const workspaceId = identity.workspace_id.trim();
  if (!workspaceId) {
    throw new Error("The upstream identity did not include a workspace_id");
  }
  if (!identity.capabilities.includes("workspace_admin")) {
    throw new Error("The upstream identity lacks workspace_admin capability");
  }
  return workspaceId;
}

async function initializeRuntime(
  dependencies: RuntimeDependencies,
): Promise<ShowcaseRuntime> {
  const config = (dependencies.loadConfig ?? loadServerConfig)();
  const client = (
    dependencies.createClient ?? ((value) => new FsliteClient(value))
  )(config);
  const identity = await client.identity();
  const workspaceId = validateIdentity(identity.data);
  const gateway = (
    dependencies.createGateway ?? ((value) => new ShowcaseGateway(value))
  )(client);
  const coordinator = (
    dependencies.createCoordinator ??
    ((value, valueConfig) =>
      new ResetCoordinator(value, () => seedWorkspace(value), {
        resetIntervalMs: valueConfig.resetIntervalMs,
      }))
  )(client, config);

  await coordinator.start();
  return new ProcessShowcaseRuntime(workspaceId, client, gateway, coordinator);
}

/**
 * Lazily initializes the one process-wide runtime. A rejected initialization is
 * deliberately removed so a transient upstream failure can recover on retry.
 */
export function getShowcaseRuntime(
  dependencies: RuntimeDependencies = {},
): Promise<ShowcaseRuntime> {
  const store = globalThis as typeof globalThis &
    Record<symbol, Promise<ShowcaseRuntime> | undefined>;
  const existing = store[SHOWCASE_RUNTIME_SYMBOL];
  if (existing) {
    return existing;
  }

  const pending = initializeRuntime(dependencies);
  store[SHOWCASE_RUNTIME_SYMBOL] = pending;
  void pending.catch(() => {
    if (store[SHOWCASE_RUNTIME_SYMBOL] === pending) {
      delete store[SHOWCASE_RUNTIME_SYMBOL];
    }
  });
  return pending;
}
