FROM rust:latest AS builder

RUN apt-get update \
    && apt-get install -y libpcap-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY . .

RUN cargo install --path .

FROM debian:stable-slim

RUN apt-get update \
    && apt-get install -y libpcap-dev openssl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/cargo/bin/osiris /app/osiris

ENTRYPOINT [ "/app/osiris" ]
CMD [ "--help" ]