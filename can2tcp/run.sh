#!/usr/bin/with-contenv bashio
set -e

bashio::log.info "Starting CAN to TCP Gateway Add-on..."

# Read config values using bashio
CAN_IFACE=$(bashio::config 'can_interface')
LISTEN_PORT=$(bashio::config 'listen_port')
LOG_LEVEL=$(bashio::config 'log_level')
GATEWAY_ENGINE=$(bashio::config 'gateway_engine')

bashio::log.info "Using CAN interface: ${CAN_IFACE}"
bashio::log.info "TCP listen: 0.0.0.0:${LISTEN_PORT}"
bashio::log.info "Gateway engine: ${GATEWAY_ENGINE}"

export CAN_INTERFACE="${CAN_IFACE}"
export LISTEN_HOST="0.0.0.0"
export LISTEN_PORT="${LISTEN_PORT}"
export LOG_LEVEL="${LOG_LEVEL}"

if ! ip link show dev "${CAN_IFACE}" >/dev/null 2>&1; then
    bashio::log.fatal "CAN interface '${CAN_IFACE}' not found. Configure host CAN first."
    exit 1
fi

# Start gateway with selected engine
if [ "${GATEWAY_ENGINE}" = "rust" ]; then
    bashio::log.info "Starting Rust gateway engine"
    exec /can-tcp-gateway
else
    bashio::log.info "Starting Python gateway engine"
    exec python3 /gateway.py
fi
