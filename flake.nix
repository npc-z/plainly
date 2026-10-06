{
  description = "Plainly development environment";

  # No `nixConfig` block: that is where readest points at its public Cachix
  # cache, and we have no equivalent. Declaring a cache that does not exist is
  # noise; development works off the upstream binary caches already configured
  # on the machine (`substituters`), because `nix develop` pulls prebuilt
  # dependencies rather than building the Rust stack from source.
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, fenix }:
    # v0 ships Linux only. The structure is readest's (a parameterised mkShell
    # over an explicit system list) so that adding `aarch64-darwin` for v1 is a
    # two-line change — but an unverified darwin shell would rot and would read
    # as "macOS works", so it is deliberately absent.
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
      let
        inherit (nixpkgs) lib;

        pkgs = import nixpkgs {
          inherit system;
          overlays = [ fenix.overlays.default ];
        };

        toolchain = with pkgs.fenix.complete; [
          cargo
          clippy
          rust-src
          rustc
          rustfmt
        ];

        nativeBuildTools = with pkgs; [
          pnpm
          nodejs_24
          clang
          pkg-config
          xdg-utils
          patchelf
        ];

        libraries = with pkgs; [
          gtk3
          gtk-layer-shell
          webkitgtk_4_1
          libsoup_3
          glib-networking
          openssl
        ];

        mkPlainlyShell = { name, extraTools ? [ ], extraEnv ? { } }: pkgs.mkShell rec {
          inherit name;

          nativeBuildInputs =
            nativeBuildTools ++ extraTools ++ [ (pkgs.fenix.combine toolchain) ];
          buildInputs = libraries;

          env = {
            # readest sets `x11` here; plainly must not. layer-shell does not
            # exist on XWayland, and GTK picks X11 whenever DISPLAY is set.
            GDK_BACKEND = "wayland";
            LD_LIBRARY_PATH = lib.makeLibraryPath buildInputs;
          } // extraEnv;
        };
      in
      {
        devShells.default = mkPlainlyShell { name = "plainly-dev"; };

        # `sqlite3` is not needed to build or run plainly — `rusqlite`'s
        # `bundled` feature compiles SQLite in — but it is the tool you reach
        # for when inspecting the history store by hand. The attribute is
        # `sqlite` (which ships the `sqlite3` binary).
        devShells.inspect = mkPlainlyShell {
          name = "plainly-inspect";
          extraTools = [ pkgs.sqlite ];
        };

        formatter = pkgs.nixpkgs-fmt;

        # Deliberately empty. readest learned this the expensive way: `checks`
        # pointed at the app build turns `nix flake check` into a full release
        # build. CI is deferred out of v0 anyway; when it lands it runs
        # `cargo test -p plainly-cli`, not a flake check.
        checks = { };
      });
}
