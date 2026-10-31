"""Package the already-built native application without compiling it again."""
import json
import stat
import zipfile
from pathlib import Path

MARKER = "nekocrate.portable.json"
README = """猫咪盒子启动器 · Portable

完整解压后运行。旁边的 .minecraft 会自动关联；设置、Java 和任务保存在 .nekocrate。
关闭启动器后，请移动整个 NekoCrate 文件夹。安装版与便携版的设置独立。
登录凭据保存在当前电脑的系统凭据库；换电脑后需要重新登录。

Windows：运行 nekocrate-launcher.exe。需要 Microsoft WebView2 Runtime。
macOS：运行 NekoCrate Portable.app。如 macOS 临时隔离应用，请用 Finder 单独移动
该 .app 到目标位置，并将 nekocrate.portable.json 和游戏目录一并放在其旁边。
Linux：运行 NekoCrate.AppImage；必要时先赋予可执行权限。

Extract the complete archive before running. The adjacent .minecraft folder is
linked automatically. Settings, managed Java and tasks live in .nekocrate.
Move the entire folder to keep your games and settings together.
Account credentials stay in the operating system credential store. Sign in again
on another computer. Windows requires Microsoft WebView2 Runtime.
On macOS, if the app is translocated, move the .app itself in Finder and keep the
portable marker and game folder beside it. On Linux, make the AppImage executable.
"""


def archive_path(release, tag, platform):
    return release / "bundle/portable" / f"NekoCrate-Launcher-{tag}-{platform}-portable.zip"


def package(source, destination, executable_name):
    """Preserve executable bits and app-bundle symlinks for native unzip tools."""
    destination.parent.mkdir(parents=True, exist_ok=True)
    root = Path("NekoCrate")
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for name, data in [(MARKER, json.dumps({"schema": 1}) + "\n"), ("README.txt", README)]:
            info = zipfile.ZipInfo((root / name).as_posix())
            info.create_system = 3
            info.external_attr = (stat.S_IFREG | 0o644) << 16
            archive.writestr(info, data)
        directory = zipfile.ZipInfo("NekoCrate/.minecraft/")
        directory.create_system = 3
        directory.external_attr = (stat.S_IFDIR | 0o755) << 16 | 0x10
        archive.writestr(directory, b"")
        paths = [source, *sorted(source.rglob("*"))] if source.is_dir() else [source]
        for path in paths:
            relative = root / executable_name / path.relative_to(source) if source.is_dir() else root / executable_name
            if path.is_symlink():
                if not path.resolve().is_relative_to(source.resolve()):
                    raise ValueError(f"Bundle symlink leaves application: {path}")
                info = zipfile.ZipInfo(relative.as_posix())
                info.create_system = 3
                info.external_attr = (stat.S_IFLNK | 0o777) << 16
                archive.writestr(info, str(path.readlink()))
            else:
                archive.write(path, relative.as_posix())
    return destination
