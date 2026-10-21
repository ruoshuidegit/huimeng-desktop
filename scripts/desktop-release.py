#!/usr/bin/env python3
"""校验桌面版本、整理平台资产并生成正式更新清单。"""

import argparse
import base64
import binascii
import datetime
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tomllib


ROOT = Path(__file__).resolve().parents[1]
BASELINE_VERSION = "0.2.99"


def version_number(version):
    if not re.fullmatch(r"(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)", version):
        raise ValueError("发布版本必须为三段数字，例如 0.3.0")
    return tuple(map(int, version.split(".")))


def check_version(args):
    version = args.expected.removeprefix("v")
    version_number(version)
    package = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))
    package_lock = json.loads((ROOT / "package-lock.json").read_text(encoding="utf-8"))
    cargo = tomllib.loads((ROOT / "src-tauri/Cargo.toml").read_text(encoding="utf-8"))
    cargo_lock = tomllib.loads((ROOT / "src-tauri/Cargo.lock").read_text(encoding="utf-8"))
    tauri = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
    versions = {
        "package.json": package["version"],
        "package-lock.json": package_lock["version"],
        "package-lock.json 根包": package_lock["packages"][""]["version"],
        "Cargo.toml": cargo["package"]["version"],
        "Cargo.lock": next(
            item["version"] for item in cargo_lock["package"]
            if item["name"] == cargo["package"]["name"]
        ),
        "tauri.conf.json": tauri["version"],
    }
    for name, actual in versions.items():
        if actual != version:
            raise ValueError(f"{name} 版本 {actual} 与请求的 {version} 不一致")
    if version_number(BASELINE_VERSION) >= version_number(version):
        raise ValueError("正式版本必须高于 Windows 更新验收基线 0.2.99")
    tag = f"v{version}"
    if args.releases_json:
        releases = json.loads(args.releases_json.read_text(encoding="utf-8"))
        if any(item["tagName"] == tag and not item["isDraft"] for item in releases):
            raise ValueError(f"{tag} 已经公开发布，不能覆盖既有版本")
    commit = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True,
    ).strip()
    if args.github_output:
        with args.github_output.open("a", encoding="utf-8") as output:
            output.write(f"version={version}\ntag={tag}\ncommit={commit}\n")
    print(f"版本校验通过：{tag}，提交 {commit}")


