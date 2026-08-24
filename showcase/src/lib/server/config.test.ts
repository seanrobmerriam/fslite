import { describe, expect, it } from "vitest";

import { loadServerConfig } from "./config";

describe("loadServerConfig", () => {
  it("prefers a trimmed token file over FSLITE_TOKEN", () => {
    const config = loadServerConfig(
      {
        FSLITE_SERVER_URL: "http://fslite-server:8080/",
        FSLITE_TOKEN_FILE: "/run/secrets/fslite-token",
        FSLITE_TOKEN: "environment-token",
      },
      () => "  file-token\n",
    );

    expect(config.token).toBe("file-token");
    expect(config.serverUrl.toString()).toBe("http://fslite-server:8080/");
  });

  it("trims an environment token and applies the safe defaults", () => {
    const config = loadServerConfig({ FSLITE_TOKEN: "  token\t " });

    expect(config).toMatchObject({
      token: "token",
      resetIntervalMs: 900_000,
      requestTimeoutMs: 10_000,
      trustProxy: false,
    });
    expect(config.serverUrl.toString()).toBe("http://fslite-server:8080/");
  });

  it.each([
    [{}, undefined],
    [{ FSLITE_TOKEN: "   " }, undefined],
    [{ FSLITE_TOKEN_FILE: "/empty" }, () => " \n"],
  ])("rejects a missing or empty bearer secret", (environment, readFile) => {
    expect(() => loadServerConfig(environment, readFile)).toThrow(
      "FSLITE_TOKEN is required",
    );
  });

  it("normalizes the configured URL and parses optional runtime settings", () => {
    const config = loadServerConfig({
      FSLITE_SERVER_URL: "https://upstream.example.test/base///",
      FSLITE_TOKEN: "token",
      FSLITE_RESET_INTERVAL_MS: "1200",
      FSLITE_REQUEST_TIMEOUT_MS: "750",
      FSLITE_TRUST_PROXY: "true",
    });

    expect(config.serverUrl.toString()).toBe(
      "https://upstream.example.test/base",
    );
    expect(config.resetIntervalMs).toBe(1200);
    expect(config.requestTimeoutMs).toBe(750);
    expect(config.trustProxy).toBe(true);
  });

  it.each([
    ["true", true],
    [" FALSE ", false],
  ])("parses FSLITE_TRUST_PROXY=%s", (value, expected) => {
    const config = loadServerConfig({
      FSLITE_TOKEN: "token",
      FSLITE_TRUST_PROXY: value,
    });

    expect(config.trustProxy).toBe(expected);
  });

  it("defaults proxy trust to false and ignores the accidental legacy variable", () => {
    expect(loadServerConfig({ FSLITE_TOKEN: "token" }).trustProxy).toBe(false);
    expect(
      loadServerConfig({ FSLITE_TOKEN: "token", TRUST_PROXY: "true" })
        .trustProxy,
    ).toBe(false);
  });

  it.each(["1", "yes", "", "falsey"])(
    "rejects invalid FSLITE_TRUST_PROXY=%s",
    (value) => {
      expect(() =>
        loadServerConfig({ FSLITE_TOKEN: "token", FSLITE_TRUST_PROXY: value }),
      ).toThrow("FSLITE_TRUST_PROXY must be true or false");
    },
  );
});
