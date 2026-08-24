import type { APIRoute } from "astro";

import {
  gatewayErrorResponse,
  json,
  methodNotAllowed,
} from "../../lib/server/http";
import {
  getShowcaseRuntime,
  type ShowcaseRuntimeStatus,
} from "../../lib/server/runtime";
import type {
  PublicWorkspaceUsage,
  WorkspaceUsage,
} from "../../lib/shared/contracts";
import { loadServerConfig } from "../../lib/server/config";
import { clientIp } from "../../lib/server/http";

export const prerender = false;

/** The status endpoint is a deliberately narrow browser DTO. */
export interface PublicShowcaseStatus {
  ready: true;
  generation: number;
  resetting: boolean;
  nextResetAt: number | null;
  now: number;
  usage: PublicWorkspaceUsage;
}

function publicStatus(status: ShowcaseRuntimeStatus): PublicShowcaseStatus {
  const source = status.usage as WorkspaceUsage;
  const usage: PublicWorkspaceUsage = {
    active_logical_bytes: source.active_logical_bytes,
    trashed_logical_bytes: source.trashed_logical_bytes,
    staged_bytes: source.staged_bytes,
    active_nodes: source.active_nodes,
    trashed_nodes: source.trashed_nodes,
    max_logical_bytes: source.max_logical_bytes,
    max_nodes: source.max_nodes,
    max_file_bytes: source.max_file_bytes,
  };
  return {
    ready: status.ready,
    generation: status.generation,
    resetting: status.resetting,
    nextResetAt: status.nextResetAt,
    now: Date.now(),
    usage,
  };
}

export const GET: APIRoute = async ({ request, clientAddress }) => {
  try {
    const config = loadServerConfig();
    const status = await (
      await getShowcaseRuntime()
    ).status(clientIp(request, clientAddress, config.trustProxy));
    return json(publicStatus(status));
  } catch (error) {
    return gatewayErrorResponse(error);
  }
};

export const ALL: APIRoute = () => methodNotAllowed("GET");
