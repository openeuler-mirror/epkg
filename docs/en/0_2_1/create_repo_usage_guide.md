# Using the create_repo Tool

## Description

This section describes how to use the create_repo tool to convert an epkg package directory structure into a local repository. The operation example is executed under the `root` user account, without installing additional software packages.

## Download

```bash
git clone https://gitee.com/openeuler/epkg.git
```

## Quick Start

After downloading the epkg repository, go to the create_repo directory and use the repository.

```shell
cd epkg/create_repo
python3 create_repo.py --help
```

## Prerequisites

The input directory is a package directory structure shown below.

```txt
# tree store/
store/
|-- 45
|   `-- 45dqn3vzum73pf7igmmu6zbcxndqqebr__glibc-devel__2.38__29.oe2403.epkg
|-- at
|   `-- atqwhznwbp6lzvur5stgb4jjem6tzllf__redis__4.0.14__6.oe2403.epkg
|-- bt
|   `-- btblk472ob4teixd522qgv6b2c7tk4v4__ncurses-base__5.9+20140118__1ubuntu1.epkg
|-- f7
|   `-- f7ealc3ghq6tdmvrh22l5vlxycgndwam__atlas__3.10.3__10.oe1.epkg
|-- jl
|   `-- jlrwn3rcjb4gbdx4uf77lslxg37rodyf__glibc__2.38__29.oe2403.epkg
|-- kc
|   `-- kclugzl6mtkqeqkgbmpf2vq4kpkeleqb__atlas-devel__3.10.3__10.oe1.epkg
`-- of
    `-- ofdjbysy76gs5tzzgxulaoumgqjfe6d2__audit__4.0.3__1.epkg

8 directories, 7 files
```

## Parameter Parsing

```txt
usage: create_repo.py [-h] -s STORE -c CONFIG

create repo parameters

optional arguments:
  -h, --help            Checks command parameters.
  -s STORE, --store STORE
                        Enter the store directory of the epkg package repository.
  -c CONFIG, --config CONFIG
                        Enter the address of the repository list configuration file.
```

## Running Case

```shell
  cd epkg/x2epkg
  python3 create_repo.py -s /root/store -c /root/config.yaml
  ls /root/repodata  # Check the generated result. The output path is the same as the path of the store repository.
```
