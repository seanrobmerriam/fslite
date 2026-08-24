# fslite Showcase deployment

This is a focused, self-contained Docker Compose reference for the disposable
Astro showcase. Caddy is the only service with a host port. Astro and
`fslite-server` communicate only on Compose's private default network, and the
bearer token is mounted read-only into those two server-side containers.

## Start the reference stack

Run these commands from the repository root. Docker Compose v2 is required.

```sh
openssl rand -hex 32 > deploy/showcase/fslite-token
chmod 0600 deploy/showcase/fslite-token
docker compose -f deploy/showcase/compose.yml up -d --build
curl --fail --insecure --resolve localhost:443:127.0.0.1 \
  https://localhost/api/health/ready
```

`openssl rand -hex 32` produces a 64-hex-character token. The checked-in
`fslite-token.example` is a non-secret placeholder; never use it as a token.
The real `deploy/showcase/fslite-token` file is ignored by Git.

The default hostname is `localhost`; Caddy serves it with its locally managed
certificate. The `--insecure` flag above is only for this local Caddy
certificate. There is no plain-HTTP proxy route. Set `SHOWCASE_HOSTNAME` to a
public DNS name before starting the stack:

```sh
SHOWCASE_HOSTNAME=showcase.example.test \
  docker compose -f deploy/showcase/compose.yml up -d --build
```

Ports 80 and 443 must reach Caddy, and the hostname's A/AAAA records must point
at the deployment. Caddy then obtains and renews the public certificate
automatically and redirects public HTTP traffic to HTTPS. If an existing Caddy
deployment already owns certificates, copy the `showcase` snippet and hostname
site block into that deployment instead of running a second public Caddy. Do
not publish Astro or `fslite-server` directly.

Keep the snippet's `X-Forwarded-For {remote_host}` override. Astro trusts this
one Caddy hop when applying per-visitor rate limits, so forwarding an incoming
visitor-supplied `X-Forwarded-For` chain would make those limits spoofable.

`/api/health/live` proves that the Astro process is running without calling
the backend. `/api/health/ready` additionally verifies Astro's authenticated
private connection to `fslite-server` and can take a short time after startup
because the showcase resets and seeds its workspace once.

The reference service creates its shared workspace with hard limits of 10 MiB
total content, 250 nodes, and 1 MiB per file. These server-side quotas are in
addition to Astro's request-size and per-visitor rate limits.

To stop the containers while preserving the `fslite_data` volume:

```sh
docker compose -f deploy/showcase/compose.yml down
```

Do not add `-v` to ordinary teardown. The volume contains the server database
and its persisted workspace configuration.

## Integrate with an existing `docker-caddy-astro` stack

The supplied stack can replace a blank `./app` Astro build context with this
repository's `showcase/` Dockerfile. Copy the `astro` service settings from
`compose.yml`, then add the `fslite-server` service, `fslite_data` volume, and
the shared `fslite_token` secret. Retain unrelated `next`, `postgres`,
`rustfs`, and `toolchain` services unchanged.

Keep the Caddy route pointed only at `astro:4321`, publish both Caddy ports 80
and 443, and configure `SHOWCASE_HOSTNAME` with the public DNS name. In
particular, do not add a host `ports:` mapping or a Caddy route for port 8080;
browsers must never see the fslite bearer credential or call the private server
directly.

For a public hostname, verify readiness without `--insecure` or `--resolve`:

```sh
curl --fail https://showcase.example.test/api/health/ready
```

## Operations

- Rotate a token by replacing the ignored token file, keeping mode `0600`,
  then recreate both trusted application services.
- Use `docker compose -f deploy/showcase/compose.yml logs` for diagnostics;
  do not paste secret-file contents into logs or support tickets.
- The showcase resets its shared workspace at startup and every 15 minutes.
  This is intentional: it is not a persistent visitor-content service.
