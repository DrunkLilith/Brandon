FROM golang:1.24.8-alpine3.22 AS build

ARG MINIO_RELEASE=RELEASE.2025-10-15T17-29-55Z
ARG MINIO_COMMIT=9e49d5e7a648f00e26f2246f4dc28e6b07f8c84a
ARG MINIO_SOURCE_SHA256=be6d0bd3696c3a13a35f02d3a0280b64319c67918b4501c5c3d87f96d000085c
RUN apk add --no-cache ca-certificates curl tar
WORKDIR /source
RUN curl --fail --show-error --location --proto '=https' \
      "https://github.com/minio/minio/archive/refs/tags/${MINIO_RELEASE}.tar.gz" \
      --output source.tar.gz \
    && echo "${MINIO_SOURCE_SHA256}  source.tar.gz" | sha256sum -c - \
    && tar -xzf source.tar.gz --strip-components=1 \
    && test "$(grep '^module ' go.mod)" = 'module github.com/minio/minio' \
    && CGO_ENABLED=0 go build -tags kqueue -trimpath \
      -ldflags "-s -w -X github.com/minio/minio/cmd.Version=2025-10-15T17:29:55Z -X github.com/minio/minio/cmd.CopyrightYear=2025 -X github.com/minio/minio/cmd.ReleaseTag=${MINIO_RELEASE} -X github.com/minio/minio/cmd.CommitID=${MINIO_COMMIT} -X github.com/minio/minio/cmd.ShortCommitID=9e49d5e7a648" \
      -o /out/minio .

FROM alpine:3.22.1
RUN apk add --no-cache ca-certificates curl \
    && addgroup -S -g 10001 minio \
    && adduser -S -D -H -u 10001 -G minio minio \
    && mkdir /data \
    && chown 10001:10001 /data
COPY --from=build /out/minio /usr/local/bin/minio
USER 10001:10001
EXPOSE 9000
ENTRYPOINT ["/usr/local/bin/minio"]
CMD ["server", "/data", "--address", ":9000"]
