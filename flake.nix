{
  description = "Plainly development environment and packages";

  # The public binary cache `just cache-push` fills. The cache URL and key also
  # appear in the README's consumer instructions, and a flake input does not
  # carry this `nixConfig` into the importing flake, so a change here means a
  # change there too.
  nixConfig = {
    extra-substituters = [ "https://plainly-npc-z.cachix.org" ];
    extra-trusted-public-keys = [
      "plainly-npc-z.cachix.org-1:F7TqG/U7AJfUZ8McMct3VHbxchqKZSF/oSop0pv9/SE="
    ];
  };
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # crane splits a cargo build into a dependency-only derivation plus the
    # package that reuses it, which is what keeps a source edit from
    # recompiling every crates.io dependency.
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, flake-utils, fenix, crane }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
      let
        inherit (nixpkgs) lib;

        # No fenix overlay: the toolchain is addressed through
        # `fenix.packages.${system}` below, so `pkgs` stays nixpkgs' own.
        pkgs = import nixpkgs { inherit system; };

        # The Rust toolchain, pinned to one exact release rather than to whatever
        # rustc nixpkgs happens to ship. `channel` is a release number from
        # static.rust-lang.org and the sha256 is that release's channel manifest,
        # so the compiler moves only when this file does, and updating nixpkgs
        # cannot change it. The workspace's `rust-version` floor is a separate
        # edit in the root `Cargo.toml`, which the generated CLI manifest
        # inherits. A wrong hash is reported by the fetch with the right one.
        rustToolchain = fenix.packages.${system}.toolchainOf {
          channel = "1.99.1";
          sha256 = "sha256-+xUr6Al1wZSqX8aEsq7DpuFOK4Ve/i+qTl+K5uWQ9GM=";
        };

        # The components the shell puts on PATH, cut from the toolchain above
        # with `withComponents` (not `fenix.combine`, which is for mixing
        # components from *different* toolchains). One release means rustc,
        # cargo, clippy and rustfmt cannot disagree; `rust-src` is the standard
        # library source the editor's analyzer reads.
        shellComponents = rustToolchain.withComponents [
          "cargo"
          "clippy"
          "rust-src"
          "rustc"
          "rustfmt"
        ];

        # What the two packages build with. `makeRustPlatform` is nixpkgs' way of
        # handing its `buildRustPackage` hooks a toolchain other than its own.
        # Only `cargo` and `rustc` are needed here, and both come from the same
        # release as the shell's components, so `cargo build` in `nix develop` and
        # `nix build` compile with the same rustc.
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain.cargo;
          rustc = rustToolchain.rustc;
        };

        # crane's library, on the same release. `overrideToolchain` tells crane
        # about a toolchain that is not nixpkgs' own. `minimalToolchain` is the
        # same cargo/rustc pair `rustPlatform` takes, so the analyzer, docs and
        # formatter do not travel with a build.
        craneLib = (crane.mkLib pkgs).overrideToolchain (_: rustToolchain.minimalToolchain);

        # Tools for whoever is working on the code. None of them are needed to
        # run plainly.
        #
        # `wl-clipboard` is the CLI's `--clipboard` reader and the main window's
        # writer, because a client with no window cannot use GTK's clipboard. The
        # panel reads the clipboard through data-control directly. `xclip` is the
        # main window's fallback on a session with no Wayland at all.
        devTools = with pkgs; [
          pnpm
          nodejs_24
          # `just` runs the root `Justfile`, which holds the common cargo, Nix and
          # script invocations. It is declared here so `nix develop` then
          # `just check` works on a machine that does not install it globally.
          just
          # `python3` is asked for by name by two scripts a contributor runs in
          # this shell: `scripts/check-headless-cli.sh` reads cargo's JSON with
          # it, and `scripts/stub-provider.py` is the manual checklist's fake
          # provider. Declaring it here is what turns a non-pure `nix develop`
          # finding some host python3 by accident into a guarantee.
          python3
          clang
          pkg-config
          xdg-utils
          patchelf
          wl-clipboard
          xclip
        ];

        # The editor's view of the code. This is the analyzer component of the
        # same release as the compiler, not fenix's nightly analyzer, so the
        # editor parses the same language the compiler accepts. It is a separate
        # few hundred megabytes, so it is opt-in: a shell that wants it says
        # `editors = true`.
        rustAnalyzer = rustToolchain.rust-analyzer;

        # `LD_LIBRARY_PATH` is built from this list alone and is not transitive:
        # gtk3's own lib dir is on the path, but the `libglib-2.0`,
        # `libgobject-2.0` and `libgio-2.0` its closure needs are not, so the
        # panel links and then fails to start inside the shell. `at-spi2-core`
        # (which is what the `atk` alias resolves to) is the same story with
        # `libatk-1.0.so.0`. The panel's tests never open a display, which is why
        # the gap only shows up when someone runs the panel.
        libraries = with pkgs; [
          glib
          at-spi2-core
          gtk3
          gtk-layer-shell
          webkitgtk_4_1
          libsoup_3
          glib-networking
          openssl
        ];

        # What `cargo run` needs and an installed binary gets from
        # `autoPatchelfHook` instead: `target/debug/plainly-desktop` links these
        # three directly, fenix's rustc gives it no RUNPATH, and `libraries`
        # does not carry them, so without them it dies in the loader with
        # "cannot open shared object file". Shell-only on purpose: `libraries`
        # also feeds `packageInputs`, and adding them there would invalidate the
        # GUI package's dependency derivation (a full Rust dependency rebuild)
        # while changing nothing an installed binary needs.
        shellLibraries = libraries ++ (with pkgs; [ gdk-pixbuf cairo dbus ]);

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
              ++ [ shellComponents ];
            buildInputs = shellLibraries;

            env = {
              # Wayland only: layer-shell does not exist on XWayland, and GTK
              # picks X11 whenever DISPLAY is set.
              GDK_BACKEND = "wayland";
              LD_LIBRARY_PATH = lib.makeLibraryPath buildInputs;
            } // extraEnv;
          };

        # Everything in the repository, with build output and history left out.
        # Without this filter the source copy carries `target/` — tens of
        # gigabytes of it — into the store on every build.
        wholeSource = lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            let
              base = baseNameOf (toString path);
            in
              !(builtins.elem base [
                "target"
                ".git"
                "result"
                ".direnv"
                "__pycache__"
              ]);
        };

        # The command line's own workspace: `plainly-core` and `plainly-cli`, and
        # nothing else.
        #
        # This is the split, not tidiness. Cargo resolves every workspace member
        # before it builds anything, so any invocation against the real workspace,
        # including `cargo build --package plainly-cli`, reaches
        # `plainly-desktop` → `tauri` → `libdbus-sys`, whose build script panics in
        # a build sandbox without a dbus C library. The command line must not need
        # one (spec §2), and a workspace without the GUI crates is what makes that
        # true of the built artifact rather than of the crate names.
        #
        # Building the real workspace with `--package plainly-cli` is not an
        # alternative, because in the sandbox that path still runs the build script
        # and fails on its panic. In the dev shell it succeeds, because the shell
        # has `dbus-1.pc` and a compiler for the probe, which is why the sandbox
        # case needs a source tree of its own.
        #
        # The manifest and lock are generated by
        # `scripts/generate-cli-workspace.sh` and committed under
        # `nix/cli-workspace/`, so edit the root manifests and regenerate rather
        # than editing them here.
        #
        # The same two crates are `MEMBERS` in `scripts/cli_workspace.py`: a new
        # CLI crate edits both, because Nix cannot read the module.
        cliSource = pkgs.runCommand "plainly-cli-source" { } ''
          mkdir -p $out/crates
          cp ${./nix/cli-workspace/Cargo.toml} $out/Cargo.toml
          cp ${./nix/cli-workspace/Cargo.lock} $out/Cargo.lock
          cp -r ${wholeSource}/crates/plainly-core $out/crates/plainly-core
          cp -r ${wholeSource}/crates/plainly-cli $out/crates/plainly-cli
        '';

        # The vendored crates.io sources: fetched once, cached by the store, and
        # only invalidated when the lock file those crates come from changes — and
        # then Nix says so with the hash it got. The CLI's workspace resolves its
        # own lock file, which is why the two hashes differ.
        cliCargoDeps = pkgs.rustPlatform.fetchCargoVendor {
          src = cliSource;
          hash = "sha256-OVl064aNTeMQsK/SGdS5W/WI2ofO0zYlb6zWTbJrhqM=";
        };
        guiCargoDeps = pkgs.rustPlatform.fetchCargoVendor {
          src = wholeSource;
          hash = "sha256-GVGZzaz68c2U37UMvpiehysU5U9xZYfT0Ms5gy4MJYc=";
        };

        # nixpkgs' `fetchCargoVendor` lays the vendored crates out under
        # `source-registry-0/`, with a `.cargo/config.toml` beside it that crane
        # ignores unless it sits at the vendor root. crane hands cargo the
        # directory it is given, so the subdirectory is what gets passed; the
        # name is that derivation's layout, not ours, and it lives here once
        # rather than in each package.
        vendorDir = deps: "${deps}/source-registry-0";

        # What the GUI package builds with. The dev shell's lists above are the
        # same set for the same reasons, and `dbus` is the one addition:
        # `plainly-desktop` reaches `tauri` → `libdbus-sys`, which probes for
        # `dbus-1.pc` and panics without it. The command line's package is built
        # from `cliSource` and never sees this list.
        packageInputs = libraries ++ (with pkgs; [ dbus ]);

        # What the two wrapped binaries need on their search path at run time, and
        # what GTK needs on `XDG_DATA_DIRS`.
        guiLibPath = lib.makeLibraryPath packageInputs;
        guiDataDirs = "${pkgs.gtk3}/share:${pkgs.gsettings-desktop-schemas}/share";

        # Tools a Rust build of this workspace needs, with the reason each one is
        # here: `pkg-config` for the `-sys` crates, `clang` because `rusqlite`'s
        # `bundled` feature compiles SQLite, and `patchelf` for the fixup phase.
        rustTools = with pkgs; [
          pkg-config
          clang
          patchelf
        ];

        common = {
          version = "0.1.0";
          env.LIBCLANG_PATH = "${pkgs.llvm.lib}/lib";
          meta = {
            description = "Make difficult English understandable in English";
            homepage = "https://github.com/npc-z/plainly";
            license = lib.licenses.mit;
            platforms = [ "x86_64-linux" ];
            mainProgram = "plainly";
          };
        };

        # The command line on its own: one binary, and no GTK, WebKit or dbus in
        # its build inputs as well as its runtime closure. A buildInput here would
        # put the whole stack in `nix path-info -r` and hand a machine with no
        # graphics libraries something to substitute or fail on. The list is three
        # tools long on purpose; adding one is a decision, not an accident.
        #
        # `plainlyDeps` compiles the third-party crates against a dummified
        # workspace (crane keeps only `Cargo.lock` and the manifests, so a `.rs`
        # edit does not reach it), and `plainly` reuses that store output through
        # `cargoArtifacts` and compiles only the workspace's own crates. `gui`
        # below uses the same split over the real workspace.

        plainlyArgs = common // {
          pname = "plainly";
          src = cliSource;
          cargoVendorDir = vendorDir cliCargoDeps;
          nativeBuildInputs = rustTools;
          buildInputs = [ pkgs.openssl ];
        };

        plainlyDeps = craneLib.buildDepsOnly plainlyArgs;

        plainly = craneLib.buildPackage (plainlyArgs // {
          cargoArtifacts = plainlyDeps;
          meta = common.meta // {
            description = "Explain a Passage of hard English, from the command line";
          };
        });

        # The whole application: the panel a compositor's key binding spawns, the
        # main window, and the CLI, plus what a desktop needs to find them.
        #
        # What is specific to this package is the hook separation:
        # `buildDepsOnly` inherits every argument it is given, and a dependency
        # build has no binaries to wrap (it dies with "Cannot wrap
        # .../bin/plainly-panel"), so the wrapper and the rpath hook belong to
        # `gui` alone. Keeping them out of `guiArgs` also keeps the dependency
        # build from running auto-patchelf over an artifact tarball.

        guiArgs = common // {
          pname = "plainly-desktop";
          src = wholeSource;
          cargoVendorDir = vendorDir guiCargoDeps;
          nativeBuildInputs = rustTools;
          buildInputs = packageInputs;
        };

        guiDeps = craneLib.buildDepsOnly guiArgs;

        gui = craneLib.buildPackage (guiArgs // {
          cargoArtifacts = guiDeps;
          # `autoPatchelfHook` puts back the RUNPATH that nixpkgs' rustc gives
          # linked binaries and fenix's does not: it resolves each ELF's
          # DT_NEEDED against the build inputs and patches the installed copies.
          # The `.wrapped` files are the real ELFs; the `bin/` names are the
          # `wrapProgram` scripts and are left alone.
          nativeBuildInputs = rustTools ++ [ pkgs.makeWrapper pkgs.autoPatchelfHook ];
          # The check phase runs the workspace's tests, which include the
          # panel's, and a build sandbox has no display for GTK to initialize
          # against. `PLAINLY_TEST_NO_DISPLAY` is that test's own way of being
          # told so, because a skip nobody announces is a test that checked
          # nothing while looking green. The name says "test" on purpose and
          # nothing in the application reads it, so a green check here does
          # **not** cover that test. Only a run with a display does. (Merged
          # rather than assigned, because `//` is shallow and a bare `env` here
          # would drop `common.env`'s `LIBCLANG_PATH`.)
          #
          # The check phase's test binaries are built by cargo, so
          # `autoPatchelfHook` never sees them (it only patches `$out`), and
          # fenix's rustc does not give them the RUNPATH nixpkgs' rustc does.
          # Without the library path the panel's test binary dies with
          # "libgobject-2.0.so.0: cannot open shared object file" (exit 127).
          env = common.env // {
            PLAINLY_TEST_NO_DISPLAY = "1";
            LD_LIBRARY_PATH = guiLibPath;
          };
          # Only the two GUI packages are built here. The command line it would
          # otherwise also produce is a second build of the same crate, so
          # `default` takes `plainly` below instead, and the two packages cannot
          # drift apart. `cargoBuildExtraArgs` narrows the build phase (and
          # therefore what is installed) to the two GUI crates, while the check
          # phase still tests the whole workspace. It must stay on `gui` and not
          # move into `guiArgs`, because `buildDepsOnly` passes whatever it is
          # given into its own build too.
          cargoBuildExtraArgs = "--package plainly-panel --package plainly-desktop";
          # The two GUI binaries are the ones that cannot start without their
          # libraries on the search path, and a compositor's key binding spawns
          # the panel with an environment of its own: no profile script, no
          # `LD_LIBRARY_PATH`. Wrapping is what an installed panel has instead of
          # the dev shell. `XDG_DATA_DIRS` is the same problem one layer up, where
          # GTK reads its schemas, and the icon theme comes from `$out/share`.
          #
          # Built with a list joined by newlines rather than an indented string.
          # Inside one of those, `for program in` ends the string at the `in`, a
          # trailing backslash is an escape, and a heredoc terminator right after
          # the closing quote reads as the start of another string.
          postFixup = lib.concatStringsSep "\n" [
            "wrapProgram $out/bin/plainly-panel --prefix LD_LIBRARY_PATH : ${guiLibPath} --prefix XDG_DATA_DIRS : ${guiDataDirs}"
            "wrapProgram $out/bin/plainly-desktop --prefix LD_LIBRARY_PATH : ${guiLibPath} --prefix XDG_DATA_DIRS : ${guiDataDirs}"
            "install -Dm644 ${./icons/icon.png} $out/share/icons/hicolor/512x512/apps/dev.plainly.app.png"
            "install -Dm644 ${./packaging/dev.plainly.app.desktop} $out/share/applications/dev.plainly.app.desktop"
          ];
          meta = common.meta // {
            description = "The Plainly panel, main window and command line";
            mainProgram = "plainly-desktop";
          };
        });
      in
      {
        # The shell for working on the code, so the one that asks for the editor
        # tooling. Nothing else does, which is the point.
        #
        # `cachix` is the client `just cache-push` drives. It is a publishing tool
        # for whoever fills the cache, not something building or running plainly
        # needs, so it goes to this shell rather than into `devTools`.
        devShells.default = mkPlainlyShell {
          name = "plainly-dev";
          editors = true;
          extraTools = [ pkgs.cachix ];
        };

        # `sqlite3` is not needed to build or run plainly, because `rusqlite`'s
        # `bundled` feature compiles SQLite in, but it is the tool you reach for
        # when inspecting the history store by hand. The attribute is `sqlite`,
        # which ships the `sqlite3` binary.
        devShells.inspect = mkPlainlyShell {
          name = "plainly-inspect";
          extraTools = [ pkgs.sqlite ];
        };

        # `plainly` is the package for a machine with no graphics stack, so it is
        # the one that has to stay installable on its own; `default` is the whole
        # application. The split is a promise about closures, not about names —
        # `scripts/check-flake-packages.sh` asserts it against the built
        # derivation, because a name cannot carry that property.
        packages = {
          inherit plainly;
          # The whole application, and the *same* CLI a `nix profile install .#plainly`
          # gives: `symlinkJoin` over the two builds rather than a third copy of
          # `plainly` inside the GUI derivation. Two builds of one crate are two
          # artifacts that can disagree; a link cannot.
          default = pkgs.symlinkJoin {
            name = "plainly-${common.version}";
            paths = [
              gui
              plainly
            ];
            meta = common.meta // {
              description = "The Plainly panel, main window and command line";
              mainProgram = "plainly-desktop";
            };
          };
        };

        # `nix run .` runs the joined command line, because the GUI derivation
        # builds only the panel and the window (`cargoBuildExtraArgs` above) and
        # `${gui}/bin/plainly` is therefore not there. The joined output is where
        # the three binaries meet.
        #
        # `desktop` and `panel` name the GUI binaries directly, so
        # `nix run .#desktop` and `nix run .#panel -- --clipboard` start the
        # wrapped release binaries an install gets, not the dev shell's
        # `target/debug`. `default` stays the command line on purpose, because it
        # is the one a machine without a display can run.
        apps = {
          default = {
            type = "app";
            program = "${self.packages.${system}.default}/bin/plainly";
          };
          plainly = {
            type = "app";
            program = "${plainly}/bin/plainly";
          };
          desktop = {
            type = "app";
            program = "${gui}/bin/plainly-desktop";
          };
          panel = {
            type = "app";
            program = "${gui}/bin/plainly-panel";
          };
        };

        formatter = pkgs.nixpkgs-fmt;

        # Deliberately empty: with `packages` defined, `nix flake check` would
        # build them, webkitgtk and the whole Tauri stack, and turn a check into a
        # release build. Packaging is verified by `nix build` / `nix profile
        # install` and `scripts/check-flake-packages.sh` instead.
        checks = { };
      });
}
