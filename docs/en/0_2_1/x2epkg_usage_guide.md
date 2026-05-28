# Using the x2epkg Tool

## Description

This section describes how to use the x2epkg tool to convert .rpm, .deb, or .pkg.tar.zst installation packages into [epkg](epkg_package_manager_usage_guide.md) packages. The operation example is executed under the `root` user account, without installing additional software packages.

## Download

```bash
git clone https://gitee.com/openeuler/epkg.git
```

## Quick Start

After downloading the epkg repository, go to the x2epkg directory and use the repository.

```shell
cd epkg/x2epkg
./x2epkg.sh --help
```

## Parameter Parsing

```txt
Usage:
    ./x2epkg xxx.rpm                                # Convert a single RPM package.
    ./x2epkg xxx.deb                                # Convert a single Debian package.
    ./x2epkg xxx.pkg.tar.zst                        # Convert a single Arch Linux package.
    ./x2epkg file_path/*.rpm                        # Convert multiple installation packages. This is also applies to .deb and .pkg.tar.zst files.
    ./x2epkg xxx.rpm --out-dir PATH                 # Specify the output directory by adding the out-dir parameter. If the parameter is not added, the output will be saved in the same directory as the package.
```

## Running Case

```shell
  cd /root
  wget https://mirrors.huaweicloud.com/archlinux/core/os/x86_64/fakeroot-1.37.1-1-x86_64.pkg.tar.zst
  cd epkg/x2epkg
  ./x2epkg.sh /root/fakeroot-1.37-1-x86_64.pkg.tar.zst
  ls /root/store/4t/4tmfsi5yikkq32rrulae6oi6u4txr5zu__fakeroot__1.37.1__1.epkg   # View the generation result.
```
