# Prebuilt-binary derivation for ReqFlux.
#
# Pulls the matching tarball from the GitHub Release named by the info file
# (./release-info.nix by default, ./nightly-info.nix for the rolling nightly)
# and installs it into a Nix store path. Avoids compiling from source on the
# consumer machine.
#
# Supported systems: the ones listed in the info file's `artifacts`
# (x86_64-linux today). Other systems fall back to the source build (see
# flake.nix).

{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  makeWrapper,
  # Runtime libraries: must mirror the `buildInputs` of the source build in
  # flake.nix so the patched binary finds the same .so files at runtime.
  openssl,
  zstd,
  zlib,
  fontconfig,
  freetype,
  wayland,
  libxkbcommon,
  libx11,
  libxcb,
  libxcursor,
  libxrandr,
  libxi,
  vulkan-loader,
  alsa-lib,
  dbus,
  stdenv_cc_libs ? stdenv.cc.cc.lib,
  # Which info file to read. Defaults to the stable channel pointer; pass
  # ./nightly-info.nix to get the rolling nightly package instead.
  infoFile ? ./release-info.nix,
}:

let
  releaseInfo = import infoFile;
  system = stdenv.hostPlatform.system;
  artifact =
    releaseInfo.artifacts.${system} or (throw (
      "reqflux: no prebuilt binary published for system '${system}'. "
      + "Use the source build (packages.reqflux-source) instead."
    ));

  # Per-channel install identity: nightly installs under distinct binary,
  # desktop and icon names so a stable and a nightly package can coexist in
  # one profile (e.g. a home-manager `buildEnv`) without colliding on
  # `bin/reqflux`.
  isNightly = lib.hasInfix "nightly" releaseInfo.version;
  appId = if isNightly then "reqflux-nightly" else "reqflux";
  appName = if isNightly then "ReqFlux Nightly" else "ReqFlux";
in
stdenv.mkDerivation {
  pname = appId;
  version = releaseInfo.version;

  src = fetchurl {
    inherit (artifact) url hash;
  };

  # The tarball expands directly into the working directory (no leading
  # top-level folder), so unpack manually.
  sourceRoot = ".";
  unpackPhase = ''
    runHook preUnpack
    mkdir -p source
    tar -xzf $src -C source
    runHook postUnpack
  '';

  nativeBuildInputs = [
    autoPatchelfHook
    makeWrapper
  ];

  # Libraries the binary links against (autoPatchelfHook resolves these via
  # DT_NEEDED) plus the ones it dlopens at runtime (added to LD_LIBRARY_PATH
  # by the wrapper below).
  buildInputs = [
    stdenv_cc_libs
    openssl
    zstd
    zlib
    fontconfig
    freetype
    wayland
    libxkbcommon
    libx11
    libxcb
    libxcursor
    libxrandr
    libxi
    vulkan-loader
    alsa-lib
    dbus
  ];

  dontBuild = true;
  dontStrip = true;

  # Tarball layout, from .github/workflows/nightly.yml:
  #   bin/reqflux
  #   share/applications/reqflux.desktop
  #   share/icons/hicolor/{scalable,256x256}/apps/reqflux.{svg,png}
  #   LICENSE-APACHE
  installPhase = ''
    runHook preInstall

    cd source

    install -Dm755 bin/reqflux $out/bin/${appId}

    # Channel-scoped desktop entry and icons, so a stable and a nightly
    # package can coexist in one profile.
    install -Dm644 share/applications/reqflux.desktop \
      $out/share/applications/${appId}.desktop
    sed -i \
      -e 's|^Exec=reqflux|Exec=${appId}|' \
      -e 's|^Icon=reqflux$|Icon=${appId}|' \
      -e 's|^Name=ReqFlux$|Name=${appName}|' \
      $out/share/applications/${appId}.desktop

    install -Dm644 share/icons/hicolor/scalable/apps/reqflux.svg \
      $out/share/icons/hicolor/scalable/apps/${appId}.svg
    install -Dm644 share/icons/hicolor/256x256/apps/reqflux.png \
      $out/share/icons/hicolor/256x256/apps/${appId}.png

    install -Dm644 LICENSE-APACHE $out/share/licenses/${appId}/LICENSE-APACHE

    runHook postInstall
  '';

  # Wayland/Vulkan/X11 libraries are dlopened at runtime; autoPatchelf cannot
  # detect these because they are not in DT_NEEDED. Inject them via
  # LD_LIBRARY_PATH on the wrapper, plus the host graphics drivers like the
  # source build.
  postFixup = ''
    wrapProgram $out/bin/${appId} \
      --prefix LD_LIBRARY_PATH : "${
        lib.makeLibraryPath [
          wayland
          libxkbcommon
          vulkan-loader
          libx11
          libxcb
          libxcursor
          libxrandr
          libxi
          fontconfig
          dbus
        ]
      }" \
      --suffix LD_LIBRARY_PATH : "/run/opengl-driver/lib"
  '';

  meta = with lib; {
    description = "A local-first HTTP client (prebuilt binary)";
    homepage = "https://github.com/0xErwin1/reqflux";
    license = licenses.asl20;
    mainProgram = appId;
    platforms = builtins.attrNames releaseInfo.artifacts;
    sourceProvenance = with sourceTypes; [ binaryNativeCode ];
  };
}
