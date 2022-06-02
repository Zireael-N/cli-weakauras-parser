FROM alpine:3.16.0
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN apk add --update --virtual deps g++ cmake make zlib \
    && apk add --virtual rustup-deps curl \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --profile=minimal -y \
    && apk del rustup-deps
