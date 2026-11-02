{
  description = "ReqFlux - a local-first HTTP client";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    let
      releaseInfo = import ./nix/release-info.nix;
      nightlyInfo = import ./nix/nightly-info.nix;

      # Systems that ship a prebuilt binary in the matching GitHub Release.
      # Other systems can still use the source build.
      prebuiltSystems = builtins.attrNames releaseInfo.artifacts;
      nightlySystems = builtins.attrNames nightlyInfo.artifacts;

      reqfluxVersion = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;

      # Builds the ReqFlux packages from a package set. The flake's own
      # outputs pass the nixpkgs pinned in flake.lock; the overlay passes the
      # consumer's package set, so the prebuilt binary is patched against, and
      # the source build links with, the same glibc as the rest of the system,
      # including the graphics drivers it loads at runtime.
      mkPackages =
        pkgs:
        let
          lib = pkgs.lib;
          system = pkgs.stdenv.hostPlatform.system;

          # Native libraries GPUI needs to build and run.
          buildInputs =
            with pkgs;
            [
              openssl
              zstd
              zlib
              freetype
              fontconfig
            ]
            ++ lib.optionals stdenv.hostPlatform.isLinux [
              wayland
              libxkbcommon
              libx11
              libxcb
              libxcursor
              libxrandr
              libxi
              vulkan-loader
              alsa-lib
            ];

          nativeBuildInputs =
            with pkgs;
            [
              pkg-config
              cmake
              rustPlatform.bindgenHook
            ]
            ++ lib.optionals stdenv.hostPlatform.isLinux [ mold ];

          # GPUI loads these with dlopen at runtime.
          runtimeLibraryPath = lib.makeLibraryPath (
            with pkgs;
            lib.optionals stdenv.hostPlatform.isLinux [
              wayland
              libxkbcommon
              vulkan-loader
              libx11
              libxcb
              fontconfig
            ]
          );

          # Build the `reqflux` package with rustPlatform, like DBFlux's
          # non-flake path in default.nix. Builds `-p reqflux` so the
          # automation crates and their capture libraries stay out of the
          # package.
          #
          # Only tracked files reach the build in a git checkout, but filter
          # the common build output anyway so a plain directory copy also
          # builds.
          reqfluxSrc = lib.cleanSourceWith {
            src = ./.;
            filter =
              path: _type:
              (builtins.match ".*\\.git" path) == null
              && !lib.hasInfix "/target/" path
              # Root `nix build` symlinks: the final segment must be exactly
              # `result` or `result-*`, so `results.rs` and a `results/`
              # module survive.
              && (builtins.match ".*(/result)" path) == null
              && (builtins.match ".*(/result-[^/]*)" path) == null;
          };

          reqfluxDesktop = ./assets/icon/reqflux.desktop;

          # Source build: compiles locally.
          reqfluxSource = pkgs.rustPlatform.buildRustPackage {
            pname = "reqflux";
            version = reqfluxVersion;
            src = reqfluxSrc;

            cargoLock = {
              lockFile = ./Cargo.lock;
              allowBuiltinFetchGit = true;
            };

            inherit buildInputs;
            nativeBuildInputs = nativeBuildInputs ++ [ pkgs.makeWrapper ];

            cargoBuildFlags = [
              "-p"
              "reqflux"
            ];

            # Tests run in CI (.github/workflows/ci.yml), not here: GPUI tests
            # need a display, which the build sandbox does not have.
            doCheck = false;

            postInstall = ''
              install -Dm644 ${reqfluxDesktop} $out/share/applications/reqflux.desktop
              install -Dm644 ${./assets/icon/reqflux.svg} $out/share/icons/hicolor/scalable/apps/reqflux.svg
              install -Dm644 ${./assets/icon/reqflux-16.png} $out/share/icons/hicolor/16x16/apps/reqflux.png
              install -Dm644 ${./assets/icon/reqflux-32.png} $out/share/icons/hicolor/32x32/apps/reqflux.png
              install -Dm644 ${./assets/icon/reqflux-48.png} $out/share/icons/hicolor/48x48/apps/reqflux.png
              install -Dm644 ${./assets/icon/reqflux-64.png} $out/share/icons/hicolor/64x64/apps/reqflux.png
              install -Dm644 ${./assets/icon/reqflux-128.png} $out/share/icons/hicolor/128x128/apps/reqflux.png
              install -Dm644 ${./assets/icon/reqflux-256.png} $out/share/icons/hicolor/256x256/apps/reqflux.png
            '';

            postFixup = lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
              wrapProgram $out/bin/reqflux \
                --prefix LD_LIBRARY_PATH : "${runtimeLibraryPath}" \
                --suffix LD_LIBRARY_PATH : "/run/opengl-driver/lib:/usr/lib/x86_64-linux-gnu:/usr/lib64:/usr/lib"
            '';

            meta = with lib; {
              description = "A local-first HTTP client";
              homepage = "https://github.com/0xErwin1/reqflux";
              license = licenses.asl20;
              platforms = platforms.linux;
              mainProgram = "reqflux";
            };
          };

          # Prebuilt-binary build, only when a released artifact exists for
          # this system. ReqFlux has no tagged release yet, so this is null
          # until nix/release-info.nix lists one.
          hasPrebuilt = builtins.elem system prebuiltSystems;
          reqfluxBin = if hasPrebuilt then pkgs.callPackage ./nix/binary.nix { } else null;

          # Rolling nightly prebuilt, pinned on the `nightly` ref.
          # On `main` the hashes in nightly-info.nix are placeholders; always
          # consume this via `github:0xErwin1/reqflux/nightly#reqflux-nightly`.
          hasNightly = builtins.elem system nightlySystems;
          reqfluxNightly =
            if hasNightly then
              pkgs.callPackage ./nix/binary.nix { infoFile = ./nix/nightly-info.nix; }
            else
              null;

          # Default package: prefer the prebuilt binary when available (fast
          # install for end users), fall back to the source build.
          reqfluxDefault = if hasPrebuilt then reqfluxBin else reqfluxSource;
        in
        {
          inherit
            buildInputs
            nativeBuildInputs
            runtimeLibraryPath
            reqfluxSource
            hasPrebuilt
            reqfluxBin
            hasNightly
            reqfluxNightly
            reqfluxDefault
            ;
        };

      # Per-system outputs (packages, devShells, apps).
      perSystem = flake-utils.lib.eachDefaultSystem (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          lib = pkgs.lib;

          rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

          inherit (mkPackages pkgs)
            buildInputs
            nativeBuildInputs
            runtimeLibraryPath
            reqfluxSource
            reqfluxBin
            reqfluxNightly
            reqfluxDefault
            ;

          # Decided from `system` alone, not from `pkgs`, so listing the
          # outputs never imports nixpkgs for a system it no longer supports.
          hasPrebuilt = builtins.elem system prebuiltSystems;
          hasNightly = builtins.elem system nightlySystems;

        in
        {
          devShells.default = pkgs.mkShell {
            inherit buildInputs;
            nativeBuildInputs = nativeBuildInputs ++ [
              rustToolchain
              pkgs.cargo-nextest
              pkgs.cargo-deny
              # Rasterizes assets/icon/*.svg to the committed PNGs; see
              # ARCHITECTURE.md, "Packaging and releases".
              pkgs.imagemagick
              pkgs.python3
            ];
            LD_LIBRARY_PATH = runtimeLibraryPath;
          };

          # Packages:
          #   .default          -> prebuilt when available, source otherwise
          #   .reqflux          -> alias for .default
          #   .reqflux-source   -> explicit source build
          #   .reqflux-bin      -> explicit prebuilt (only once a release exists)
          #   .reqflux-nightly  -> rolling nightly prebuilt (pin to nightly ref)
          packages = {
            default = reqfluxDefault;
            reqflux = reqfluxDefault;
            reqflux-source = reqfluxSource;
          }
          // nixpkgs.lib.optionalAttrs hasPrebuilt { reqflux-bin = reqfluxBin; }
          // nixpkgs.lib.optionalAttrs hasNightly { reqflux-nightly = reqfluxNightly; };

          apps = {
            default = flake-utils.lib.mkApp {
              drv = reqfluxDefault;
              exePath = "/bin/reqflux";
            };
            reqflux = flake-utils.lib.mkApp {
              drv = reqfluxDefault;
              exePath = "/bin/reqflux";
            };
          }
          // nixpkgs.lib.optionalAttrs hasNightly {
            reqflux-nightly = flake-utils.lib.mkApp {
              drv = reqfluxNightly;
              exePath = "/bin/reqflux-nightly";
            };
          };

          formatter = pkgs.nixfmt-rfc-style;
        }
      );
    in
    perSystem
    // {
      # Overlay for downstream consumers:
      #
      #   nixpkgs.overlays = [ inputs.reqflux.overlays.default ];
      #   home.packages = [ pkgs.reqflux-nightly ];
      #
      # `pkgs.reqflux`         -> prebuilt binary once released, source otherwise
      # `pkgs.reqflux-source`  -> built from source
      # `pkgs.reqflux-bin`     -> explicit prebuilt (only on prebuilt systems)
      # `pkgs.reqflux-nightly` -> rolling nightly prebuilt (only on nightly systems)
      #
      # Every package is built from the consumer's own package set, not from
      # this flake's nixpkgs, so it uses the same glibc as the rest of the
      # system, including the graphics drivers it loads at runtime.
      #
      # Which attributes exist is decided from `prev` only: deciding it from
      # `final` would make the overlay's own attribute names depend on the
      # fixpoint it is part of, which is an infinite recursion.
      overlays.default =
        final: prev:
        let
          system = prev.stdenv.hostPlatform.system;
          hasSystem = perSystem.packages ? ${system};
          hasPrebuilt = builtins.elem system prebuiltSystems;
          hasNightly = builtins.elem system nightlySystems;
          consumerPkgs = mkPackages final;
        in
        if hasSystem then
          {
            reqflux = consumerPkgs.reqfluxDefault;
            reqflux-source = consumerPkgs.reqfluxSource;
          }
          // nixpkgs.lib.optionalAttrs hasPrebuilt { reqflux-bin = consumerPkgs.reqfluxBin; }
          // nixpkgs.lib.optionalAttrs hasNightly { reqflux-nightly = consumerPkgs.reqfluxNightly; }
        else
          { };
    };
}
