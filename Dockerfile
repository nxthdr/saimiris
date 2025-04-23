FROM rust:latest AS builder

RUN apt-get update \
    && apt-get install -y libpcap-dev libsasl2-dev libssl-dev capnproto \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY . .

RUN cargo install --path .

FROM debian:stable-slim

RUN apt-get update \
    && apt-get install -y libpcap-dev libsasl2-dev libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/cargo/bin/saimiris /app/saimiris

ENTRYPOINT [ "/app/saimiris" ]
CMD [ "--help" ]
