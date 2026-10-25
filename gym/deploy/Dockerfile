# The gym: Node + Playwright's Chromium, running gym/server against Excel for the web.
# Build context is the repo root: docker build -f gym/deploy/Dockerfile .
FROM mcr.microsoft.com/playwright:v1.63.0-noble

ENV NODE_ENV=production \
    GYM_ADDIN_DIR=/app/gym/bridge-addin \
    GYM_STATE_DIR=/var/lib/gym \
    GYM_PORT=8787

RUN corepack enable
WORKDIR /app
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json ./
COPY gym/server/package.json gym/server/
COPY gym/bridge-addin/package.json gym/bridge-addin/
RUN pnpm install --frozen-lockfile --prod --filter ./gym/server

COPY gym/server gym/server
COPY gym/bridge-addin gym/bridge-addin

EXPOSE 8787
HEALTHCHECK --interval=30s --timeout=5s CMD node -e "fetch('http://127.0.0.1:8787/healthz').then(r=>process.exit(r.ok?0:1),()=>process.exit(1))"
# deploy.sh runs the container as the host user that owns the state directory.
CMD ["node", "gym/server/src/main.ts"]
