### ---------- Stage 1: Build the Rust gateway binary ----------
FROM rust:1-alpine AS rust-builder

RUN apk add --no-cache musl-dev linux-headers

WORKDIR /build
COPY src/rust/ .

RUN cargo build --release && \
    cp target/release/can-tcp-gateway /can-tcp-gateway


### ---------- Stage 2: Final add-on image ----------
ARG BUILD_FROM
FROM $BUILD_FROM

# Install Python dependencies (for the Python engine)
RUN apk add --no-cache python3 py3-pip iproute2
RUN pip3 install --break-system-packages python-can

# Copy the Rust binary from builder stage
COPY --from=rust-builder /can-tcp-gateway /can-tcp-gateway
RUN chmod a+x /can-tcp-gateway

# Copy Python gateway and entrypoint
COPY run.sh /
COPY src/python/gateway.py /

RUN chmod a+x /run.sh

WORKDIR /usr/src/app

CMD [ "/run.sh" ]
