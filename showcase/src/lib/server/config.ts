import { readFileSync } from "node:fs";

export interface ServerConfig {
  serverUrl: URL;
  token: string;
  resetIntervalMs: number;
  requestTimeoutMs: number;
  trustProxy: boolean;
}

type Environment = Record<string, string | undefined>;
type ReadTokenFile = (path: string) => string;

const DEFAULT_SERVER_URL = "http://fslite-server:8080";
const DEFAULT_RESET_INTERVAL_MS = 900_000;
const DEFAULT_REQUEST_TIMEOUT_MS = 10_000;

function positiveInteger(
  value: string | undefined,
  fallback: number,
  name: string,
): number {
  if (value === undefined || value.trim() === "") {
    return fallback;
  }

  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return parsed;
}

function normalizedUrl(rawUrl: string): URL {
  let url: URL;
  try {
    url = new URL(rawUrl.trim());
  } catch {
    throw new Error("FSLITE_SERVER_URL must be an absolute URL");
  }

  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("FSLITE_SERVER_URL must use http or https");
  }
  if (url.search || url.hash) {
    throw new Error("FSLITE_SERVER_URL must not include a query or fragment");
  }

  url.pathname = url.pathname.replace(/\/+$/, "") || "/";
  return url;
}

/**
 * Loads private runtime configuration. This module is deliberately only
 * imported by server modules; neither token source may cross the Astro API
 * boundary.
 */
export function loadServerConfig(
  environment: Environment = process.env,
  readTokenFile: ReadTokenFile = (path) => readFileSync(path, "utf8"),
): ServerConfig {
  const tokenFile = environment.FSLITE_TOKEN_FILE?.trim();
  const rawToken = tokenFile
    ? readTokenFile(tokenFile)
    : environment.FSLITE_TOKEN;
  const token = rawToken?.trim() ?? "";

  if (!token) {
    throw new Error("FSLITE_TOKEN is required");
  }

  return {
    serverUrl: normalizedUrl(
      environment.FSLITE_SERVER_URL ?? DEFAULT_SERVER_URL,
    ),
    token,
    resetIntervalMs: positiveInteger(
      environment.FSLITE_RESET_INTERVAL_MS,
      DEFAULT_RESET_INTERVAL_MS,
      "FSLITE_RESET_INTERVAL_MS",
    ),
    requestTimeoutMs: positiveInteger(
      environment.FSLITE_REQUEST_TIMEOUT_MS,
      DEFAULT_REQUEST_TIMEOUT_MS,
      "FSLITE_REQUEST_TIMEOUT_MS",
    ),
    trustProxy: environment.TRUST_PROXY?.trim().toLowerCase() === "true",
  };
}