def copy_one(pattern, destination):
    matches = list(ROOT.glob(pattern))
    if len(matches) != 1 or not matches[0].is_file() or matches[0].stat().st_size == 0:
        raise ValueError(f"发布资产必须唯一且非空：{pattern}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(matches[0], destination)


def stage(args):
    version_number(args.version)
    upload = ROOT / "upload"
    prefix = f"huimeng-desktop_{args.version}"
    if args.platform == "windows":
        if args.arch != "x64" or args.target:
            raise ValueError("Windows 发行资产使用本机 x64 目标")
        bundle = "src-tauri/target/release/bundle"
        for extension in ["exe", "exe.sig"]:
            copy_one(f"{bundle}/nsis/*_{args.version}_*-setup.{extension}", upload / f"{prefix}_x64-setup.{extension}")
    else:
        targets = {"aarch64": "aarch64-apple-darwin", "x86_64": "x86_64-apple-darwin"}
        if targets.get(args.arch) != args.target:
            raise ValueError("macOS 架构与 Rust 构建目标不一致")
        bundle = f"src-tauri/target/{args.target}/release/bundle"
        for extension in ["app.tar.gz", "app.tar.gz.sig"]:
            copy_one(f"{bundle}/macos/*.{extension}", upload / f"{prefix}_{args.arch}.{extension}")
        dmg_arch = "x64" if args.arch == "x86_64" else args.arch
        copy_one(f"{bundle}/dmg/*_{args.version}_*.dmg", upload / f"{prefix}_{dmg_arch}.dmg")
    print(f"已整理正式资产：{args.platform} {args.arch}")


def stage_baseline(args):
    if version_number(BASELINE_VERSION) >= version_number(args.candidate_version):
        raise ValueError("更新候选版本必须高于验收基线")
    copy_one(
        f"src-tauri/target/debug/bundle/nsis/*_{BASELINE_VERSION}_*-setup.exe",
        ROOT / "baseline" / f"huimeng-desktop_{BASELINE_VERSION}_windows-update-baseline.exe",
    )
    print(f"已整理独立 Windows 更新验收基线 {BASELINE_VERSION}")


def read_signature(path):
    signature = path.read_text(encoding="utf-8").strip()
    try:
        decoded = base64.b64decode(signature, validate=True)
    except (ValueError, binascii.Error) as error:
        raise ValueError(f"更新签名格式无效：{path.name}") from error
    if not decoded.startswith(b"untrusted comment:") or len(decoded.splitlines()) < 4:
        raise ValueError(f"更新签名不完整：{path.name}")
    return signature


def manifest(args):
    version_number(args.version)
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repository):
        raise ValueError("GitHub 仓库标识无效")
    directory = args.directory
    prefix = f"huimeng-desktop_{args.version}"
    payloads = {
        "darwin-aarch64": f"{prefix}_aarch64.app.tar.gz",
        "darwin-x86_64": f"{prefix}_x86_64.app.tar.gz",
        "windows-x86_64": f"{prefix}_x64-setup.exe",
    }
    required = {
        f"{prefix}_aarch64.dmg", f"{prefix}_x64.dmg",
        *payloads.values(), *(f"{name}.sig" for name in payloads.values()),
    }
    actual = {path.name for path in directory.iterdir() if path.is_file()}
    unexpected = actual - required - {"latest.json", "SHASUMS256.txt"}
    if unexpected:
        raise ValueError(f"正式资产包含意外文件：{', '.join(sorted(unexpected))}")
    for name in required:
        path = directory / name
        if not path.is_file() or path.stat().st_size == 0:
            raise ValueError(f"正式发布资产缺失或为空：{name}")
    base = f"https://github.com/{args.repository}/releases/download/v{args.version}"
    platforms = {
        platform: {
            "signature": read_signature(directory / f"{name}.sig"),
            "url": f"{base}/{name}",
        }
        for platform, name in payloads.items()
    }
    result = {
        "version": args.version,
        "notes": "桌面端功能与体验更新。",
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z"),
        "platforms": platforms,
    }
    (directory / "latest.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    checksums = []
    for name in sorted(required | {"latest.json"}):
        with (directory / name).open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        checksums.append(f"{digest}  {name}")
    (directory / "SHASUMS256.txt").write_text("\n".join(checksums) + "\n", encoding="utf-8")
    print(f"更新清单和 SHA-256 校验文件已生成：{len(platforms)} 个更新平台，{len(required)} 个发布资产")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    check = commands.add_parser("check-version")
    check.add_argument("--expected", required=True)
    check.add_argument("--releases-json", type=Path)
    check.add_argument("--github-output", type=Path)
    check.set_defaults(run=check_version)
    collect = commands.add_parser("stage")
    collect.add_argument("--version", required=True)
    collect.add_argument("--platform", choices=["macos", "windows"], required=True)
    collect.add_argument("--arch", required=True)
    collect.add_argument("--target", default="")
    collect.set_defaults(run=stage)
    baseline = commands.add_parser("stage-baseline")
    baseline.add_argument("--candidate-version", required=True)
    baseline.set_defaults(run=stage_baseline)
    listing = commands.add_parser("manifest")
    listing.add_argument("--version", required=True)
    listing.add_argument("--repository", required=True)
    listing.add_argument("--directory", type=Path, required=True)
    listing.set_defaults(run=manifest)
    args = parser.parse_args()
    try:
        args.run(args)
    except (ValueError, OSError, StopIteration, KeyError) as error:
        parser.exit(1, f"发布校验失败：{error}\n")


if __name__ == "__main__":
    main()
