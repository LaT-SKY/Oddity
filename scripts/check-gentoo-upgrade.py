#!/usr/bin/env python3
"""在替换 overlay 文件前拒绝版本回退, 同版本可重复发布."""

import argparse
from pathlib import Path

from portage.versions import vercmp, ververify


def check_upgrade(directory: Path, version: str) -> None:
    if not ververify(version):
        raise ValueError(f"无效的 Gentoo 版本: {version}")
    for ebuild in directory.glob("oddity-bin-*.ebuild"):
        current = ebuild.name.removeprefix("oddity-bin-").removesuffix(".ebuild")
        if not ververify(current):
            raise ValueError(f"无法识别现有版本: {ebuild.name}")
        if vercmp(version, current) < 0:
            raise ValueError(f"拒绝将 Gentoo 包从 {current} 降级到 {version}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("version")
    args = parser.parse_args()
    try:
        check_upgrade(args.directory, args.version)
    except ValueError as error:
        parser.exit(1, f"{error}\n")
