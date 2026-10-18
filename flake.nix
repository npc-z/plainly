{
  description = "Plainly development environment and packages";

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
    # crane splits a cargo build into a dependency-only derivation plus the
    # package that reuses it, which is what keeps a source edit from
    # recompiling every crates.io dependency.
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, flake-utils, fenix, crane }:
    # v0 ships Linux only. The structure is readest's (a parameterised mkShell
    # over an explicit system list) so that adding `aarch64-darwin` for v1 is a
    # two-line change — but an unverified darwin shell would rot and would read
    # as "macOS works", so it is deliberately absent.
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
      let
        inherit (nixpkgs) lib;

        # No fenix overlay: the toolchain is addressed through
        # `fenix.packages.${system}` below, so `pkgs` stays nixpkgs' own.
        pkgs = import nixpkgs { inherit system; };

        # The Rust toolchain, pinned to one exact release rather than to whatever
        # rustc nixpkgs happens to ship. `channel` is a release number from
        # static.rust-lang.org and the sha256 is that release's channel manifest,
        # so the compiler moves only when this file does — updating nixpkgs
        # cannot change it. (The workspace's `rust-version` floor is a separate
        # edit: the root `Cargo.toml` alone; the CLI package's manifest is
        # generated from it. A wrong hash is reported by the fetch with the
        # right one.)
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
        # handing its `buildRustPackage` hooks a toolchain other than its own;
        # only `cargo` and `rustc` are needed here, and both come from the same
        # 1.99.1 release as the shell's components, so a `cargo build` in
        # `nix develop` and a `nix build` compile with the same rustc.
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain.cargo;
          rustc = rustToolchain.rustc;
        };

        # crane's library, on the same release. `overrideToolchain` is how crane
        # is told about a toolchain that is not nixpkgs' own; `minimalToolchain`
        # is the same cargo/rustc pair `rustPlatform` above takes — one
        # derivation, so neither binding repeats the other's list, and the
        # analyzer, docs and formatter do not travel with a build.
        craneLib = (crane.mkLib pkgs).overrideToolchain (_: rustToolchain.minimalToolchain);

        # Tools for whoever is working on the code: the frontend's package
        # manager, a C toolchain for linking, and the odds and ends the tickets
        # reach for. None of this is needed to *run* plainly — this is a mkShell,
        # so the list is simply what ends up on PATH.
        #
        # `wl-clipboard` is here for `plainly explain --clipboard`, the headless
        # surface's reader of the session clipboard (ticket 12). The panel reads
        # the clipboard through data-control directly; this is what a client with
        # no window has instead. The main window *writes* the clipboard through
        # `wl-copy` from the same package, and falls back to `xclip` on a session
        # with no Wayland at all (ticket 14) — hence both.
        devTools = with pkgs; [
          pnpm
          nodejs_24
          # `python3` is asked for by name in two places a contributor is told to
          # run inside this shell: `scripts/check-headless-cli.sh` reads cargo's
          # JSON with it, and `scripts/stub-provider.py` is the fake provider the
          # manual end-to-end checklist uses. On a machine whose base system has
          # no python3, a non-pure `nix develop` would still find one by accident
          # through the host `PATH`; declaring it here is what makes it a
          # guarantee (AGENTS.md: a missing program is fixed in `flake.nix`).
          python3
          clang
          pkg-config
          xdg-utils
          patchelf
          wl-clipboard
          xclip
        ];

        # The editor's view of the code. Not needed to build or run anything,
        # but the alternative is every developer pointing their editor at a
        # rust-analyzer of their own, which may disagree with the toolchain above
        # and report errors the compiler does not have. This is the analyzer
        # component of the same 1.99.1 release, not fenix's nightly analyzer, so
        # the editor parses the same language the compiler accepts. It is still
        # a separate few hundred megabytes, so it is never implied: a shell that
        # wants it says `editors = true` (see `mkPlainlyShell` below), and no
        # future shell inherits the download by accident.
        rustAnalyzer = rustToolchain.rust-analyzer;

        # `glib` is here because `LD_LIBRARY_PATH` is built from this list alone,
        # and it is not transitive: gtk3's own lib dir is on the path, but the
        # `libglib-2.0` / `libgobject-2.0` / `libgio-2.0` its closure needs are
        # not, so the panel would link and then fail to start inside the shell.
        #
        # `at-spi2-core` (which is what the `atk` alias resolves to) is the same
        # story with a different library: gtk3's closure has
        # `libatk-1.0.so.0` in at-spi2-core's lib dir, which is not on the path
        # either, and the panel dies at startup with "cannot open shared object
        # file". The panel's own tests do not need it (they never open a display),
        # which is exactly why this gap only shows up when someone runs the panel
        # — so it is fixable here or it is rediscovered every time.
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
            buildInputs = libraries;

            env = {
              # readest sets `x11` here; plainly must not. layer-shell does not
              # exist on XWayland, and GTK picks X11 whenever DISPLAY is set.
              GDK_BACKEND = "wayland";
              LD_LIBRARY_PATH = lib.makeLibraryPath buildInputs;
            } // extraEnv;
          };

        # --- packages -------------------------------------------------------

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
        # before it builds anything, so *any* invocation against the real
        # workspace — including `cargo build --package plainly-cli` — reaches
        # `plainly-desktop` → `tauri` → `libdbus-sys`, whose build script panics
        # without a dbus C library. The command line must not need one (spec §2),
        # and a workspace without the GUI crates is what makes that true of the
        # built artifact rather than of the crate names.
        #
        # The manifest and the lock file are generated, not written here: they are
        # derived from the root ones by `scripts/generate-cli-workspace.sh` and
        # committed under `nix/cli-workspace/`, and
        # `scripts/check-cli-workspace.sh` fails when the two drift apart. The lock
        # is cargo's own normalisation of the pruned root lock, so it keeps the
        # versions the root lock pinned while dropping the optional edges only the
        # GUI activates — `cargo --locked`, which crane always passes, is what the
        # committed pair must satisfy. The lock is a small fraction of the real
        # workspace's package set.
        #
        # It is kept, rather than the root workspace built with `--package
        # plainly-cli`, because of one word in the sentence above: *panics*. Built
        # inside the sandbox, that path does run `libdbus-sys`'s build script and
        # fails on `explicit panic` (observed 2026-10-11); a `cargo build -p
        # plainly-cli` in the dev shell does *not*, because the shell has
        # `dbus-1.pc` and a compiler for the probe the script runs. The dev shell
        # is the one place the question cannot be asked, which is exactly why this
        # copy exists.
        # These two crates are also `MEMBERS` in `scripts/cli_workspace.py`;
        # adding a CLI crate edits both places (Nix cannot read the module).
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
        # `makeWrapper` is the GUI's alone — `wrapProgram` comes from it.
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
            homepage = "https://github.com/npc/plainly";
            license = lib.licenses.mit;
            platforms = [ "x86_64-linux" ];
            mainProgram = "plainly";
          };
        };

        # The command line on its own: one binary, no GTK, no WebKit, no dbus —
        # in its *build* inputs as well as its runtime closure. A buildInput here
        # would put the whole stack in `nix path-info -r` and hand a machine with
        # no graphics libraries something to substitute or fail on, which is the
        # opposite of what this package is for. The list is three tools long on
        # purpose; adding one is a decision, not an accident.
        #
        # The build is split the way crane expects. `plainlyDeps` compiles the
        # third-party crates against a dummified workspace (crane keeps only
        # `Cargo.lock` and the manifests, so a `.rs` edit does not reach it), and
        # `plainly` reuses that store output through `cargoArtifacts` and
        # compiles only the workspace's own crates. `gui` below uses the same
        # split over the real workspace.
        #
        # Two consequences of the move, both deliberate:
        # - the old `cargoBuildFlags = [ "--package" "plainly-cli" ]` is gone: it
        #   was a nixpkgs-only knob, and there is nothing to narrow here anyway —
        #   the synthetic CLI workspace is `plainly-core` + `plainly-cli`, so all
        #   of it is built, tested and installed either way.
        # - the binaries no longer carry cargo-auditable's `.dep-v0` section
        #   (nixpkgs' `buildRustPackage` adds it by default; crane has no
        #   equivalent). Nothing in this repo reads it, and `default` — the join
        #   of this package and `gui` — carries none of it either.

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
        # `gui` alone — `guiArgs` carries neither, which also keeps the
        # dependency build from running auto-patchelf over an artifact tarball.

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
          # `autoPatchelfHook` puts back the RUNPATH that nixpkgs' rustc used to
          # give linked binaries and fenix's does not: it resolves each ELF's
          # DT_NEEDED against the build inputs and patches the installed copies.
          # The `.wrapped` files are the real ELFs; the `bin/` names are the
          # `wrapProgram` scripts and are left alone.
          nativeBuildInputs = rustTools ++ [ pkgs.makeWrapper pkgs.autoPatchelfHook ];
          # The check phase runs the workspace's tests, which include the
          # panel's, and a build sandbox has no display for GTK to initialize
          # against. `PLAINLY_TEST_NO_DISPLAY` is that test's own way of being
          # told so: without it the test fails with the message naming the
          # variable, because a skip that nobody announces is a test that
          # checked nothing while looking green. The name says "test" on
          # purpose — nothing in the application reads it — and it means the
          # green check here does **not** cover that test: only a run with a
          # display does. (Merged rather than assigned: `//` is shallow, so a
          # bare `env` here would drop `common.env`'s `LIBCLANG_PATH`.)
          #
          # The check phase's test binaries are built by cargo, so
          # `autoPatchelfHook` never sees them (it only patches `$out`), and
          # fenix's rustc does not give them the RUNPATH nixpkgs' rustc did.
          # Without the library path the panel's test binary dies with
          # "libgobject-2.0.so.0: cannot open shared object file" (exit 127).
          env = common.env // {
            PLAINLY_TEST_NO_DISPLAY = "1";
            LD_LIBRARY_PATH = guiLibPath;
          };
          # Only the two GUI packages are built here. The command line it would
          # otherwise also produce is a *second* build of the same crate, so
          # `default` takes `plainly` below instead: one artifact, and the two
          # packages cannot drift apart. `symlinkJoin` is where they meet.
          # `cargoBuildExtraArgs` narrows the build phase (and therefore what is
          # installed) to the two GUI crates; the check phase still tests the
          # whole workspace. It must stay on `gui` and not move into `guiArgs`:
          # `buildDepsOnly` passes whatever it is given into its own build too.
          cargoBuildExtraArgs = "--package plainly-panel --package plainly-desktop";
          # The two GUI binaries are the ones that cannot start without their
          # libraries on the search path, and a compositor's key binding spawns the
          # panel with an environment of its own — no profile script, no
          # `LD_LIBRARY_PATH`. Wrapping is what an installed panel has instead of
          # the dev shell. `XDG_DATA_DIRS` is the same problem one layer up: GTK
          # reads its schemas from there, and the icon theme from `$out/share`.
          #
          # Built with a list joined by newlines rather than an indented string:
          # inside one of those, `for program in` ends the string at the `in` and a
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

        # `nix run .` runs the command line, and it runs the *joined* one: the
        # GUI derivation builds only the panel and the window
        # (`cargoBuildExtraArgs` above), so pointing an app at `${gui}/bin/plainly`
        # names a file that is not there and turns `nix run` into "No such file
        # or directory". The joined output is where the three binaries meet.
        apps = {
          default = {
            type = "app";
            program = "${self.packages.${system}.default}/bin/plainly";
          };
          plainly = {
            type = "app";
            program = "${plainly}/bin/plainly";
          };
        };

        formatter = pkgs.nixpkgs-fmt;

        # Deliberately empty, and the packages above do not change the reason: with
        # `packages` defined, `nix flake check` would build them — webkitgtk and
        # the whole Tauri stack — and turn a check into a release build, which is
        # what readest taught us to avoid. Packaging is verified by `nix build` /
        # `nix profile install` and `scripts/check-flake-packages.sh` instead, which
        # is also what CI would run if it ever built more than the two headless
        # crates.
        checks = { };
      });
}
