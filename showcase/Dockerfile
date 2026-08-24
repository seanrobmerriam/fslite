FROM node:22-alpine AS dependencies

WORKDIR /app

ENV PNPM_HOME=/pnpm
ENV PATH="$PNPM_HOME:$PATH"

RUN corepack enable && corepack prepare pnpm@10.12.4 --activate

COPY showcase/package.json showcase/pnpm-lock.yaml ./
RUN pnpm fetch --frozen-lockfile

FROM dependencies AS build

COPY showcase/package.json showcase/pnpm-lock.yaml ./
RUN pnpm install --offline --frozen-lockfile

COPY showcase ./
RUN pnpm build && pnpm prune --prod

FROM node:22-alpine AS runtime

WORKDIR /app

RUN addgroup --gid 10001 fslite \
    && adduser --disabled-password --gecos "" --uid 10001 --ingroup fslite fslite

ENV NODE_ENV=production \
    HOST=0.0.0.0 \
    PORT=4321

COPY --from=build --chown=10001:10001 /app/node_modules ./node_modules
COPY --from=build --chown=10001:10001 /app/dist ./dist
COPY showcase/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh

RUN chmod 0755 /usr/local/bin/docker-entrypoint.sh

EXPOSE 4321
USER 10001:10001

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["node", "./dist/server/entry.mjs"]

HEALTHCHECK --interval=5s --timeout=3s --start-period=10s --retries=12 \
  CMD wget -qO- http://127.0.0.1:4321/api/health/live >/dev/null || exit 1
