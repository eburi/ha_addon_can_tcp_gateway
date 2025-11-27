#!/usr/bin/with-contenv bashio
set -e

bashio::log.info "Starting CAN to TCP Gateway Add-on..."

# Read config values using bashio
CAN_IFACE=$(bashio::config 'can_interface')
LISTEN_HOST=$(bashio::config 'listen_host')
LISTEN_PORT=$(bashio::config 'listen_port')
LOG_LEVEL=$(bashio::config 'log_level')

bashio::log.info "Using CAN interface: ${CAN_IFACE}"
bashio::log.info "TCP listen: ${LISTEN_HOST}:${LISTEN_PORT}"

export CAN_INTERFACE="${CAN_IFACE}"
export LISTEN_HOST="${LISTEN_HOST}"
export LISTEN_PORT="${LISTEN_PORT}"
export LOG_LEVEL="${LOG_LEVEL}"


# Init can0
ip link set can0 down 2>/dev/null
ip link set can0 type can bitrate 250000 fd off restart-ms 100
ip link set can0 up
ifconfig can0 txqueuelen 10000

# Start gateway
exec python3 /gateway.py
