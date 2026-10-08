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

        # Tools for whoever is working on the code: the frontend's package
        # manager, a C toolchain for linking, and the odds and ends the tickets
        # reach for. None of this is needed to *run* plainly — this is a mkShell,
        # so the list is simply what ends up on PATH.
        #
        # `wl-clipboard` is here for `plainly explain --clipboard`, the headless
        # surface's reader of the session clipboard (ticket 12). The panel reads
        # the clipboard through data-control directly; this is what a client with
        # no window has instead.
        devTools = with pkgs; [
          pnpm
          nodejs_24
          clang
          pkg-config
          xdg-utils
          patchelf
          wl-clipboard
        ];

        # The editor's view of the code. Not needed to build or run anything,
        # but the alternative is every developer pointing their editor at a
        # rust-analyzer of their own, which may disagree with the toolchain above
        # and report errors the compiler does not have. Taken from fenix, not
        # nixpkgs, so both speak the same nightly — which also makes it a separate
        # few hundred megabytes. So it is never implied: a shell that wants it
        # says `editors = true` (see `mkPlainlyShell` below), and no future shell
        # inherits the download by accident.
        rustAnalyzer = fenix.packages.${system}.rust-analyzer;

        # `glib` is here because `LD_LIBRARY_PATH` is built from this list alone,
        # and it is not transitive: gtk3's own lib dir is on the path, but the
        # `libglib-2.0` / `libgobject-2.0` / `libgio-2.0` its closure needs are
        # not, so the panel would link and then fail to start inside the shell.
        libraries = with pkgs; [
          glib
          gtk3
          gtk-layer-shell
          webkitgtk_4_1
          libsoup_3
          glib-networking
          openssl
        ];

        mkPlainlyShell =
          { name
          , editors ? false
          , extraTools ? [ ]
          , extraEnv ? { }
          }:
          pkgs.mkShell rec {
            inherit name;

            nativeBuildInputs = devTools
              ++ lib.optionals editors [ rustAnalyzer ]
              ++ extraTools
              ++ [ (pkgs.fenix.combine toolchain) ];
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
        # The shell for working on the code, so the one that asks for the editor
        # tooling. Nothing else does, which is the point.
        devShells.default = mkPlainlyShell {
          name = "plainly-dev";
          editors = true;
        };

        # `sqlite3` is not needed to build or run plainly — `rusqlite`'s
        # `bundled` feature compiles SQLite in — but it is the tool you reach for
        # when inspecting the history store by hand. The attribute is `sqlite`
        # (which ships the `sqlite3` binary).
        #
        # This is the shell you enter to look at the store rather than to write
        # Rust, so it does not ask for the editor tools and stays lean. Anyone
        # editing Rust wants `devShells.default`.
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
