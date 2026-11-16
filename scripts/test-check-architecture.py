"""Regression coverage for renamed and platform GPUI dependency boundaries."""
import importlib.util
from pathlib import Path
import sys
import unittest

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location(
    "architecture", Path(__file__).with_name("check-architecture.py")
)
architecture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(architecture)


class DependencyBoundaryTests(unittest.TestCase):
    def check_dependency(self, package, dependency):
        metadata = {"packages": [
            {"name": name, "publish": [], "dependencies": (
                [dependency] if name == package else []
            )}
            for name in architecture.allowed
        ]}
        return architecture.check_packages(metadata)

    def test_runtime_platform_build_and_renamed_dependencies_stay_in_ui(self):
        for name, alias in [
            ("gpui", None), ("gpui-pre", "gpui"),
            ("gpui-pre-platform", "gpui_platform"),
            ("gpui-pre-macos", "backend"), ("gpui_platform", "backend"),
            ("another-package", "gpui"),
        ]:
            for kind in [None, "build"]:
                dependency = {"name": name, "rename": alias, "kind": kind,
                              "target": 'cfg(target_os = "windows")'}
                with self.subTest(name=name, alias=alias, kind=kind):
                    self.assertEqual(self.check_dependency("ryne-ui", dependency), [])
                    self.assertIn("GPUI outside UI: ryne-session",
                                  self.check_dependency("ryne-session", dependency))

    def test_dev_dependencies_and_unrelated_names_are_not_rejected(self):
        for dependency in [
            {"name": "gpui-pre", "rename": "gpui", "kind": "dev"},
            {"name": "image", "rename": None, "kind": None},
        ]:
            self.assertEqual(self.check_dependency("ryne-session", dependency), [])


if __name__ == "__main__":
    unittest.main()
