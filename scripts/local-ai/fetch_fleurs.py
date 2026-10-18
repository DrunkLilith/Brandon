"""Fetch five public CC-BY-4.0 Russian speech samples, not a whole dataset.

Fixed upstream revision, bounded streaming reads, no archive path extraction.
This convenience sample is NOT a held-out or representative quality benchmark.
"""

import argparse
import csv
from datetime import datetime, timezone
import gzip
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import tarfile
import time
import urllib.request


REVISION = "70bb2e84b976b7e960aa89f1c648e09c59f894dd"
BASE = f"https://huggingface.co/datasets/google/fleurs/resolve/{REVISION}/data/ru_ru"
MAX_AUDIO_BYTES = 4 * 1024 * 1024


class BoundedReader:
    def __init__(self, source, limit, deadline):
        self.source = source
        self.limit = limit
        self.deadline = deadline
        self.total = 0

    def read(self, size=-1):
        if time.monotonic() > self.deadline:
            raise TimeoutError("Public fixture stream exceeded its time budget")
        size = min(size if size >= 0 else self.limit + 1, self.limit - self.total + 1)
        data = self.source.read(size)
        self.total += len(data)
        if self.total > self.limit:
            raise ValueError("Public fixture stream exceeded its byte budget")
        return data


def select_samples(archive, rows, count):
    result = []
    seen = set()
    for member in archive:
        name = PurePosixPath(member.name).name
        if not member.isfile() or name not in rows or name in seen:
            continue
        if not 44 <= member.size <= MAX_AUDIO_BYTES:
            raise ValueError("Unexpected WAV member size")
        with archive.extractfile(member) as stream:
            data = stream.read(MAX_AUDIO_BYTES + 1)
        if len(data) != member.size or not (data[:4] == b"RIFF" and data[8:12] == b"WAVE"):
            raise ValueError("Invalid public WAV fixture")
        result.append((name, data, rows[name]))
        seen.add(name)
        if len(result) == count:
            break
    if len(result) != count:
        raise ValueError("Not enough matching public fixtures within the stream budget")
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=Path("target/qa/local-ai"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    manifest_path = args.output / "fleurs-ru.json"
    if manifest_path.exists():
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        if manifest.get("revision") != REVISION:
            raise ValueError("Existing fixture revision differs; no files overwritten")
        for sample in manifest["samples"]:
            path = args.output / sample["file"]
            if path.parent.resolve() != args.output.resolve() or hashlib.sha256(path.read_bytes()).hexdigest() != sample["sha256"]:
                raise ValueError("Existing public fixture failed verification")
        print(json.dumps({"cached": True, "samples": len(manifest["samples"])}))
        return
    deadline = time.monotonic() + 180
    with urllib.request.urlopen(f"{BASE}/test.tsv", timeout=30) as response:
        raw = BoundedReader(response, 5 * 1024 * 1024, deadline).read()
    rows = {}
    for row in csv.reader(io.StringIO(raw.decode("utf-8")), delimiter="\t"):
        if len(row) >= 7 and row[1].endswith(".wav") and int(row[5]) <= 30 * 16000:
            rows[row[1]] = {"id": row[0], "reference": row[2], "referenceNormalizedUpstream": row[3], "gender": row[6]}
    with urllib.request.urlopen(f"{BASE}/audio/test.tar.gz", timeout=30) as response:
        compressed = BoundedReader(response, 32 * 1024 * 1024, deadline)
        with gzip.GzipFile(fileobj=compressed) as decompressor:
            bounded = BoundedReader(decompressor, 64 * 1024 * 1024, deadline)
            with tarfile.open(fileobj=bounded, mode="r|") as archive:
                samples = select_samples(archive, rows, 5)
        downloaded_bytes = compressed.total
    manifest = {
        "dataset": "google/fleurs", "revision": REVISION, "language": "ru", "split": "test",
        "license": "CC-BY-4.0", "source": "https://huggingface.co/datasets/google/fleurs",
        "attribution": "FLEURS, Google et al.; see the linked dataset card and paper for contributors.",
        "changes": "Five WAV recordings copied unchanged; no full archive retained.",
        "selection": "First five matching ordinary WAV entries in archive order with <=30 second upstream duration; convenience sample, not representative.",
        "downloadedUtc": datetime.now(timezone.utc).isoformat(), "archiveCompressedBytesRead": downloaded_bytes,
        "metadataSha256": hashlib.sha256(raw).hexdigest(), "samples": [],
    }
    for index, (name, data, metadata) in enumerate(samples, 1):
        path = args.output / f"fleurs-ru-{index:02d}.wav"
        # Exclusive creation; never replace an existing fixture or user file.
        with path.open("xb") as stream:
            stream.write(data)
        manifest["samples"].append(metadata | {"file": path.name, "originalFile": name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
    with manifest_path.open("x", encoding="utf-8") as stream:
        json.dump(manifest, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    print(json.dumps({"samples": len(samples), "compressedBytesRead": downloaded_bytes, "manifest": str(manifest_path)}))


if __name__ == "__main__":
    main()
