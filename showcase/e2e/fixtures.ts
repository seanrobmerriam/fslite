import { randomUUID } from "node:crypto";
import {
  access,
  chmod,
  mkdtemp,
  realpath,
  rm,
  writeFile,
} from "node:fs/promises";
import { createServer, type Server } from "node:net";
import {
  createServer as createHttpServer,
  request as httpRequest,
  type Server as HttpServer,
} from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  spawn,
  type ChildProcess,
  type SpawnOptions,
} from "node:child_process";

import { expect, test as base } from "@playwright/test";

const SHOWCASE_DIR = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const REPOSITORY_DIR = resolve(SHOWCASE_DIR, "..");
const TEMP_PREFIX = "fslite-showcase-e2e-";
const LOG_LIMIT = 24 * 1024;

interface CappedLog {
  readonly raw: () => string;
  readonly read: () => string;
}

export interface E2eFixture {
  readonly baseURL: string;
  readonly fixtureDir: string;
  readonly token: string;
  readonly resetIntervalMs: number;
  logs(): { rust: string; showcase: string };
  /** Redacted, bounded process diagnostics safe to attach to a test failure. */
  diagnostics(): string;
  request(path: string, init?: RequestInit): Promise<Response>;
}

type E2eOptions = { resetIntervalMs: number; resetResponseDelayMs: number };

function cappedLog(child: ChildProcess, secret: string): CappedLog {
  let output = "";
  const append = (chunk: Buffer) => {
    output = `${output}${chunk.toString("utf8")}`.slice(-LOG_LIMIT);
  };
  child.stdout?.on("data", append);
  child.stderr?.on("data", append);
  return {
    raw: () => output,
    read: () =>
      output
        .replaceAll(secret, "[REDACTED]")
        .replaceAll("fslite-server:8080", "[REDACTED_UPSTREAM]"),
  };
}

export async function freeLoopbackPort(
  createListener: () => Server = createServer,
): Promise<number> {
  const listener = createListener();
  try {
    await new Promise<void>((resolveListening, rejectListening) => {
      listener.once("listening", resolveListening);
      listener.once("error", rejectListening);
      listener.listen({ host: "127.0.0.1", port: 0 });
    });
    const address = listener.address();
    if (!address || typeof address === "string") {
      throw new Error("could not allocate an IPv4 loopback port");
    }
    return address.port;
  } finally {
    await new Promise<void>((resolveClose) =>
      listener.close(() => resolveClose()),
    );
  }
}

async function waitFor(
  url: string,
  label: string,
  diagnostics: () => string,
  headers?: HeadersInit,
): Promise<void> {
  const deadline = Date.now() + 45_000;
  let lastFailure = "no response";
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url, {
        headers,
        signal: AbortSignal.timeout(1_000),
      });
      if (response.ok) return;
      lastFailure = `HTTP ${response.status}`;
    } catch (error) {
      lastFailure =
        error instanceof Error ? error.message : "connection failed";
    }
    await new Promise<void>((resolveDelay) => setTimeout(resolveDelay, 50));
  }
  throw new Error(
    `${label} did not become ready (${lastFailure}).\n${diagnostics()}`,
  );
}

async function assertFixtureDirectory(directory: string): Promise<string> {
  const resolved = await realpath(directory);
  const temporary = await realpath(tmpdir());
  if (
    !resolved.startsWith(`${temporary}/`) ||
    !resolved.split("/").at(-1)?.startsWith(TEMP_PREFIX) ||
    relative(temporary, resolved).startsWith("..")
  ) {
    throw new Error("refusing to clean a directory outside this E2E fixture");
  }
  return resolved;
}

export interface StopProcessGroupDependencies {
  kill?: (pid: number, signal?: NodeJS.Signals | 0) => void;
  waitForExit?: (child: ChildProcess, timeoutMs: number) => Promise<boolean>;
  timeoutMs?: number;
}

function isEsrch(error: unknown): boolean {
  return (error as NodeJS.ErrnoException | undefined)?.code === "ESRCH";
}

/** Waits only for a bounded interval; fixture teardown must never hang on exit. */
export function waitForProcessExit(
  child: ChildProcess,
  timeoutMs: number,
): Promise<boolean> {
  if (child.exitCode !== null || child.signalCode !== null) {
    return Promise.resolve(true);
  }
  return new Promise((resolveExited) => {
    const onExit = () => finish(true);
    const timer = setTimeout(() => finish(false), timeoutMs);
    const finish = (exited: boolean) => {
      clearTimeout(timer);
      child.off("exit", onExit);
      resolveExited(exited);
    };
    child.once("exit", onExit);
  });
}

