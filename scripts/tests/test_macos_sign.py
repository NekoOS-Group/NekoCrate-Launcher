import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import macos_sign as signing


class MacSigningTests(unittest.TestCase):
    fingerprint = "A1" * 20

    def test_existing_identity_is_reused_without_provisioning(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            (state / "identity.json").write_text(json.dumps({
                "name": signing.NAME, "sha1": self.fingerprint,
            }))
            with patch.object(signing, "STATE", state), \
                    patch.object(signing.sys, "platform", "darwin"), \
                    patch.object(signing, "identities", return_value={self.fingerprint}), \
                    patch.object(signing.subprocess, "run") as run:
                self.assertEqual(signing.ensure_identity(), self.fingerprint)
                run.assert_not_called()

    def test_missing_private_identity_never_silently_rotates(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            (state / "identity.json").write_text(json.dumps({
                "name": signing.NAME, "sha1": self.fingerprint,
            }))
            with patch.object(signing, "STATE", state), \
                    patch.object(signing.sys, "platform", "darwin"), \
                    patch.object(signing, "identities", return_value=set()), \
                    patch.object(signing.subprocess, "run") as run:
                with self.assertRaisesRegex(RuntimeError, "不会自动更换身份"):
                    signing.ensure_identity()
                run.assert_not_called()

    def test_requirement_rejects_injected_or_malformed_fingerprint(self):
        for value in ["", "A" * 39, 'A" or true', "g" * 40]:
            with self.assertRaises(ValueError):
                signing.requirement(value)

    def test_valid_signature_is_not_rewritten(self):
        with tempfile.NamedTemporaryFile() as executable, \
                patch.object(signing, "ensure_identity", return_value=self.fingerprint), \
                patch.object(signing.subprocess, "run", return_value=
                             subprocess.CompletedProcess([], 0)) as verify, \
                patch.object(signing, "run") as mutate:
            signing.sign(executable.name)
            self.assertEqual(verify.call_args.args[0][-2], "=" + signing.requirement(self.fingerprint))
            mutate.assert_not_called()

    def test_signing_failure_is_not_silently_accepted(self):
        with tempfile.NamedTemporaryFile() as executable, \
                patch.object(signing, "ensure_identity", return_value=self.fingerprint), \
                patch.object(signing.subprocess, "run", return_value=
                             subprocess.CompletedProcess([], 1)), \
                patch.object(signing, "run", side_effect=RuntimeError("signing failed")) as mutate:
            with self.assertRaisesRegex(RuntimeError, "signing failed"):
                signing.sign(executable.name)
            self.assertEqual(mutate.call_count, 1)
            args = mutate.call_args.args
            self.assertIn("--sign", args)
            self.assertIn("=designated => " + signing.requirement(self.fingerprint), args)


if __name__ == "__main__":
    unittest.main()
