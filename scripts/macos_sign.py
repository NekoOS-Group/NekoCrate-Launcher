#!/usr/bin/env python3
"""Stable local code signing; private key material lives only in the user keychain."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile

NAME = "NekoCrate Local Development"
IDENTIFIER = "dev.nekocrate.launcher"
STATE = Path.home() / "Library/Application Support/NekoCrate/signing"


def run(*args, capture=True):
    result = subprocess.run(args, text=True, capture_output=capture)
    if result.returncode:
        detail = (result.stderr or result.stdout or "").strip() if capture else ""
        raise RuntimeError(f"{Path(args[0]).name} 执行失败：{detail}")
    return result.stdout.strip() if capture else ""


def identities():
    output = run("/usr/bin/security", "find-identity", "-v", "-p", "codesigning")
    return {fingerprint for fingerprint, name in re.findall(r'\b([0-9A-F]{40}) "([^"]+)"', output)
            if name == NAME}


def certificate_hash(pem):
    with tempfile.NamedTemporaryFile(suffix=".pem") as f:
        f.write(pem.encode())
        f.flush()
        der = subprocess.run(["/usr/bin/openssl", "x509", "-in", f.name, "-outform", "DER"],
                             check=True, capture_output=True).stdout
    return hashlib.sha1(der).hexdigest().upper()


def ensure_identity():
    if sys.platform != "darwin":
        raise RuntimeError("本地代码签名仅用于 macOS")
    state_file = STATE / "identity.json"
    if state_file.exists():
        info = json.loads(state_file.read_text())
        fingerprint = info.get("sha1", "")
        if not re.fullmatch(r"[0-9A-F]{40}", fingerprint) or info.get("name") != NAME:
            raise RuntimeError(f"签名配置无效：{state_file}")
        if fingerprint not in identities():
            raise RuntimeError("固定签名证书或私钥不可用。请解锁登录钥匙串并检查证书；不会自动更换身份。")
        return fingerprint

    # Reuse an interrupted setup. Never silently replace an existing certificate.
    found = subprocess.run(["/usr/bin/security", "find-certificate", "-a", "-c", NAME, "-p"],
                           capture_output=True, text=True)
    certificates = re.findall(r"-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----",
                              found.stdout, re.S)
    if len(certificates) > 1:
        raise RuntimeError(f"发现多个 {NAME} 证书，请在钥匙串中确认应保留的身份。")
    keychain = run("/usr/bin/security", "default-keychain", "-d", "user").strip('"')
    with tempfile.TemporaryDirectory(prefix="nekocrate-sign-") as directory:
        temp = Path(directory)
        cert = temp / "certificate.pem"
        if certificates:
            cert.write_text(certificates[0] + "\n")
        else:
            config = temp / "openssl.cnf"
            config.write_text(f"""[req]
prompt = no
distinguished_name = subject
x509_extensions = signing
[subject]
CN = {NAME}
[signing]
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
subjectKeyIdentifier = hash
""")
            key = temp / "private-key.pem"
            run("/usr/bin/openssl", "req", "-new", "-x509", "-newkey", "rsa:3072",
                "-nodes", "-sha256", "-days", "3650", "-config", str(config),
                "-keyout", str(key), "-out", str(cert))
            key.chmod(0o600)
            bundle = temp / "identity.pem"
            bundle.write_bytes(key.read_bytes() + cert.read_bytes())
            bundle.chmod(0o600)
            # Unencrypted transport exists only in a 0700 temporary directory.
            # PEM avoids macOS/LibreSSL PKCS#12 password-format differences.
            # Import non-extractably and permit codesign, never all applications.
            run("/usr/bin/security", "import", str(bundle), "-k", keychain,
                "-f", "pemseq", "-t", "agg", "-x", "-T", "/usr/bin/codesign")
            key.unlink()
            bundle.unlink()
        fingerprint = certificate_hash(cert.read_text())
        if fingerprint not in identities():
            print("请在 macOS 系统提示中确认本地代码签名证书；只添加当前用户的代码签名信任。", file=sys.stderr)
            # Current-user code-signing policy only: no admin/SSL trust changes.
            run("/usr/bin/security", "add-trusted-cert", "-r", "trustRoot", "-p", "codeSign",
                "-k", keychain, str(cert), capture=False)
        if fingerprint not in identities():
            raise RuntimeError("本地代码签名身份不可用，请检查证书对应的私钥和代码签名信任。")
        STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
        (STATE / "certificate.pem").write_text(cert.read_text())
        state_file.write_text(json.dumps({"name": NAME, "sha1": fingerprint,
                                          "identifier": IDENTIFIER}, indent=2) + "\n")
        state_file.chmod(0o600)
    print(f"已准备固定代码签名身份：{NAME}", file=sys.stderr)
    return fingerprint


def requirement(fingerprint):
    if not re.fullmatch(r"[0-9A-F]{40}", fingerprint):
        raise ValueError("invalid certificate fingerprint")
    return f'identifier "{IDENTIFIER}" and certificate leaf = H"{fingerprint}"'


def sign(path):
    fingerprint = ensure_identity()
    path = Path(path).resolve(strict=True)
    if not path.is_file() and path.suffix != ".app":
        raise RuntimeError("请选择可执行文件或 .app")
    policy = requirement(fingerprint)
    verification = ["/usr/bin/codesign", "--verify", "--strict", "-R", "=" + policy, str(path)]
    valid = subprocess.run(verification,
                           capture_output=True).returncode == 0
    if valid:
        return
    run("/usr/bin/codesign", "--force", "--sign", fingerprint,
        "--identifier", IDENTIFIER, "--timestamp=none",
        "--preserve-metadata=entitlements", "--requirements", f"=designated => {policy}",
        str(path), capture=False)
    run(*verification)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("ensure")
    signer = commands.add_parser("sign")
    signer.add_argument("path")
    args = parser.parse_args()
    if args.command == "ensure":
        ensure_identity()
    else:
        sign(args.path)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"本地签名失败：{error}", file=sys.stderr)
        sys.exit(1)
