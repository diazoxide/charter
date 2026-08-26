"""The package must be importable and expose a version + a CLI entry point.

Guards the one property everything else rests on: `charter` is an installable
package, not a directory that happens to sit next to its data."""
import subprocess
import sys
import unittest


class TestPackaging(unittest.TestCase):
    def test_version_is_exposed(self):
        import charter
        self.assertRegex(charter.__version__, r"^\d+\.\d+\.\d+")

    def test_module_entry_point_runs(self):
        """A real child, pointed at a throwaway plane. It inherits this process's cwd —
        the checkout — which is how it used to resolve the developer's own plane (#527).
        """
        from tests._isolation import child_plane_env
        _plane, env = child_plane_env(self)
        p = subprocess.run([sys.executable, "-m", "charter", "--version"],
                           env=env, capture_output=True, text=True)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("charter", p.stdout.lower())

    def test_runtime_has_zero_dependencies(self):
        """Stdlib-only is the product's cleanest promise — assert it mechanically."""
        import tomllib
        from pathlib import Path
        cfg = tomllib.loads((Path(__file__).resolve().parents[1] / "pyproject.toml").read_text())
        self.assertEqual(cfg["project"].get("dependencies", []), [])

    def test_doctor_python_floor_matches_pyproject(self):
        """`charter doctor` is the one command whose entire job is truthful environment
        reporting — its Python-version gate must match `requires-python`, not drift from it."""
        import re
        import tomllib
        from pathlib import Path
        from charter import doctor
        cfg = tomllib.loads((Path(__file__).resolve().parents[1] / "pyproject.toml").read_text())
        m = re.match(r">=\s*(\d+)\.(\d+)", cfg["project"]["requires-python"])
        self.assertIsNotNone(m, "unexpected requires-python format")
        pinned = (int(m.group(1)), int(m.group(2)))
        self.assertEqual(doctor.MIN_PYTHON, pinned,
                         "doctor.MIN_PYTHON has drifted from pyproject.toml's requires-python")


if __name__ == "__main__":
    unittest.main()
