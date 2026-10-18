"""Opt-in local model smoke probes. Synthetic data only, never a user vault.

This small corpus verifies plumbing, not representative OCR/ASR quality.
Model downloads are explicit; --offline forbids network cache misses.
"""

import argparse
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import platform
import subprocess
import time
import unicodedata
import urllib.request
from datetime import datetime, timezone


ASR_MODELS = {
    "0.6b": ("Qwen/Qwen3-ASR-0.6B-hf", "7f1569a48a89f3e3f4dc3a5c9d28bddd903bc76c"),
    "1.7b": ("Qwen/Qwen3-ASR-1.7B-hf", "bcd2b5b7f32b480ab5790554cfa8347f246a14f3"),
}
OCR_LINES = [
    "Задача: закончить локальный поиск.",
    "Документы, фотографии и календарь.",
    "Brandon local-first workspace 3090",
    "Срок: 24 октября. Файл: notes.md",
]
SPEECH = {
    "ru": "Привет. Это проверка распознавания русской речи. Завтра нужно закончить задачу и открыть календарь.",
    "en": "Hello. This is a local speech recognition test. Tomorrow we need to finish the task and open the calendar.",
}


def normalized(text, punctuation=False):
    text = unicodedata.normalize("NFKC", text).casefold()
    if punctuation:
        text = "".join(" " if unicodedata.category(c).startswith("P") else c for c in text)
    return " ".join(text.split())


def distance(left, right):
    row = list(range(len(right) + 1))
    for i, a in enumerate(left, 1):
        nxt = [i]
        for j, b in enumerate(right, 1):
            nxt.append(min(nxt[-1] + 1, row[j] + 1, row[j - 1] + (a != b)))
        row = nxt
    return row[-1]


def metrics(reference, actual):
    ref, got = normalized(reference), normalized(actual)
    words, predicted = normalized(reference, True).split(), normalized(actual, True).split()
    return {
        "cerCasefoldWhitespaceNormalized": distance(ref, got) / max(1, len(ref)),
        "werPunctuationNormalized": distance(words, predicted) / max(1, len(words)),
    }


def metadata():
    versions = {}
    for name in ["rapidocr", "onnxruntime", "Pillow", "torch", "transformers", "accelerate", "librosa", "soundfile"]:
        try:
            versions[name] = importlib.metadata.version(name)
        except importlib.metadata.PackageNotFoundError:
            pass
    return {"dateUtc": datetime.now(timezone.utc).isoformat(), "python": platform.python_version(), "packages": versions,
            "scope": "synthetic plumbing smoke, not a representative quality benchmark"}


def fixtures(output):
    from PIL import Image, ImageDraw, ImageFont
    output.mkdir(parents=True, exist_ok=True)
    for name, background, foreground in [("light", "#ffffff", "#161616"), ("dark", "#1e1e1e", "#d6d6d6")]:
        image = Image.new("RGB", (1100, 340), background)
        draw = ImageDraw.Draw(image)
        font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 32)
        for index, line in enumerate(OCR_LINES):
            draw.text((35, 30 + index * 70), line, fill=foreground, font=font)
        image.save(output / f"ocr-{name}.png")
    for language, text in SPEECH.items():
        subprocess.run(["espeak-ng", "-v", language, "-s", "150", "-w", str(output / f"speech-{language}.wav"), "--stdin"], input=text, text=True, check=True)


