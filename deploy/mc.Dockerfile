FROM golang:1.24.8-alpine3.22 AS build

ARG MC_RELEASE=RELEASE.2025-08-13T08-35-41Z
ARG MC_COMMIT=7394ce0dd2a80935aded936b09fa12cbb3cb8096
ARG MC_SOURCE_SHA256=29db22500374169a43951c7cef09daf19e7291ea5ba00ac10f321371b0a35b32
RUN apk add --no-cache ca-certificates curl tar
WORKDIR /source
COPY deploy/mc-credentials.go /tmp/mc-credentials.go
RUN curl --fail --show-error --location --proto '=https' \
      "https://github.com/minio/mc/archive/refs/tags/${MC_RELEASE}.tar.gz" \
      --output source.tar.gz \
    && echo "${MC_SOURCE_SHA256}  source.tar.gz" | sha256sum -c - \
    && tar -xzf source.tar.gz --strip-components=1 \
    && test "$(grep '^module ' go.mod)" = 'module github.com/minio/mc' \
    && CGO_ENABLED=0 go build -tags kqueue -trimpath \
      -ldflags "-s -w -X github.com/minio/mc/cmd.Version=2025-08-13T08:35:41Z -X github.com/minio/mc/cmd.CopyrightYear=2025 -X github.com/minio/mc/cmd.ReleaseTag=${MC_RELEASE} -X github.com/minio/mc/cmd.CommitID=${MC_COMMIT} -X github.com/minio/mc/cmd.ShortCommitID=7394ce0dd2a8" \
      -o /out/mc . \
    && CGO_ENABLED=0 go build -trimpath -ldflags "-s -w" -o /out/mc-credentials /tmp/mc-credentials.go

FROM alpine:3.22.1
RUN apk add --no-cache ca-certificates \
    && addgroup -S -g 10002 mc \
    && adduser -S -D -H -u 10002 -G mc mc
COPY --from=build /out/mc /usr/local/bin/mc
COPY --from=build /out/mc-credentials /usr/local/bin/mc-credentials
USER 10002:10002
ENTRYPOINT ["/usr/local/bin/mc"]
