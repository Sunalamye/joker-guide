#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Iterable

LOG_PATH = Path("python-env/experiments/checkpoints.jsonl")


def read_entries() -> list[dict[str, object]]:
    if not LOG_PATH.exists():
        return []

    entries: list[dict[str, object]] = []
    for line in LOG_PATH.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue
        entries.append(obj)
    return entries


def print_entries(entries: Iterable[dict[str, object]]) -> None:
    for entry in entries:
        print(
            f"{entry.get('timestamp','?')} | steps={entry.get('steps','?')} | "
            f"total={entry.get('total_timesteps','?')} | interval={entry.get('save_interval','?')} | "
            f"{entry.get('checkpoint','?')}"
        )


def main() -> None:
    parser = argparse.ArgumentParser(description="Show SB3 checkpoint experiment log")
    parser.add_argument("--tail", type=int, default=5, help="Show the last N entries")
    args = parser.parse_args()

    entries = read_entries()
    if not entries:
        print(f"{LOG_PATH} is empty or missing.")
        return

    print_entries(entries[-args.tail :])


if __name__ == "__main__":
    main()
