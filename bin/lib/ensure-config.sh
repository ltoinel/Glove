# shellcheck shell=bash
# Sourced by the bin/ scripts. config.yaml is local (git-ignored, may hold
# secrets); a fresh checkout only has config.yaml.sample, copied on first use.
ensure_config() {
    local root="$1"
    [ -f "$root/config.yaml" ] && return
    cp "$root/config.yaml.sample" "$root/config.yaml"
    echo -e "\033[0;33m[glove]\033[0m config.yaml created from config.yaml.sample — review it (api_key, cors_origins)."
}
