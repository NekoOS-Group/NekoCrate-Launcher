FROM ubuntu:22.04

LABEL org.opencontainers.image.source="https://github.com/NekoOS-Group/NekoCrate-Launcher"
LABEL org.opencontainers.image.description="Ubuntu 22.04 native dependencies for NekoCrate CI and Linux releases"

COPY scripts/install_linux_dependencies.sh scripts/linux_build_packages.txt /opt/nekocrate/
RUN DEBIAN_FRONTEND=noninteractive bash /opt/nekocrate/install_linux_dependencies.sh \
      $(cat /opt/nekocrate/linux_build_packages.txt) \
      ca-certificates curl git zstd unzip xz-utils python3 sudo \
      libbz2-1.0 libffi8 liblzma5 libncursesw6 libreadline8 libsqlite3-0 libgdbm6 libuuid1 \
    && dpkg-query -W > /opt/nekocrate/dpkg-manifest.txt \
    && rm -rf /var/lib/apt/lists/* /var/cache/apt/archives/*.deb

CMD ["/bin/bash"]
