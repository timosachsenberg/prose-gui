#!/bin/bash
# Run a command in the ProSE Tauri build and test environment of a host without root: Rust and
# tauri-driver from the folder of the user-space tools, Node, and WebKitGTK 4.1 and friends from the
# sysroot that native/scripts/make-sysroot.sh builds.
#
#   in-env.sh [--disable-webkit-sandbox] <cmd...>
#
#   --disable-webkit-sandbox   sets WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1. WebKitGTK sandboxes
#                              its web and network processes with bubblewrap, which needs
#                              unprivileged user namespaces. Where the host forbids them (AppArmor)
#                              and has no /usr/bin/bwrap, which is where WebKit looks for it, the
#                              processes start unsandboxed anyway and a webview renders without
#                              this flag. The flag only matters should a /usr/bin/bwrap appear.
#                              Use it for tests of the app on such a host only, never for
#                              anything that loads untrusted content.
#
# Environment: PROSE_TOOLS is the folder of the user-space tools (in it: rust/rustup, rust/cargo and
# tauri-sysroot; native/README.md, "Tauri toolchain without root"). TAURI_SYSROOT, RUSTUP_HOME and
# CARGO_HOME name one of its parts elsewhere. NODE_BIN is the folder of the Node to use (default: the
# node on the PATH).
set -euo pipefail

TOOLS=${PROSE_TOOLS:-}
SYSROOT=${TAURI_SYSROOT:-${TOOLS:+$TOOLS/tauri-sysroot}}
if [ -z "$SYSROOT" ]; then
  echo "error: set PROSE_TOOLS to the folder of the user-space tools (or TAURI_SYSROOT to the sysroot); see native/README.md, \"Tauri toolchain without root\"" >&2
  exit 1
fi
if [ -n "$TOOLS" ]; then
  export RUSTUP_HOME=${RUSTUP_HOME:-$TOOLS/rust/rustup}
  export CARGO_HOME=${CARGO_HOME:-$TOOLS/rust/cargo}
fi
NODE_BIN=${NODE_BIN:-}

sandbox_flag=0
if [ "${1:-}" = "--disable-webkit-sandbox" ]; then
  sandbox_flag=1
  shift
fi
[ "$#" -ge 1 ] || { echo "usage: $0 [--disable-webkit-sandbox] <cmd...>" >&2; exit 2; }
[ -d "$SYSROOT/usr/lib/x86_64-linux-gnu" ] || { echo "error: no sysroot at $SYSROOT (run native/scripts/make-sysroot.sh)" >&2; exit 1; }

LIBDIRS="$SYSROOT/usr/lib/x86_64-linux-gnu:$SYSROOT/lib/x86_64-linux-gnu"

# Host PATH without linuxbrew, behind the sysroot, cargo and node.
HOST_PATH=$(printf '%s' "$PATH" | tr ':' '\n' | grep -v 'linuxbrew' | paste -sd: -)
export PATH="$SYSROOT/usr/bin:${CARGO_HOME:+$CARGO_HOME/bin:}${NODE_BIN:+$NODE_BIN:}$HOST_PATH"

# Compile and link against the sysroot. The .pc files of the sysroot already name its paths, so
# PKG_CONFIG_SYSROOT_DIR stays unset: it would also prefix the host's own .pc files.
export PKG_CONFIG_PATH="$SYSROOT/usr/lib/x86_64-linux-gnu/pkgconfig:$SYSROOT/usr/share/pkgconfig:$SYSROOT/usr/lib/pkgconfig"
export LIBRARY_PATH="$LIBDIRS${LIBRARY_PATH:+:$LIBRARY_PATH}"
export LD_LIBRARY_PATH="$LIBDIRS${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

# Data, modules and schemas. GIO_EXTRA_MODULES adds the sysroot's GIO modules (glib-networking,
# proxy) to the host's; GIO_MODULE_DIR would replace the host's directory. GSETTINGS_SCHEMA_DIR
# adds a schema directory to the host's. WebKit's own helper processes are found because
# make-sysroot.sh patches the library (WEBKIT_EXEC_PATH is not honoured by distribution builds).
export XDG_DATA_DIRS="$SYSROOT/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
export GIO_EXTRA_MODULES="$SYSROOT/usr/lib/x86_64-linux-gnu/gio/modules"
export GSETTINGS_SCHEMA_DIR="$SYSROOT/usr/share/glib-2.0/schemas"
if [ -f "$SYSROOT/usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0/2.10.0/loaders.cache" ]; then
  export GDK_PIXBUF_MODULE_FILE="$SYSROOT/usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0/2.10.0/loaders.cache"
fi

# Software GL from the sysroot's Mesa, which make-sysroot.sh may have upgraded over the host's.
if [ -d "$SYSROOT/usr/lib/x86_64-linux-gnu/dri" ]; then
  export LIBGL_DRIVERS_PATH="$SYSROOT/usr/lib/x86_64-linux-gnu/dri"
fi
export __EGL_VENDOR_LIBRARY_DIRS="$SYSROOT/usr/share/glvnd/egl_vendor.d:/usr/share/glvnd/egl_vendor.d"

if [ "$sandbox_flag" = 1 ]; then
  export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1
fi

exec "$@"
