"""Offline tests: no model imports, network requests, GPU or user files."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("probe", Path(__file__).resolve().parents[2] / "scripts/local-ai/probe.py")
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class ProbeMetricTests(unittest.TestCase):
    def test_edit_distance(self):
        self.assertEqual(probe.distance("календарь", "календарь"), 0)
        self.assertEqual(probe.distance("abc", "adc"), 1)
        self.assertEqual(probe.distance(["one", "two"], ["one", "three", "two"]), 1)

    def test_normalization_is_explicit(self):
        result = probe.metrics("Привет, мир!", "привет мир")
        self.assertEqual(result["werPunctuationNormalized"], 0)
        self.assertGreater(result["cerCasefoldWhitespaceNormalized"], 0)
        self.assertEqual(probe.normalized("Ａ  Б\nВ"), "a б в")

    def test_empty_and_hallucinated_output(self):
        self.assertEqual(probe.metrics("", "")["werPunctuationNormalized"], 0)
        self.assertEqual(probe.metrics("", "invented words")["werPunctuationNormalized"], 2)
        self.assertEqual(probe.metrics("текст", "")["cerCasefoldWhitespaceNormalized"], 1)

    def test_models_are_pinned_and_distinct(self):
        self.assertEqual(len(probe.ASR_MODELS), 2)
        for name, revision in probe.ASR_MODELS.values():
            self.assertTrue(name.startswith("Qwen/"))
            self.assertEqual(len(revision), 40)
            int(revision, 16)


if __name__ == "__main__":
    unittest.main()
