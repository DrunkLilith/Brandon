"""Report unique-clip corpus WER, not the mean of repeated inference runs."""
import json
from pathlib import Path
import statistics
from probe import distance, normalized


def summarize(report):
    unique = [item for item in report["results"] if item["repetition"] == 1]
    errors = words = 0
    for item in unique:
        reference = normalized(item["reference"], True).split()
        predicted = normalized(item["text"], True).split()
        errors += distance(reference, predicted)
        words += len(reference)
    warmed = [item for item in report["results"] if item["repetition"] > 0]
    return {
        "model": report["model"], "revision": report["revision"], "scope": report["scope"],
        "uniqueClips": len(unique), "audioSeconds": sum(item["audioSeconds"] for item in unique),
        "wordEdits": errors, "referenceWords": words, "corpusWer": errors / max(1, words),
        "medianWarmRealTimeFactor": statistics.median(item["realTimeFactor"] for item in warmed),
        "peakAllocatedBytes": max(item["peakAllocatedBytes"] for item in report["results"]),
    }


if __name__ == "__main__":
    output = Path("target/qa/local-ai")
    reports = [summarize(json.loads((output / f"asr-{name}-fleurs-ru.json").read_text(encoding="utf-8"))) for name in ["0.6b", "1.7b"]]
    (output / "asr-comparison.json").write_text(json.dumps(reports, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(reports, ensure_ascii=False, indent=2))
