import importlib.util
from pathlib import Path
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    "upgrade", Path(__file__).with_name("check-gentoo-upgrade.py")
)
upgrade = importlib.util.module_from_spec(spec)
spec.loader.exec_module(upgrade)


class UpgradeTests(unittest.TestCase):
    def test_publication_order(self):
        cases = [
            ([], "0.1.0_alpha1", True),
            (["0.1.0_alpha2"], "0.1.0_alpha2", True),
            (["0.1.0_alpha2"], "0.1.0_alpha10", True),
            (["0.1.0_alpha10"], "0.1.0_alpha2", False),
            (["0.1.0_rc1"], "0.1.0", True),
            (["0.1.0"], "0.1.0_rc1", False),
            (["0.1.0-r1"], "0.1.0", False),
            (["0.1.0_alpha1", "0.2.0"], "0.1.0", False),
            (["invalid"], "0.1.0", False),
            ([], "invalid", False),
        ]
        for current, target, allowed in cases:
            with self.subTest(current=current, target=target):
                with tempfile.TemporaryDirectory() as temporary:
                    directory = Path(temporary)
                    for version in current:
                        (directory / f"oddity-bin-{version}.ebuild").write_text("EAPI=8\n")
                    before = {p.name: p.read_bytes() for p in directory.iterdir()}
                    if allowed:
                        upgrade.check_upgrade(directory, target)
                    else:
                        with self.assertRaises(ValueError):
                            upgrade.check_upgrade(directory, target)
                    self.assertEqual(before, {p.name: p.read_bytes() for p in directory.iterdir()})


if __name__ == "__main__":
    unittest.main()