def run_ocr(args, output):
    import onnxruntime as ort
    ort.disable_telemetry_events()
    from rapidocr import EngineType, LangRec, ModelType, OCRVersion, RapidOCR
    manifest = json.loads(Path(__file__).with_name("ocr-models.lock.json").read_text(encoding="utf-8"))
    model_paths = {}
    for item in manifest["models"]:
        path = Path("/cache/rapidocr") / item["file"]
        if not path.exists():
            if args.offline:
                raise RuntimeError(f"Offline cache miss: {item['file']}")
            path.parent.mkdir(parents=True, exist_ok=True)
            temporary = path.with_suffix(".download")
            try:
                with urllib.request.urlopen(item["url"], timeout=60) as response, temporary.open("wb") as target:
                    total = 0
                    while chunk := response.read(1024 * 1024):
                        total += len(chunk)
                        if total > item["bytes"]:
                            raise RuntimeError("Model exceeds its pinned download size")
                        target.write(chunk)
                if hashlib.sha256(temporary.read_bytes()).hexdigest() != item["sha256"]:
                    raise RuntimeError("Downloaded model hash mismatch")
                temporary.replace(path)
            finally:
                temporary.unlink(missing_ok=True)
        if path.stat().st_size != item["bytes"] or hashlib.sha256(path.read_bytes()).hexdigest() != item["sha256"]:
            raise RuntimeError(f"Cached model checksum mismatch: {item['file']}")
        model_paths[f"{item['slot']}.model_path"] = str(path)
    started = time.perf_counter()
    engine = RapidOCR(params={
        "Global.model_root_dir": "/cache/rapidocr",
        "Rec.lang_type": LangRec.CYRILLIC,
        "Rec.ocr_version": OCRVersion.PPOCRV5,
        "Rec.model_type": ModelType.MOBILE,
        "Rec.engine_type": EngineType.ONNXRUNTIME,
        **model_paths,
    })
    report = metadata() | {"engineLoadSeconds": time.perf_counter() - started, "recognizer": "PP-OCRv5 cyrillic", "results": []}
    for theme in ["light", "dark"]:
        image = output / f"ocr-{theme}.png"
        start = time.perf_counter()
        tesseract = subprocess.run(["tesseract", str(image), "stdout", "-l", "rus+eng", "--psm", "6"], capture_output=True, text=True, check=True)
        report["results"].append({"engine": "tesseract-rus+eng", "theme": theme, "seconds": time.perf_counter() - start,
                                  "text": tesseract.stdout.strip(), **metrics("\n".join(OCR_LINES), tesseract.stdout)})
        for repetition in range(3):
            start = time.perf_counter()
            result = engine(str(image))
            text = "\n".join(result.txts if result.txts is not None else [])
            report["results"].append({"engine": "rapidocr-cyrillic", "theme": theme, "repetition": repetition,
                                      "seconds": time.perf_counter() - start, "text": text,
                                      "boxes": result.boxes.tolist() if result.boxes is not None else [], **metrics("\n".join(OCR_LINES), text)})
    report["modelFiles"] = [{"path": str(path.relative_to('/cache')), "bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                            for path in Path("/cache/rapidocr").rglob("*") if path.is_file()]
    return report


def run_asr(args, output):
    import soundfile as sf
    import torch
    from transformers import AutoModelForMultimodalLM, AutoProcessor
    torch.set_num_threads(4)
    if not torch.cuda.is_available():
        raise RuntimeError("CUDA unavailable; this probe is explicitly the RTX 3090 run, not a hidden CPU fallback")
    torch.cuda.set_per_process_memory_fraction(0.45)
    model_id, revision = ASR_MODELS[args.model]
    started = time.perf_counter()
    processor = AutoProcessor.from_pretrained(model_id, revision=revision, trust_remote_code=False, local_files_only=args.offline)
    model = AutoModelForMultimodalLM.from_pretrained(model_id, revision=revision, trust_remote_code=False,
                                                   local_files_only=args.offline, dtype=torch.bfloat16,
                                                   device_map={"": 0}, attn_implementation="sdpa").eval()
    torch.cuda.synchronize()
    report = metadata() | {"model": model_id, "revision": revision, "license": "Apache-2.0", "gpu": torch.cuda.get_device_name(),
                           "loadSecondsIncludingCacheMissDownloads": time.perf_counter() - started, "results": []}
    if args.corpus == "fleurs-ru":
        corpus = json.loads((output / "fleurs-ru.json").read_text(encoding="utf-8"))
        report["scope"] = "Five public natural Russian speech recordings; convenience sample, not representative or held-out"
        report["corpus"] = corpus
        samples = [("ru", item["reference"], output / item["file"], item["sha256"]) for item in corpus["samples"]]
    else:
        samples = [(language, reference, output / f"speech-{language}.wav", None) for language, reference in SPEECH.items()]
    for language, reference, path, checksum in samples:
        if path.parent.resolve() != output.resolve() or path.is_symlink() or path.stat().st_size > 4 * 1024 * 1024:
            raise RuntimeError("Fixture path or size violates probe bounds")
        if checksum and hashlib.sha256(path.read_bytes()).hexdigest() != checksum:
            raise RuntimeError("Public fixture checksum mismatch")
        audio, rate = sf.read(path)
        duration = len(audio) / rate
        if not 0 < duration <= 30:
            raise RuntimeError("Fixture exceeds duration limit")
        for repetition in range(3):
            torch.cuda.reset_peak_memory_stats()
            start = time.perf_counter()
            inputs = processor.apply_transcription_request(audio=str(path), language=language).to(model.device, model.dtype)
            with torch.inference_mode():
                output_ids = model.generate(**inputs, max_new_tokens=192, do_sample=False)
            torch.cuda.synchronize()
            elapsed = time.perf_counter() - start
            text = processor.decode(output_ids[:, inputs["input_ids"].shape[1]:], return_format="transcription_only")[0]
            report["results"].append({"language": language, "file": path.name, "reference": reference, "repetition": repetition, "audioSeconds": duration, "seconds": elapsed,
                                      "realTimeFactor": elapsed / duration, "peakAllocatedBytes": torch.cuda.max_memory_allocated(),
                                      "text": text, **metrics(reference, text)})
    return report


def main():
    os.environ.setdefault("NUMBA_CACHE_DIR", "/cache/numba")
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["fixtures", "ocr", "asr"])
    parser.add_argument("--model", choices=ASR_MODELS, default="0.6b")
    parser.add_argument("--corpus", choices=["synthetic", "fleurs-ru"], default="synthetic")
    parser.add_argument("--output", type=Path, default=Path("/results"))
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    if args.offline:
        os.environ["HF_HUB_OFFLINE"] = "1"
        os.environ["TRANSFORMERS_OFFLINE"] = "1"
    if args.action == "fixtures":
        fixtures(args.output)
        report = metadata() | {"fixtureText": OCR_LINES, "speechText": SPEECH}
    else:
        report = run_ocr(args, args.output) if args.action == "ocr" else run_asr(args, args.output)
    corpus_suffix = "-fleurs-ru" if args.corpus == "fleurs-ru" else ""
    path = args.output / (f"{args.action}-{args.model}{corpus_suffix}.json" if args.action == "asr" else f"{args.action}.json")
    path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"report": str(path), "results": len(report.get("results", []))}))


if __name__ == "__main__":
    main()
