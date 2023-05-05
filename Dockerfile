FROM rust:1.67.1 as build
WORKDIR /usr/src/ckb-analyzer

COPY . .
RUN cargo build --release

FROM ubuntu:20.04

COPY --from=build /usr/src/ckb-analyzer/target/release/ckb-analyzer /bin/ckb-analyzer

RUN apt-get update && apt install -y openssl ca-certificates && update-ca-certificates && rm -rf /var/lib/apt/lists/*

ENV POSTGRES_HOST       127.0.0.1
ENV POSTGRES_PORT       5432
ENV POSTGRES_DB         ckb
ENV POSTGRES_USER       postgres
ENV POSTGRES_PASSWORD   postgres
