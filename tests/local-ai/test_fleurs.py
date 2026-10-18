"""Public fixture downloader tests use only an in-memory synthetic tar."""
import importlib.util
import io
from pathlib import Path
import tarfile
import time
import unittest

spec = importlib.util.spec_from_file_location("fetch_fleurs", Path(__file__).resolve().parents[2] / "scripts/local-ai/fetch_fleurs.py")
fetch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fetch)


class FleursFixtureTests(unittest.TestCase):
    def test_bounded_reads(self):
        reader = fetch.BoundedReader(io.BytesIO(b"12345"), 4, time.monotonic() + 10)
        with self.assertRaises(ValueError):
            reader.read()
        with self.assertRaises(TimeoutError):
            fetch.BoundedReader(io.BytesIO(b"a"), 4, 0).read()

    def test_never_extracts_archive_paths(self):
        buffer = io.BytesIO()
        wav = b"RIFF" + bytes(4) + b"WAVE" + bytes(40)
        with tarfile.open(fileobj=buffer, mode="w") as archive:
            info = tarfile.TarInfo("../../fixture.wav")
            info.size = len(wav)
            archive.addfile(info, io.BytesIO(wav))
        buffer.seek(0)
        with tarfile.open(fileobj=buffer, mode="r|") as archive:
            selected = fetch.select_samples(archive, {"fixture.wav": {"reference": "synthetic"}}, 1)
        self.assertEqual(selected[0][0], "fixture.wav")
        self.assertEqual(selected[0][1], wav)

    def test_rejects_oversize_wav_before_read(self):
        info = tarfile.TarInfo("huge.wav")
        info.size = fetch.MAX_AUDIO_BYTES + 1
        with self.assertRaises(ValueError):
            fetch.select_samples([info], {"huge.wav": {}}, 1)


if __name__ == "__main__":
    unittest.main()
