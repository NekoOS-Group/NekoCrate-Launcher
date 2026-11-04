"""Package standalone executables and the common macOS update archive."""
import plistlib
import shutil
import subprocess
import tarfile
import tempfile
from pathlib import Path

RUNTIME_CONFIG = "nekocrate.runtime.json"
SUFFIXES = {
    "mac-appsilicon": ".app.tar.gz",
    "win-amd64": ".exe",
    "linux-amd64": ".elf",
}


def archive_path(release, tag, platform):
    return release / "bundle/standalone" / f"NekoCrate-Launcher-{tag}-{platform}{SUFFIXES[platform]}"


def package(source, destination, app=None):
    destination.parent.mkdir(parents=True, exist_ok=True)
    if app is None:
        # Byte-identical to the executable included in the installer.
        shutil.copy2(source, destination)
        destination.chmod(0o755)
        return destination
    with tempfile.TemporaryDirectory(prefix="nekocrate-portable-app-") as temporary:
        bundle = Path(temporary) / app.name
        for path in app.rglob("*"):
            if path.is_symlink() and not path.resolve().is_relative_to(app.resolve()):
                raise ValueError(f"Bundle symlink leaves application: {path}")
        shutil.copytree(app, bundle, symlinks=True)
        info = plistlib.loads((bundle / "Contents/Info.plist").read_bytes())
        executable = info["CFBundleExecutable"]
        if Path(executable).name != executable or executable in (".", ".."):
            raise ValueError("Invalid bundle executable")
        shutil.copy2(source, bundle / "Contents/MacOS" / executable)
        # Keep the compiled code, but omit the installed app's mode file.
        for directory in ("Contents/Resources", "Contents/MacOS"):
            (bundle / directory / RUNTIME_CONFIG).unlink(missing_ok=True)
        subprocess.run(["codesign", "--force", "--deep", "--sign", "-", str(bundle)], check=True)
        subprocess.run(["codesign", "--verify", "--deep", "--strict", str(bundle)], check=True)
        # This single archive serves both manual extraction and signed updates.
        with tarfile.open(destination, "w:gz", compresslevel=6) as archive:
            archive.add(bundle, arcname=bundle.name)
    return destination
