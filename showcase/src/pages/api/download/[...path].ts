import type { APIRoute } from "astro";

import { loadServerConfig } from "../../../lib/server/config";
import {
  clientIp,
  decodeCanonicalPath,
  gatewayErrorResponse,
  MAX_REQUEST_BYTES,
  methodNotAllowed,
  ResponseTooLargeError,
} from "../../../lib/server/http";
import { getShowcaseRuntime } from "../../../lib/server/runtime";

export const prerender = false;

function headerText(value: string | number): string {
  return String(value).replace(/[\r\n\0]/g, "_");
}

function downloadFilename(path: string): string {
  const basename = path.split("/").at(-1) || "download";
  return basename.replace(/[\\/"\r\n\0]/g, "_");
}

export const GET: APIRoute = async ({ request, params, clientAddress }) => {
  try {
    const path = decodeCanonicalPath(params.path, true);
    const config = loadServerConfig();
    const result = await (
      await getShowcaseRuntime()
    ).download(path, clientIp(request, clientAddress, config.trustProxy));
    if (result.data.byteLength > MAX_REQUEST_BYTES) {
      throw new ResponseTooLargeError(MAX_REQUEST_BYTES);
    }

    return new Response(result.data as unknown as BodyInit, {
      headers: {
        "content-type": "application/octet-stream",
        "content-disposition": `attachment; filename="${downloadFilename(path)}"`,
        "x-fslite-method": headerText(result.activity.method),
        "x-fslite-path": headerText(result.activity.path),
        "x-fslite-status": headerText(result.activity.status),
        "x-fslite-duration-ms": headerText(result.activity.durationMs),
        "x-request-id": headerText(result.activity.requestId),
      },
    });
  } catch (error) {
    return gatewayErrorResponse(error);
  }
};

export const ALL: APIRoute = () => methodNotAllowed("GET");
