import type { APIRoute } from "astro";

import {
  gatewayErrorResponse,
  json,
  methodNotAllowed,
} from "../../lib/server/http";
import { getShowcaseRuntime } from "../../lib/server/runtime";

export const prerender = false;

export const GET: APIRoute = async () => {
  try {
    const status = await (await getShowcaseRuntime()).status();
    return json({ ...status, now: Date.now() });
  } catch (error) {
    return gatewayErrorResponse(error);
  }
};

export const ALL: APIRoute = () => methodNotAllowed("GET");
