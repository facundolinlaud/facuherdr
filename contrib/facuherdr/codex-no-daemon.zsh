# Herdr: inside Herdr panes, run interactive Codex without its shared background
# daemon. Codex runs hooks in that daemon, where they cannot tell which pane they
# belong to; with --no-daemon they run in the pane, so Herdr gets Codex's status
# and account label. Outside Herdr panes, codex is unchanged.
codex() {
  if [[ -z "${HERDR_PANE_ID:-}" ]]; then
    command codex "$@"
    return
  fi
  case "${1:-}" in
    resume|fork) command codex "$1" --no-daemon "${@:2}" ;;
    agents|exec|review|login|logout|mcp|plugin|app-server|remote-control|app|completion|update|doctor|sandbox|debug|apply|queue|archive|delete|migrate-rollouts|unarchive|cloud|exec-server|features|help)
      command codex "$@" ;;
    *) command codex --no-daemon "$@" ;;
  esac
}
