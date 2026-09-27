FROM node:24.19.0-alpine AS build

WORKDIR /workspace
RUN corepack enable && corepack prepare pnpm@11.19.0 --activate
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/web apps/web
RUN pnpm install --frozen-lockfile --filter @knowledge/web...
RUN pnpm --filter @knowledge/web build

FROM nginxinc/nginx-unprivileged:1.28.0-alpine AS runtime
COPY apps/web/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /workspace/apps/web/dist /usr/share/nginx/html
USER 101:101
EXPOSE 8080
