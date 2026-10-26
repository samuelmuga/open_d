#!/usr/bin/env bash

# To change the current shell's directory, source this file:
#   source /path/to/open_d/src/open_d.sh <folder>
# To list indexed workspaces:
#   source /path/to/open_d/src/open_d.sh --list
# To rebuild the index:
#   source /path/to/open_d/src/open_d.sh --index
# To show help:
#   source /path/to/open_d/src/open_d.sh --help
# To open a workspace in a new terminal window:
#   source /path/to/open_d/src/open_d.sh -t <folder>
# Executing a child script cannot change its parent shell, so execution mode
# prints the selected path for use by the caller.
open_d_start_terminal() {
    local target_dir=$1

    if command -v gnome-terminal >/dev/null 2>&1; then
        gnome-terminal --working-directory="$target_dir" -- bash -i >/dev/null 2>&1 &
    elif command -v konsole >/dev/null 2>&1; then
        konsole --workdir "$target_dir" -e bash -i >/dev/null 2>&1 &
    elif command -v xfce4-terminal >/dev/null 2>&1; then
        xfce4-terminal --working-directory="$target_dir" --command='bash -i' >/dev/null 2>&1 &
    elif command -v kitty >/dev/null 2>&1; then
        kitty --directory "$target_dir" bash -i >/dev/null 2>&1 &
    elif command -v alacritty >/dev/null 2>&1; then
        alacritty --working-directory "$target_dir" -e bash -i >/dev/null 2>&1 &
    elif command -v wezterm >/dev/null 2>&1; then
        wezterm start --cwd "$target_dir" -- bash -i >/dev/null 2>&1 &
    elif command -v x-terminal-emulator >/dev/null 2>&1; then
        x-terminal-emulator -e bash -lc 'cd -- "$1" && exec bash -i' open_d "$target_dir" >/dev/null 2>&1 &
    else
        echo "open_d: no supported terminal emulator found" >&2
        return 127
    fi
}

open_d() {
    local new_terminal=false query
    if [[ $# -eq 2 && ( "$1" == "--new-terminal" || "$1" == "-t" ) ]]; then
        new_terminal=true
        query=$2
    elif [[ $# -eq 1 ]]; then
        query=$1
    else
        echo "usage: open_d <folder_name> | open_d --list | open_d [-t|--new-terminal] <folder_name>" >&2
        return 2
    fi

    local script_dir project_dir binary target_dir
    script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)" || return
    project_dir="$(cd -- "$script_dir/.." && pwd)" || return
    binary="$project_dir/target/release/open_d"
    if [[ ! -x "$binary" ]]; then
        binary="$project_dir/target/debug/open_d"
    fi
    if [[ ! -x "$binary" ]]; then
        echo "open_d: binary not found; build the project with cargo build --release" >&2
        return 127
    fi

    if [[ "$query" == "--list" && "$new_terminal" == false ]]; then
        "$binary" --list
        return $?
    fi
    if [[ "$query" == "--index" && "$new_terminal" == false ]]; then
        "$binary" --index
        return $?
    fi
    if [[ ( "$query" == "--help" || "$query" == "-h" || "$query" == ".help" ) && "$new_terminal" == false ]]; then
        "$binary" "$query"
        return $?
    fi

    if ! target_dir=$("$binary" "$query"); then
        return 1
    fi
    if [[ ! -d "$target_dir" ]]; then
        echo "open_d: selected path is not a directory: $target_dir" >&2
        return 1
    fi

    if [[ "$new_terminal" == true ]]; then
        open_d_start_terminal "$target_dir"
        return $?
    fi

    if [[ "${BASH_SOURCE[0]}" != "$0" ]]; then
        cd -- "$target_dir" || return
    else
        printf '%s\n' "$target_dir"
    fi
}

open_d "$@"
