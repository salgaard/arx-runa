#!/usr/bin/env sh
set -eu

command="${1:-}"
case "$command" in
  config)
    subcommand="${2:-}"
    if [ "$subcommand" = "create" ]; then
      echo "If your browser doesn't open automatically, go to the following link: https://example.com/oauth"
      exit 0
    fi
    if [ "$subcommand" = "dump" ]; then
      echo "{\"remote\":{\"type\":\"drive\"}}"
      exit 0
    fi
    echo "unknown config subcommand" 1>&2
    exit 2
    ;;
  ok)
    shift
    echo "${1:-ok}"
    exit 0
    ;;
  status)
    shift
    code="${1:-1}"
    shift || true
    echo "${1:-error}" 1>&2
    exit "$code"
    ;;
  sleep)
    sleep 10
    exit 0
    ;;
  *)
    echo "unknown command" 1>&2
    exit 2
    ;;
esac
