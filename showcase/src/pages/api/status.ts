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

export const prerender = false;

/** The status endpoint is a deliberately narrow browser DTO. */
export interface PublicShowcaseStatus {
  ready: true;
  generation: number;
  resetting: boolean;
  nextResetAt: number | null;
  now: number;
  usage: unknown;
}

function publicStatus(status: ShowcaseRuntimeStatus): PublicShowcaseStatus {
  return {
    ready: status.ready,
    generation: status.generation,
    resetting: status.resetting,
    nextResetAt: status.nextResetAt,
    now: Date.now(),
    usage: status.usage,
  };
}

export const GET: APIRoute = async () => {
  try {
    const status = await (await getShowcaseRuntime()).status();
    return json(publicStatus(status));
  } catch (error) {
    return gatewayErrorResponse(error);
  }
};

export const ALL: APIRoute = () => methodNotAllowed("GET");
