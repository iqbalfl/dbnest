#!/usr/bin/env python3
"""Isi url dan sha256 satu artefak di manifest/manifest.json.

Dipakai oleh .github/workflows/build-engines.yml setelah tarball engine
diunggah ke Releases. Sengaja tidak pernah mengarang nilai: url dan sha256
wajib diberikan pemanggil, dan sha256 harus berbentuk 64 digit heksadesimal.

Soal flag `verified`: flag itu berlaku per versi, bukan per arsitektur, dan
`manifest::select_artifact` menolak versi yang `verified`-nya false. Artefak
arsitektur yang masih "TODO" karena belum pernah dibangun karena itu dihapus,
bukan dibiarkan — supaya pengguna arsitektur tersebut mendapat
`VersionUnavailable` yang jujur, bukan pesan "belum diverifikasi" yang juga
memblokir arsitektur yang sudah siap.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys

SHA256_RE = re.compile(r"\A[0-9a-f]{64}\Z")


def is_real(artifact: dict) -> bool:
    return bool(SHA256_RE.match(artifact.get("sha256", ""))) and artifact.get(
        "url", "TODO"
    ).startswith("https://")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", default="manifest/manifest.json")
    parser.add_argument("--engine", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--arch", required=True, choices=["x86_64", "aarch64"])
    parser.add_argument("--url", required=True)
    parser.add_argument("--sha256", required=True)
    args = parser.parse_args()

    sha = args.sha256.strip().lower()
    if not SHA256_RE.match(sha):
        print(f"sha256 tidak valid: {args.sha256!r}", file=sys.stderr)
        return 2
    if not args.url.startswith("https://"):
        print(f"url harus https: {args.url!r}", file=sys.stderr)
        return 2

    path = pathlib.Path(args.manifest)
    data = json.loads(path.read_text(encoding="utf-8"))

    engine = data.get("engines", {}).get(args.engine)
    if engine is None:
        print(f"engine {args.engine!r} tidak ada di {path}", file=sys.stderr)
        return 1

    entry = next(
        (v for v in engine.get("versions", []) if v.get("version") == args.version),
        None,
    )
    if entry is None:
        print(
            f"versi {args.engine} {args.version} tidak ada di {path}", file=sys.stderr
        )
        return 1

    artifacts = entry.get("artifacts")
    if not isinstance(artifacts, dict):
        print(
            f"{args.engine} {args.version} tidak punya blok artifacts "
            "(mungkin memakai variants_by_distro)",
            file=sys.stderr,
        )
        return 1

    artifact = artifacts.get(args.arch)
    if artifact is None:
        # Arsitektur ini pernah dipangkas (atau memang belum pernah ada).
        # Bentuk ulang entrinya dengan meniru saudaranya, supaya build arsitektur
        # kedua tetap bisa masuk tanpa menyunting manifest dengan tangan.
        sibling = next(iter(artifacts.values()), None)
        if sibling is None:
            print(
                f"{args.engine} {args.version} tidak punya artefak apa pun untuk ditiru",
                file=sys.stderr,
            )
            return 1
        artifact = {k: v for k, v in sibling.items() if k not in ("url", "sha256")}
        artifacts[args.arch] = artifact
        print(f"membuat entri artefak {args.arch} baru", file=sys.stderr)

    artifact["url"] = args.url
    artifact["sha256"] = sha

    for arch in [a for a, art in artifacts.items() if not is_real(art)]:
        print(f"menghapus artefak {arch} yang masih TODO", file=sys.stderr)
        del artifacts[arch]

    entry["verified"] = bool(artifacts) and all(is_real(a) for a in artifacts.values())

    path.write_text(
        json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    print(f"{args.engine} {args.version} {args.arch} -> {args.url}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
