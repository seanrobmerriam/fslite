import type { APIRoute } from "astro";

import { loadServerConfig } from "../../lib/server/config";
import {
  clientIp,
  gatewayErrorResponse,
  json,
  methodNotAllowed,
  readBoundedBody,
  validateQueryPath,
} from "../../lib/server/http";
import { getShowcaseRuntime } from "../../lib/server/runtime";

export const prerender = false;

export const POST: APIRoute = async ({ request, url, clientAddress }) => {
  try {
    const path = validateQueryPath(url.searchParams.get("path"));
    const bytes = await readBoundedBody(request);
    const config = loadServerConfig();
    const result = await (
      await getShowcaseRuntime()
    ).upload(path, bytes, clientIp(request, clientAddress, config.trustProxy));
    return json(result);
  } catch (error) {
    return gatewayErrorResponse(error);
  }
};

export const ALL: APIRoute = () => methodNotAllowed("POST");
