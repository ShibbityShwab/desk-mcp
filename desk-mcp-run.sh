#!/usr/bin/env bash
export PATH="${PATH}"
export YDOTOOL_SOCKET="${YDOTOOL_SOCKET:-/run/user/1000/ydotoold.sock}"
export ALLOW_SHELL="${ALLOW_SHELL:-1}"
export ALLOW_CODE="${ALLOW_CODE:-1}"
export DESKMCP_WORKSPACE="${DESKMCP_WORKSPACE:-$HOME}"
exec /home/shibbityshwab/Documents/GitHub/desk-mcp/target/release/desk-mcp "$@"