export async function stopProcessGroup(
  child: ChildProcess | undefined,
  dependencies: StopProcessGroupDependencies = {},
): Promise<void> {
  if (!child?.pid || child.exitCode !== null || child.signalCode !== null)
    return;
  const kill = dependencies.kill ?? process.kill;
  const waitForExit = dependencies.waitForExit ?? waitForProcessExit;
  const timeoutMs = dependencies.timeoutMs ?? 5_000;
  const pid = child.pid;
  try {
    kill(pid, 0);
  } catch (error) {
    if (isEsrch(error)) return;
    throw error;
  }
  try {
    kill(-pid, "SIGTERM");
  } catch (error) {
    if (isEsrch(error)) return;
    throw error;
  }
  if (await waitForExit(child, timeoutMs)) return;
  try {
    kill(-pid, "SIGKILL");
  } catch (error) {
    if (isEsrch(error)) return;
    throw error;
  }
  await waitForExit(child, timeoutMs);
}

export interface FixtureLifecycle {
  addCleanup(cleanup: () => Promise<void>): void;
}

/** Runs every registered cleanup in reverse order, even if setup or cleanup fails. */
export async function runFixtureLifecycle<T>(
  setup: (lifecycle: FixtureLifecycle) => Promise<T>,
  use: (value: T) => Promise<void>,
): Promise<void> {
  const cleanups: Array<() => Promise<void>> = [];
  let primaryError: unknown;
  try {
    await use(
      await setup({
        addCleanup: (cleanup) => cleanups.push(cleanup),
      }),
    );
  } catch (error) {
    primaryError = error;
  }
  const cleanupErrors: unknown[] = [];
  for (const cleanup of cleanups.toReversed()) {
    try {
      await cleanup();
    } catch (error) {
      cleanupErrors.push(error);
    }
  }
  if (primaryError !== undefined || cleanupErrors.length > 0) {
    throw new AggregateError(
      primaryError === undefined
        ? cleanupErrors
        : [primaryError, ...cleanupErrors],
      "E2E fixture setup or cleanup failed",
    );
  }
}

export interface SpawnCheckedDependencies {
  spawn?: (
    command: string,
    args: readonly string[],
    options: SpawnOptions,
  ) => ChildProcess;
}

export async function spawnChecked(
  command: string,
  args: readonly string[],
  options: SpawnOptions,
  dependencies: SpawnCheckedDependencies = {},
): Promise<ChildProcess> {
  const child = (dependencies.spawn ?? spawn)(command, args, options);
  await new Promise<void>((resolveSpawn, rejectSpawn) => {
    child.once("spawn", resolveSpawn);
    child.once("error", rejectSpawn);
  });
  // Keep an error listener for an asynchronous exec failure after `spawn`.
  child.on("error", () => undefined);
  return child;
}

async function startResetDelayProxy(
  target: string,
  port: number,
  delayMs: number,
): Promise<HttpServer> {
  const targetUrl = new URL(target);
  const proxy = createHttpServer((client, clientResponse) => {
    const upstream = httpRequest(
      {
        host: targetUrl.hostname,
        port: targetUrl.port,
        method: client.method,
        path: client.url,
        headers: client.headers,
      },
      (upstreamResponse) => {
        clientResponse.writeHead(
          upstreamResponse.statusCode ?? 502,
          upstreamResponse.headers,
        );
        const pause = client.url?.endsWith("/reset") ? delayMs : 0;
        setTimeout(() => upstreamResponse.pipe(clientResponse), pause);
      },
    );
    upstream.on("error", (error) => clientResponse.destroy(error));
    client.pipe(upstream);
  });
  await new Promise<void>((resolveListening, rejectListening) => {
    proxy.once("listening", resolveListening);
    proxy.once("error", rejectListening);
    proxy.listen({ host: "127.0.0.1", port });
  });
  return proxy;
}

async function stopServer(server: HttpServer | undefined): Promise<void> {
  if (!server) return;
  await new Promise<void>((resolveClose, rejectClose) =>
    server.close((error) => (error ? rejectClose(error) : resolveClose())),
  );
}

function processEnvironment(values: Record<string, string>): NodeJS.ProcessEnv {
  return { ...process.env, ...values };
}

function assertNodeVersion(): void {
  const [major, minor] = process.versions.node.split(".").map(Number);
  if (major < 22 || (major === 22 && minor < 12)) {
    throw new Error("E2E requires Node.js >=22.12.0");
  }
}

