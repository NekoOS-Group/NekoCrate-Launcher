export const articles = [
  {
    id: "start",
    title: "First launch",
    group: "Getting started",
    summary: "From download to launching your first game version.",
    sections: [
      {
        title: "1. Download and open",
        paragraphs: [
          "On the download page, choose your operating system and package type. Windows and Linux have x64 builds, and macOS has an Apple Silicon build. There is no prebuilt package for Intel Macs yet.",
        ],
        link: { label: "Choose a download", href: "" },
      },
      {
        title: "2. Add a game directory",
        paragraphs: [
          "Click the plus button in the instance bar to link an existing .minecraft or create a new game directory. The official launcher's directory appears automatically in the list.",
          "You don't need to move existing game files. Drag a folder that contains .minecraft into the window, and the launcher scans it and lets you pick which directory to link.",
        ],
      },
      {
        title: "3. Choose an account",
        paragraphs: [
          "Click the account button at the top left, add a Microsoft account, and complete authorization in your browser. After signing in, return to the launcher and select the account.",
          "You can also play local content as a guest. Servers and online services that require a legitimate copy still need a valid game account and entitlement.",
        ],
      },
      {
        title: "4. Add a version and launch",
        paragraphs: [
          "In the version menu next to the launch button, choose \"Add new version\", then pick a Minecraft version and the loader you need. You can also switch to modpack browsing.",
          "When the download finishes, click \"Launch game\". The launcher prepares a compatible Java, dependencies and resources. While the game runs, you can view the log or stop the game.",
        ],
      },
    ],
  },
  {
    id: "install",
    title: "System requirements and installation",
    group: "Getting started",
    summary: "Requirements and installation methods for the three platforms.",
    sections: [
      {
        title: "Windows x64",
        paragraphs: [
          "For the installer, run setup.exe and follow the wizard. For the portable version, run the EXE directly. The launcher interface requires the Microsoft Edge WebView2 runtime.",
          "Current release packages are not code-signed with a Windows publisher certificate. Make sure your download comes from the Releases page of NekoOS-Group/NekoCrate-Launcher.",
        ],
      },
      {
        title: "macOS Apple Silicon",
        paragraphs: [
          "Apple Silicon Macs are supported, and the app requires macOS 11 or later. For the installer, open the DMG and drag the app into Applications. For the portable version, extract the .app.tar.gz and double-click the app.",
          "The current dev build is not notarized by Apple. If the system blocks it from opening, confirm the source, then check the app's open options under System Settings → Privacy & Security. Do not turn off system security checks globally.",
        ],
      },
      {
        title: "Linux x64",
        paragraphs: [
          "On Debian / Ubuntu, you can install the DEB package. Other compatible distributions can run the ELF, but the system must still provide libraries such as GTK 3, WebKitGTK 4.1 and D-Bus.",
          "After downloading the ELF, make it executable. You can do this in your file manager's properties, or run the following commands in a terminal, replacing the file name with the name you downloaded.",
        ],
        code: "chmod +x NekoCrate-Launcher-VERSION-linux-amd64.elf\n./NekoCrate-Launcher-VERSION-linux-amd64.elf",
      },
      {
        title: "Java for the game",
        paragraphs: [
          "The launcher itself doesn't depend on Java, but Minecraft: Java Edition does. NekoCrate detects local runtimes, checks compatibility, and downloads a suitable version if none is found. Games and modpacks also have their own memory, graphics and system requirements.",
        ],
      },
    ],
  },
  {
    id: "portable",
    title: "Installed and portable versions",
    group: "Getting started",
    summary: "Choose where launcher data lives and learn how to migrate it.",
    sections: [
      {
        title: "Installed version: lives in the system",
        paragraphs: [
          "The installed version keeps launcher settings, indexes and tasks in the system app data directory. Game directories can be anywhere convenient and are linked through the launcher. It suits long-term use on a single computer.",
        ],
      },
      {
        title: "Portable version: travels with the folder",
        paragraphs: [
          "The portable program looks for .minecraft in its own directory and creates it if missing. Launcher data is saved in the adjacent .nekocrate. On macOS, the game directory and data sit outside the .app, next to it.",
        ],
        code: "My Games/\n├── NekoCrate Launcher.app  (or .exe / .elf)\n├── .minecraft/       games, versions and saves\n└── .nekocrate/       settings, Java, indexes and tasks",
      },
      {
        title: "Migration and coexistence",
        paragraphs: [
          "With the launcher and game closed, moving the whole portable directory keeps your games and settings. If you move only the executable, it uses fresh data in the new location.",
          "The installed and portable versions keep separate data. Two launcher processes can't use the same data directory at once. Account credentials stay in the system credential store, so you need to sign in again on a new computer.",
        ],
      },
      {
        title: "Updates",
        paragraphs: [
          "The macOS .app, the Windows installed and portable versions, and the Linux ELF support in-app updates. Update packages are signature-verified, installed, and the launcher restarts. The Linux DEB is updated by downloading a new package.",
        ],
      },
    ],
  },
  {
    id: "directories",
    title: "Game directories and versions",
    group: "Managing games",
    summary: "An instance is one .minecraft, and one instance can hold multiple versions.",
    sections: [
      {
        title: "Link an existing directory",
        paragraphs: [
          "After you select or drag in an existing game directory, NekoCrate recognizes, reads and manages the game files in place. It doesn't copy the whole directory into the launcher data directory. You can also drag in a parent directory that contains several .minecraft folders to scan it.",
        ],
      },
      {
        title: "Version isolation",
        paragraphs: [
          "Version management handles the launchable versions inside one game directory. You can choose the isolation mode in instance settings. Newly installed loader versions and modpacks use separate working directories, so mods, configs and saves don't get mixed.",
          "Existing modpacks keep their original working directory where possible. Don't manually move loader configs or dependencies to tidy up names. You can rename a version's display name directly in the launcher, and it is independent of the directory name on disk.",
        ],
      },
      {
        title: "Ordering and management",
        paragraphs: [
          "Drag instance or version entries to reorder them. In version management, you can browse metadata, mods, data packs, resource packs, runtime dependencies and saves. Instance settings provide Java, memory, game window and launch options.",
        ],
      },
      {
        title: "What happens when you remove something",
        paragraphs: [
          "Removing a linked game directory only unlinks it, and the original files stay. Removing a launcher-managed instance or a single version moves it to the system Trash / Recycle Bin and checks the dependencies of other versions. Shared files aren't deleted along with a single version.",
        ],
      },
    ],
  },
  {
    id: "resources",
    title: "Modpacks and resources",
    group: "Managing games",
    summary: "Online browsing, local import, and reusing resources across versions.",
    sections: [
      {
        title: "Install a modpack",
        paragraphs: [
          "In \"Add new version\", switch to modpacks and browse Modrinth or CurseForge. Open a modpack's details, choose a version, check the target game directory, then download. Local .mrpack and CurseForge-format modpacks can also be imported.",
          "Modpacks are installed from their manifest. Some authors don't allow third parties to distribute their files directly. These projects appear as manual import steps in the task window. Download the file from the project page, then drag it into the prompt area to continue.",
        ],
      },
      {
        title: "Drag in and copy resources",
        paragraphs: [
          "Drag mods, resource packs, shaders or maps onto the matching area of the target version. ZIP files are identified by their actual contents, so the drop location doesn't change the resource type. Data packs can be placed into a specific save.",
          "Drag an existing resource onto another version to copy it, and the source file stays. The global resource library also supports favorites, external import and online downloads, and lets you assign resources to different versions.",
        ],
      },
      {
        title: "Enable, disable and configure",
        paragraphs: [
          "Click the status icon in the mod list to toggle a mod on or off. Expand an entry to see its metadata, dependencies and linked online description. Mods with config files can be opened in the config editor.",
          "JSON, TOML and CFG files get matching editing aids, and saving validates and backs up the file. You are asked to confirm again before deleting a mod declared in a modpack manifest.",
        ],
      },
      {
        title: "Loader compatibility",
        paragraphs: [
          "Fabric, Forge and NeoForge can be installed. A mod must match the game version and loader. Both being JAR files doesn't mean they can be mixed. Quilt installation and other game version types are not currently supported.",
        ],
      },
    ],
  },
  {
    id: "java",
    title: "Java and memory",
    group: "Managing games",
    summary: "Reuses compatible Java first, with per-instance launch settings.",
    sections: [
      {
        title: "Automatic Java selection",
        paragraphs: [
          "The launcher searches for local Java and picks a compatible version based on constraints from the game, loader and mods. An existing compatible runtime is reused first. If none is found, one is downloaded through Adoptium / GitHub.",
          "A newer Java isn't always more compatible. Older Forge, certain native libraries and some mods can limit the usable range. On Apple Silicon, older games may also need an Intel Java and system support for running Intel programs.",
        ],
      },
      {
        title: "Specify a local runtime",
        paragraphs: [
          "In the Java list in instance settings, choose Auto, a detected runtime, or pick an executable with the file picker. The Java page in global resource management shows the distribution, architecture and managed identifier.",
          "Launcher-managed Java can be deleted. Java installed on the system is managed by the system or the original installer.",
        ],
      },
      {
        title: "Allocate memory",
        paragraphs: [
          "Options are \"Minimum\", \"Recommended\", \"Auto\" and \"Custom\". Auto mode estimates from the size of the game and modpack and the available memory. Custom values are in MB.",
          "Leave enough memory for the system and other programs. If the game crashes, use the log to tell whether it was low memory, an incompatible Java or something else.",
        ],
      },
    ],
  },
  {
    id: "downloads",
    title: "Downloads, repair and logs",
    group: "Troubleshooting",
    summary: "Understand task progress, keep completed work, and find the cause of failures.",
    sections: [
      {
        title: "Task stages",
        paragraphs: [
          "Download means transferring over the network, reuse means using files already on disk, verify checks file integrity, and install covers steps such as extraction and running loader installers. The task window shows the current stage and file progress.",
          "Downloads support pause, resume and retrying failed files. Completed work is kept on resume, but if the server doesn't support Range, the affected file may need to be downloaded again. Download staging is cleaned up after a successful install.",
        ],
      },
      {
        title: "Network settings",
        paragraphs: [
          "You can choose separate download sources for the game itself and for mods, with automatic selection and system / HTTP / SOCKS proxies supported. Concurrency can be set up to 256, or you can use adaptive mode. Higher concurrency isn't necessarily faster.",
          "If several HTTPS downloads fail at the same time, first check the system date, network and proxy settings, then look at the specific error in the task.",
        ],
      },
      {
        title: "Verify and repair",
        paragraphs: [
          "If an installation is incomplete or files are corrupted, run verify and repair. A manual repair checks the required files, prefers reusable local resources, and downloads again when needed.",
          "Repair doesn't treat every game crash as file corruption. Missing prerequisites, version conflicts and configuration errors need to be handled based on the diagnostics.",
        ],
      },
      {
        title: "Report launch problems",
        paragraphs: [
          "After an abnormal exit, open the log and look at the highlighted errors and diagnostics. When reporting, include the launcher version, operating system, game version, loader and the log around the error. Do not include account credentials or access tokens.",
        ],
        external: {
          label: "Report an issue on GitHub",
          href: "https://github.com/NekoOS-Group/NekoCrate-Launcher/issues",
        },
      },
    ],
  },
  {
    id: "development",
    title: "Development and contributing",
    group: "Developers",
    summary: "Running locally, repository layout and open-source license.",
    sections: [
      {
        title: "Local development",
        paragraphs: [
          "You need Rust 1.93 or newer stable, Node.js 22, and the system dependencies Tauri requires for your platform. Run the following commands from the repository root. Local signing on macOS also requires Python 3.",
        ],
        code: "git switch neko_dev\nnpm --prefix app ci\ncargo ui",
      },
      {
        title: "Core and interface",
        paragraphs: [
          "The Rust core is in crates/nekocrate-core, the Vue frontend is in app/src, and Tauri IPC and packaging are in app/src-tauri. The desktop app and the CLI share the core logic. The website lives separately in site and reuses the launcher's background rendering and theme directly.",
        ],
        external: {
          label: "Read the architecture notes",
          href: "https://github.com/NekoOS-Group/NekoCrate-Launcher/blob/neko_dev/docs/architecture.md",
        },
      },
      {
        title: "Contributing",
        paragraphs: [
          "Reproducible bug reports, improvements and documentation fixes are welcome. The code is licensed under GPL-3.0-only. Third-party dependencies and Minecraft assets keep their own licenses and rights notices.",
        ],
        external: {
          label: "Browse the source and contribute",
          href: "https://github.com/NekoOS-Group/NekoCrate-Launcher",
        },
      },
    ],
  },
];
