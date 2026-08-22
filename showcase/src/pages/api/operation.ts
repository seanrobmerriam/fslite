import type { APIRoute } from "astro";

import { loadServerConfig } from "../../lib/server/config";
import {
  clientIp,
  gatewayErrorResponse,
  isJsonRequest,
  json,
  methodNotAllowed,
  PublicRequestError,
  readBoundedBody,
} from "../../lib/server/http";
import { getShowcaseRuntime } from "../../lib/server/runtime";

export const prerender = false;

export const POST: APIRoute = async ({ request, clientAddress }) => {
  if (!isJsonRequest(request)) {
    return gatewayErrorResponse(
      new PublicRequestError(
        415,
        "unsupported_media_type",
        "Content-Type must be application/json.",
      ),
    );
  }

  try {
    const bytes = await readBoundedBody(request);
    const input = JSON.parse(new TextDecoder().decode(bytes)) as unknown;
    const config = loadServerConfig();
    const runtime = await getShowcaseRuntime();
    const result = await runtime.execute(
      input,
      clientIp(request, clientAddress, config.trustProxy),
    );
    return json(result);
  } catch (error) {
    return gatewayErrorResponse(error);
  }
};

export const ALL: APIRoute = () => methodNotAllowed("POST");