export const test = base.extend<E2eOptions & { e2e: E2eFixture }>({
  resetIntervalMs: [300_000, { option: true }],
  resetResponseDelayMs: [0, { option: true }],
  e2e: async ({ resetIntervalMs, resetResponseDelayMs }, use) => {
    const fixtureDir = await mkdtemp(join(tmpdir(), TEMP_PREFIX));
    let checkedFixtureDir: string | undefined;
    let rust: ChildProcess | undefined;
    let showcase: ChildProcess | undefined;
    let rustLog: CappedLog | undefined;
    let showcaseLog: CappedLog | undefined;
    let resetProxy: HttpServer | undefined;
    await runFixtureLifecycle(async (lifecycle) => {
      checkedFixtureDir = await assertFixtureDirectory(fixtureDir);
      lifecycle.addCleanup(async () => {
        const directory = await assertFixtureDirectory(
          checkedFixtureDir ?? fixtureDir,
        );
        await rm(directory, { recursive: true, force: true });
      });
      assertNodeVersion();
      const token = `e2e-${randomUUID().replaceAll("-", "")}`;
      const tokenFile = join(checkedFixtureDir, "token");
      const database = join(checkedFixtureDir, "fslite.db");
      const config = join(checkedFixtureDir, "server.json");
      const [serverPort, showcasePort] = await Promise.all([
        freeLoopbackPort(),
        freeLoopbackPort(),
      ]);
      const serverUrl = `http://127.0.0.1:${serverPort}`;
      const baseURL = `http://127.0.0.1:${showcasePort}`;
      await writeFile(tokenFile, `${token}\n`, { mode: 0o600 });
      await chmod(tokenFile, 0o600);
      await access(resolve(SHOWCASE_DIR, "dist/server/entry.mjs"));

      rust = await spawnChecked(
        "cargo",
        [
          "run",
          "-p",
          "fslite-server",
          "--",
          "--db",
          database,
          "--config",
          config,
          "--bind",
          `127.0.0.1:${serverPort}`,
          "--token-file",
          tokenFile,
          "--max-bytes",
          String(10 * 1024 * 1024),
          "--max-nodes",
          "250",
          "--max-file-bytes",
          String(1024 * 1024),
        ],
        {
          cwd: REPOSITORY_DIR,
          detached: true,
          stdio: ["ignore", "pipe", "pipe"],
          env: processEnvironment({ RUSTUP_TOOLCHAIN: "1.85.0" }),
        },
      );
      lifecycle.addCleanup(() => stopProcessGroup(rust));
      rustLog = cappedLog(rust, token);
      await waitFor(
        `${serverUrl}/readyz`,
        "fslite-server",
        () => rustLog?.read() ?? "",
        {
          Authorization: `Bearer ${token}`,
        },
      );
      let gatewayUrl = serverUrl;
      if (resetResponseDelayMs > 0) {
        const proxyPort = await freeLoopbackPort();
        resetProxy = await startResetDelayProxy(
          serverUrl,
          proxyPort,
          resetResponseDelayMs,
        );
        lifecycle.addCleanup(() => stopServer(resetProxy));
        gatewayUrl = `http://127.0.0.1:${proxyPort}`;
      }

      showcase = await spawnChecked(
        process.execPath,
        ["./dist/server/entry.mjs"],
        {
          cwd: SHOWCASE_DIR,
          detached: true,
          stdio: ["ignore", "pipe", "pipe"],
          env: processEnvironment({
            HOST: "127.0.0.1",
            PORT: String(showcasePort),
            FSLITE_SERVER_URL: gatewayUrl,
            FSLITE_TOKEN_FILE: tokenFile,
            FSLITE_RESET_INTERVAL_MS: String(resetIntervalMs),
            FSLITE_TRUST_PROXY: "true",
            FSLITE_REQUEST_TIMEOUT_MS: "5000",
          }),
        },
      );
      lifecycle.addCleanup(() => stopProcessGroup(showcase));
      showcaseLog = cappedLog(showcase, token);
      await waitFor(
        `${baseURL}/api/health/ready`,
        "showcase",
        () =>
          `Rust:\n${rustLog?.read() ?? ""}\nAstro:\n${showcaseLog?.read() ?? ""}`,
      );

      return {
        baseURL,
        fixtureDir: checkedFixtureDir,
        token,
        resetIntervalMs,
        logs: () => ({
          rust: rustLog?.raw() ?? "",
          showcase: showcaseLog?.raw() ?? "",
        }),
        diagnostics: () =>
          `Rust process diagnostics:\n${rustLog?.read() ?? ""}\nAstro process diagnostics:\n${showcaseLog?.read() ?? ""}`,
        request: (path, init) => fetch(new URL(path, baseURL), init),
      };
    }, use);
  },
});

export { expect };
