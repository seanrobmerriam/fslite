import type { APIRoute } from "astro";

import { json, methodNotAllowed } from "../../../lib/server/http";

export const prerender = false;

/** This must remain independent from upstream runtime initialization. */
export const GET: APIRoute = () => json({ ok: true });

export const ALL: APIRoute = () => methodNotAllowed("GET");
