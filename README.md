# Tap

Tap is a tool for easily building your C/C++ projects.

Make has its flaws, but it still outshines most other build systems (namely CMake and Meson, which are quite mainstream) in its user interface for us people who make the shell their home -- just run `make`. No need to create a build directory, run some command and *then* run `make`/`ninja` *in the build directory*.

So, Tap is a layer of abstraction over the mess that is C/C++ build systems, providing a simple uniform interface. See the [Usage](#usage) section for examples.

# Installation

TODO

# Usage

## Build the project

```
tap build
```

or

```
tap b
```

You can also specify a build mode, as such:

```
tap build -m release
```

## Run an executable

```
tap run <EXECUTABLE> -- <ARGS>
```

or

```
tap r <EXECUTABLE> -- <ARGS>
```

Just like with `tap build`, you can pass a build mode.

Note: the `--` is just to prevent any arguments meant for your executable from being recognized by tap itself.

## Clean build files

```
tap clean
```

or

```
tap c
```

## Install the built binaries

```
tap install
```

or

```
tap i
```

If you run as root (e.g., with `sudo`), then the binaries will be installed to `/usr/local`.
If not run as root, then they will be installed to the parent directory of the user's executable directory (which usually means `~/.local`).

You can also specify a custom install prefix:

```
tap install --prefix <INSTALLATION_PREFIX>
```
